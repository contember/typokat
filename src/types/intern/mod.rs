//! The interner: hash-consing + canonicalization over the SoA `Store`.
//!
//! The dedup index maps structural hash to candidates, then confirms ties with
//! structural comparison. Intrinsics intern first, giving stable `WellKnown` ids.

mod composites;
mod operators;
mod snapshot;
#[cfg(test)]
mod tests;

use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::repr::{
    ConditionalType, IntrinsicKind, LiteralValue, MappedType, ObjectType, ParameterType,
    PropertyType, TypeFlags, TypeParamId, TypeParamType, TypeTag,
};
use crate::types::store::{Store, TypeId, TypeParamFreezeError};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(test)]
struct UserDeltaDropWitness {
    discarded: Arc<AtomicBool>,
}

#[cfg(test)]
impl Drop for UserDeltaDropWitness {
    fn drop(&mut self) {
        self.discarded.store(true, Ordering::Release);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservedTypeKind {
    Object,
    Conditional,
    Mapped,
}

#[derive(Clone, Debug)]
pub(crate) enum ReservedTypeFill {
    Object(TypeId, ObjectType),
    Conditional(TypeId, ConditionalType),
    Mapped(TypeId, MappedType),
}

impl ReservedTypeFill {
    fn target(&self) -> TypeId {
        match self {
            ReservedTypeFill::Object(id, _)
            | ReservedTypeFill::Conditional(id, _)
            | ReservedTypeFill::Mapped(id, _) => *id,
        }
    }

    fn kind(&self) -> ReservedTypeKind {
        match self {
            ReservedTypeFill::Object(_, _) => ReservedTypeKind::Object,
            ReservedTypeFill::Conditional(_, _) => ReservedTypeKind::Conditional,
            ReservedTypeFill::Mapped(_, _) => ReservedTypeKind::Mapped,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservedTypeFillError {
    Duplicate(TypeId),
    NotReserved(TypeId),
    KindMismatch {
        id: TypeId,
        reserved: ReservedTypeKind,
        supplied: ReservedTypeKind,
    },
    AlreadyFrozen(TypeId),
    InvalidBackingRow(TypeId),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservedObjectPromotionError {
    NotReserved(TypeId),
    KindMismatch(TypeId),
    NotFrozen(TypeId),
    InvalidBackingRow(TypeId),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ReservedTypeState {
    Pending,
    Frozen,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ReservedType {
    kind: ReservedTypeKind,
    state: ReservedTypeState,
}

/// Well-known intrinsic type ids. Because intrinsics are interned first and in a
/// fixed order, these ids are stable for the life of the process and can be used
/// as constants (e.g. relation-engine fast paths, the checker's annotation
/// lowering).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WellKnown {
    pub error: TypeId,
    pub any: TypeId,
    pub unknown: TypeId,
    pub never: TypeId,
    pub void: TypeId,
    pub null: TypeId,
    pub undefined: TypeId,
    pub boolean: TypeId,
    pub number: TypeId,
    pub string: TypeId,
    /// M28 string-intrinsic markers, used only as symbolic instantiation bases
    /// intercepted by evaluator identity checks.
    pub uppercase: TypeId,
    pub lowercase: TypeId,
    pub capitalize: TypeId,
    pub uncapitalize: TypeId,
    /// Backlog 70 contextual-`this` marker base. Its operand lives in a lazy
    /// instantiation so no context-only side table is needed.
    pub this_type: TypeId,
    /// Trusted `OmitThisParameter<T>` evaluator marker.
    pub omit_this_parameter: TypeId,
    /// The non-primitive `object` keyword.
    pub object: TypeId,
}

impl WellKnown {
    /// Whether `id` is one of the four M28 string-intrinsic markers.
    pub fn is_string_intrinsic_marker(&self, id: TypeId) -> bool {
        id == self.uppercase
            || id == self.lowercase
            || id == self.capitalize
            || id == self.uncapitalize
    }

    /// The operand of an exact `ThisType<T>` marker instantiation.
    pub fn this_type_operand(&self, store: &Store, id: TypeId) -> Option<TypeId> {
        let instantiation = store.instantiation_type(id)?;
        (instantiation.base == self.this_type && instantiation.args.len() == 1)
            .then(|| instantiation.args[0].1)
    }
}

pub struct Interner {
    store: Store,
    /// Immutable structural buckets owned by the sealed prefix.
    dedup_base: Arc<FxHashMap<u64, SmallVec<[TypeId; 2]>>>,
    /// Structural hash → interned candidates sharing that hash (architecture
    /// §3.3). `SmallVec` because collisions are rare; the common case is a
    /// 1-element bucket.
    dedup: FxHashMap<u64, SmallVec<[TypeId; 2]>>,
    /// Immutable nominal reservation terminals owned by the sealed prefix.
    reserved_types_base: Arc<FxHashMap<TypeId, ReservedType>>,
    /// Nominal placeholder rows are mutable exactly once, as one validated batch.
    reserved_types: FxHashMap<TypeId, ReservedType>,
    well_known: WellKnown,
    #[cfg(test)]
    user_delta_drop_witness: Option<UserDeltaDropWitness>,
}

impl Interner {
    /// Build an interner with the intrinsic types pre-interned in canonical
    /// order, returning it ready to use. The `WellKnown` table is filled as a
    /// side effect.
    pub fn with_intrinsics() -> Self {
        let mut interner = Interner {
            store: Store::new(),
            dedup_base: Arc::new(FxHashMap::default()),
            dedup: FxHashMap::default(),
            reserved_types_base: Arc::new(FxHashMap::default()),
            reserved_types: FxHashMap::default(),
            // Placeholder; overwritten below before any use.
            well_known: WellKnown {
                error: TypeId(0),
                any: TypeId(0),
                unknown: TypeId(0),
                never: TypeId(0),
                void: TypeId(0),
                null: TypeId(0),
                undefined: TypeId(0),
                boolean: TypeId(0),
                number: TypeId(0),
                string: TypeId(0),
                uppercase: TypeId(0),
                lowercase: TypeId(0),
                capitalize: TypeId(0),
                uncapitalize: TypeId(0),
                this_type: TypeId(0),
                omit_this_parameter: TypeId(0),
                object: TypeId(0),
            },
            #[cfg(test)]
            user_delta_drop_witness: None,
        };

        // Intern every intrinsic in the canonical order so ids are fixed.
        let mut ids = [TypeId(0); IntrinsicKind::ALL.len()];
        for (slot, kind) in IntrinsicKind::ALL.into_iter().enumerate() {
            ids[slot] = interner.intern_intrinsic(kind);
        }

        // Map kinds → ids positionally (ALL order is the source of truth).
        let id_of = |kind: IntrinsicKind| {
            let pos = IntrinsicKind::ALL
                .into_iter()
                .position(|k| k == kind)
                .expect("every IntrinsicKind is in ALL");
            ids[pos]
        };
        interner.well_known = WellKnown {
            error: id_of(IntrinsicKind::Error),
            any: id_of(IntrinsicKind::Any),
            unknown: id_of(IntrinsicKind::Unknown),
            never: id_of(IntrinsicKind::Never),
            void: id_of(IntrinsicKind::Void),
            null: id_of(IntrinsicKind::Null),
            undefined: id_of(IntrinsicKind::Undefined),
            boolean: id_of(IntrinsicKind::Boolean),
            number: id_of(IntrinsicKind::Number),
            string: id_of(IntrinsicKind::String),
            uppercase: id_of(IntrinsicKind::Uppercase),
            lowercase: id_of(IntrinsicKind::Lowercase),
            capitalize: id_of(IntrinsicKind::Capitalize),
            uncapitalize: id_of(IntrinsicKind::Uncapitalize),
            this_type: id_of(IntrinsicKind::ThisType),
            omit_this_parameter: id_of(IntrinsicKind::OmitThisParameter),
            object: id_of(IntrinsicKind::Object),
        };

        interner
    }

    /// Read-only access to the underlying store (the relation engine and
    /// renderer borrow it).
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Seal a complete standalone interner as the immutable shared prefix.
    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        if !self.dedup_base.is_empty() || !self.reserved_types_base.is_empty() {
            return Err("interner is already sealed");
        }
        if self
            .reserved_types
            .values()
            .any(|reserved| reserved.state == ReservedTypeState::Pending)
        {
            return Err("interner has pending reserved types");
        }
        self.store.freeze_as_base()?;
        self.dedup_base = Arc::new(std::mem::take(&mut self.dedup));
        self.reserved_types_base = Arc::new(std::mem::take(&mut self.reserved_types));
        Ok(())
    }

    /// Create an isolated interner suffix over this sealed prefix.
    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        if !self.dedup.is_empty() || !self.reserved_types.is_empty() {
            return Err("interner base has an unpublished suffix");
        }
        Ok(Self {
            store: self.store.fork_delta()?,
            dedup_base: Arc::clone(&self.dedup_base),
            dedup: FxHashMap::default(),
            reserved_types_base: Arc::clone(&self.reserved_types_base),
            reserved_types: FxHashMap::default(),
            well_known: self.well_known,
            #[cfg(test)]
            user_delta_drop_witness: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn install_user_delta_drop_witness_for_test(&mut self, discarded: Arc<AtomicBool>) {
        self.user_delta_drop_witness = Some(UserDeltaDropWitness { discarded });
    }

    fn reserved_type(&self, id: TypeId) -> Option<ReservedType> {
        self.reserved_types_base
            .get(&id)
            .or_else(|| self.reserved_types.get(&id))
            .copied()
    }

    fn contains_reserved_type(&self, id: TypeId) -> bool {
        self.reserved_types_base.contains_key(&id) || self.reserved_types.contains_key(&id)
    }

    fn dedup_buckets(&self) -> impl Iterator<Item = (&u64, &SmallVec<[TypeId; 2]>)> {
        self.dedup_base.iter().chain(self.dedup.iter())
    }

    #[cfg(test)]
    pub(crate) fn frozen_structural_object_probe_for_test(&self) -> Option<(TypeId, ObjectType)> {
        let id = self
            .dedup_base
            .values()
            .flatten()
            .copied()
            .filter(|id| self.store.tag(*id) == TypeTag::Object)
            .min_by_key(|id| id.0)?;
        Some((id, self.store.object_type(id)?.clone()))
    }

    #[cfg(test)]
    pub(crate) fn base_index_family_sharing_with(&self, other: &Self) -> [bool; 3] {
        [
            Arc::ptr_eq(&self.dedup_base, &other.dedup_base),
            Arc::ptr_eq(&self.reserved_types_base, &other.reserved_types_base),
            self.well_known == other.well_known,
        ]
    }

    #[cfg(test)]
    pub(crate) fn local_index_row_counts_for_test(&self) -> [usize; 3] {
        [
            self.dedup.values().map(SmallVec::len).sum(),
            self.reserved_types.len(),
            0,
        ]
    }

    fn reserved_types(&self) -> impl Iterator<Item = (&TypeId, &ReservedType)> {
        self.reserved_types_base
            .iter()
            .chain(self.reserved_types.iter())
    }

    fn has_nonempty_delta(&self) -> bool {
        self.store.has_nonempty_delta()
            || (self.store.is_sealed_base()
                && (!self.dedup.is_empty() || !self.reserved_types.is_empty()))
    }

    #[cfg(test)]
    pub(crate) fn shares_base_indexes_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.dedup_base, &other.dedup_base)
            && Arc::ptr_eq(&self.reserved_types_base, &other.reserved_types_base)
    }

    fn register_reserved_type(&mut self, id: TypeId, kind: ReservedTypeKind) -> TypeId {
        let previous = self.reserved_types.insert(
            id,
            ReservedType {
                kind,
                state: ReservedTypeState::Pending,
            },
        );
        debug_assert!(
            previous.is_none(),
            "new type ids cannot already be reserved"
        );
        id
    }

    /// Atomically fill and freeze a complete recursive publication batch.
    pub(crate) fn fill_reserved_type_batch(
        &mut self,
        mut fills: Vec<ReservedTypeFill>,
    ) -> Result<(), ReservedTypeFillError> {
        let mut targets = FxHashSet::default();
        for fill in &fills {
            let id = fill.target();
            if !targets.insert(id) {
                return Err(ReservedTypeFillError::Duplicate(id));
            }
        }

        for fill in &fills {
            let id = fill.target();
            let Some(reserved) = self.reserved_type(id) else {
                return Err(ReservedTypeFillError::NotReserved(id));
            };
            if reserved.kind != fill.kind() {
                return Err(ReservedTypeFillError::KindMismatch {
                    id,
                    reserved: reserved.kind,
                    supplied: fill.kind(),
                });
            }
            if reserved.state == ReservedTypeState::Frozen {
                return Err(ReservedTypeFillError::AlreadyFrozen(id));
            }
            let valid_backing_row = match reserved.kind {
                ReservedTypeKind::Object => self.store.object_type(id).is_some(),
                ReservedTypeKind::Conditional => self.store.conditional_type(id).is_some(),
                ReservedTypeKind::Mapped => self.store.mapped_type(id).is_some(),
            };
            if !valid_backing_row {
                return Err(ReservedTypeFillError::InvalidBackingRow(id));
            }
        }

        for fill in &mut fills {
            if let ReservedTypeFill::Object(_, object) = fill {
                object.properties.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }

        // Every fallible check is complete. The exclusive interner borrow keeps the
        // sequential writes unobservable until the whole batch is frozen.
        if !fills.is_empty() {
            self.store.mark_semantic_graph_mutation();
        }
        for fill in fills {
            let id = fill.target();
            match fill {
                ReservedTypeFill::Object(_, object) => self.store.set_object(id, object),
                ReservedTypeFill::Conditional(_, conditional) => {
                    self.store.set_conditional(id, conditional);
                }
                ReservedTypeFill::Mapped(_, mapped) => self.store.set_mapped(id, mapped),
            }
            self.reserved_types
                .get_mut(&id)
                .expect("validated reservation must remain registered")
                .state = ReservedTypeState::Frozen;
        }
        Ok(())
    }

    /// Record a type parameter's `extends` constraint (M24) in the store-side
    /// constraint column, keyed by [`TypeParamId`]. The checker calls this while
    /// lowering a generic declaration's parameter list (frame active); the constraint
    /// is a side column, not folded into the interned `TypeParamType` identity.
    pub fn set_type_param_constraint(&mut self, id: TypeParamId, constraint: TypeId) -> bool {
        self.store.set_type_param_constraint(id, constraint)
    }

    /// Erase a type parameter's constraint (M24 — the `TK2313` circularity fix). A
    /// circular parameter records **no** constraint; see
    /// `Store::remove_type_param_constraint`.
    pub fn remove_type_param_constraint(&mut self, id: TypeParamId) -> bool {
        self.store.remove_type_param_constraint(id)
    }

    pub(crate) fn type_param_metadata_is_frozen(&self, id: TypeParamId) -> bool {
        self.store.type_param_metadata_is_frozen(id)
    }

    pub(crate) fn freeze_type_param_metadata(
        &mut self,
        ids: &[TypeParamId],
    ) -> Result<(), TypeParamFreezeError> {
        self.store.freeze_type_param_metadata(ids)
    }

    /// Record a reserved template row's alias display name (M28 round 3) — a
    /// rendering-only side column, never part of identity. The checker calls this
    /// right after `reserve_conditional`/`reserve_mapped` for a named alias, so a
    /// deferred instantiation renders as `Extract<K, string>` instead of the raw body.
    pub fn set_template_name(&mut self, id: TypeId, name: impl Into<String>) {
        let Some(reserved) = self.reserved_types.get(&id) else {
            return;
        };
        if !matches!(
            reserved.kind,
            ReservedTypeKind::Conditional | ReservedTypeKind::Mapped
        ) {
            return;
        }
        self.store.set_template_name(id, name.into());
    }

    pub fn well_known(&self) -> WellKnown {
        self.well_known
    }

    /// Intern an intrinsic type, returning the shared id.
    pub fn intern_intrinsic(&mut self, kind: IntrinsicKind) -> TypeId {
        let key = StructuralKey::Intrinsic(kind);
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.intrinsic_kind(id) == Some(kind))
        {
            return existing;
        }
        // The error type is the only intrinsic that carries a flag.
        let flags = if kind == IntrinsicKind::Error {
            TypeFlags::CONTAINS_ERROR
        } else {
            TypeFlags::EMPTY
        };
        let id = self.store.push_intrinsic(kind, flags);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a literal type, returning the shared id.
    pub fn intern_literal(&mut self, value: LiteralValue) -> TypeId {
        let key = StructuralKey::Literal(&value);
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.literal_value(id) == Some(&value))
        {
            return existing;
        }
        let id = self.store.push_literal(value, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a type-parameter type. Identity is the declaration-site
    /// [`TypeParamId`]; the source name is rendering-only.
    pub fn intern_type_param(&mut self, id: TypeParamId, name: impl Into<String>) -> TypeId {
        let key = StructuralKey::TypeParam(id);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, ty| {
            store.type_param(ty).map(|p| p.id) == Some(id)
        }) {
            return existing;
        }
        let interned = self.store.push_type_param(
            TypeParamType {
                id,
                name: name.into(),
            },
            TypeFlags::EMPTY,
        );
        self.dedup.entry(hash).or_default().push(interned);
        interned
    }

    /// Intern an **`infer` binder** at the given de Bruijn index (M25). Identity is the
    /// index alone, so alpha-equivalent conditionals hash-cons to one node.
    pub fn intern_infer(&mut self, index: u32) -> TypeId {
        let key = StructuralKey::Infer(index);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| store.infer_index(id) == Some(index))
        {
            return existing;
        }
        let id = self.store.push_infer(index, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Look up an existing id in the dedup bucket for `hash`, accepting the first
    /// candidate for which `eq` confirms a real structural match.
    fn lookup(&self, hash: u64, eq: impl Fn(&Store, TypeId) -> bool) -> Option<TypeId> {
        self.dedup_base
            .get(&hash)
            .into_iter()
            .chain(self.dedup.get(&hash))
            .flat_map(|bucket| bucket.iter().copied())
            .find(|&id| eq(&self.store, id))
    }
}

/// Structural equality of two **canonical** (name-sorted) property lists — the
/// dedup-bucket tie-break for object types. Property types compare by `TypeId`,
/// which is itself canonical thanks to hash-consing, so nested object equality is
/// decided cheaply by id without recursing.
fn object_props_eq(a: &[PropertyType], b: &[PropertyType]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            // Match every identity-bearing property field; see `PropertyType`.
            x.name == y.name
                && x.optional == y.optional
                && x.ty == y.ty
                && x.write_ty == y.write_ty
                && x.visibility == y.visibility
                && x.declaring_class == y.declaring_class
                && x.readonly == y.readonly
                && x.is_accessor == y.is_accessor
        })
}

/// Positional equality of two parameter lists — the dedup-bucket tie-break for
/// function types. Parameters are compared in order (not sorted); types compare
/// by `TypeId` (canonical via hash-consing), so nested function/object equality
/// is decided by id without recursing.
fn function_params_eq(a: &[ParameterType], b: &[ParameterType]) -> bool {
    a == b
}

impl TypeTag {
    /// Convenience for assertions/tests: whether this tag is a cold side-table
    /// tag (vs the inline `Intrinsic`). Currently informational.
    #[allow(dead_code)] // TODO(M2+): used once side-table tags are constructed.
    pub fn has_side_table(self) -> bool {
        !matches!(self, TypeTag::Intrinsic)
    }
}
