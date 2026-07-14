//! The interner: hash-consing + canonicalization over the SoA `Store`.
//!
//! The dedup index maps structural hash to candidates, then confirms ties with
//! structural comparison. Intrinsics intern first, giving stable `WellKnown` ids.

mod composites;
mod operators;
#[cfg(test)]
mod tests;

use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::repr::{
    IntrinsicKind, LiteralValue, ParameterType, PropertyType, TypeFlags, TypeParamId,
    TypeParamType, TypeTag,
};
use crate::types::store::{Store, TypeId, TypeParamFreezeError};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Well-known intrinsic type ids. Because intrinsics are interned first and in a
/// fixed order, these ids are stable for the life of the process and can be used
/// as constants (e.g. relation-engine fast paths, the checker's annotation
/// lowering).
#[derive(Copy, Clone, Debug)]
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
    /// Structural hash → interned candidates sharing that hash (architecture
    /// §3.3). `SmallVec` because collisions are rare; the common case is a
    /// 1-element bucket.
    dedup: FxHashMap<u64, SmallVec<[TypeId; 2]>>,
    well_known: WellKnown,
}

impl Interner {
    /// Build an interner with the intrinsic types pre-interned in canonical
    /// order, returning it ready to use. The `WellKnown` table is filled as a
    /// side effect.
    pub fn with_intrinsics() -> Self {
        let mut interner = Interner {
            store: Store::new(),
            dedup: FxHashMap::default(),
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
            },
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
        };

        interner
    }

    /// Read-only access to the underlying store (the relation engine and
    /// renderer borrow it).
    pub fn store(&self) -> &Store {
        &self.store
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
        let bucket = self.dedup.get(&hash)?;
        bucket.iter().copied().find(|&id| eq(&self.store, id))
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
