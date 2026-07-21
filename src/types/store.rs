//! Append-only struct-of-arrays type arena keyed by `TypeId(u32)`.
//!
//! Hot columns are indexed directly by `TypeId`; variable-size data lives in
//! tag-selected side tables. `TypeId`s stay stable for the process lifetime.

#[cfg(test)]
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader, SnapshotWriter};
use crate::types::hash::StableHash;
use crate::types::repr::{
    ArrayType, ClassInstanceType, ConditionalType, DeferredIndexedAccessType, FunctionType,
    InstantiationType, IntrinsicKind, LiteralValue, MappedType, ObjectType, TemplateType,
    TupleType, TypeFlags, TypeParamId, TypeParamType, TypeTag,
};
#[cfg(test)]
use crate::types::repr::{
    GenericTypeParam, ModifierOp, ParameterType, PropertyType, TupleRestType, Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// A run-local handle to a type: an index into the SoA arena. Cheap to copy and
/// compare; structural equality of two interned types is `a == b`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TypeId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeParamFreezeError {
    Duplicate(TypeParamId),
    AlreadyFrozen(TypeParamId),
}

impl TypeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The SoA arena. Hot columns first, cold side-tables after.
#[derive(Default)]
pub struct Store {
    // --- hot, parallel, indexed by TypeId ---
    tag: Vec<TypeTag>,
    flags: Vec<TypeFlags>,
    /// For `Intrinsic`: the `IntrinsicKind` discriminant. Otherwise: an index
    /// into the cold side-table selected by `tag`.
    payload: Vec<u32>,

    // --- cold side-tables ---
    literals: Vec<LiteralValue>,
    /// Object types (M2). Addressed by the `payload` of an `Object`-tagged row.
    objects: Vec<ObjectType>,
    /// Union members (M4), already canonicalized: flattened, sorted by `TypeId`,
    /// deduped, with `never` dropped (mvp-plan §3.3). Addressed by the `payload`
    /// of a `Union`-tagged row. A union row always has at least two members — the
    /// 0- and 1-member cases collapse in the interner and never reach the store.
    unions: Vec<Box<[TypeId]>>,
    /// Intersection members (M31), already canonicalized by the interner. Like a
    /// union row, always at least two members.
    intersections: Vec<Box<[TypeId]>>,
    /// Function types (M3). Addressed by the `payload` of a `Function`-tagged row.
    functions: Vec<FunctionType>,
    /// Type-parameter types (M9). Addressed by the `payload` of a
    /// `TypeParam`-tagged row. Each entry's identity is its `TypeParamId`.
    type_params: Vec<TypeParamType>,
    /// Array types (M17). Addressed by the `payload` of an `Array`-tagged row. Each
    /// entry's identity is its element `TypeId` (so `number[]` hash-conses to one id
    /// and `number[]` ≠ `string[]`).
    arrays: Vec<ArrayType>,
    /// Tuple types (M18, rest-shape expanded in M32/WU2). Addressed by the
    /// `payload` of a `Tuple`-tagged row. Each entry's identity is its ordered
    /// fixed element list plus any rest position/type.
    tuples: Vec<TupleType>,
    /// Conditional types (M25). Addressed by the `payload` of a `Conditional`-tagged
    /// row. Field order is significant (position is meaning); a recursive alias
    /// template row is reserved empty and filled in place (`fill_conditional`), like a
    /// nominal object.
    conditionals: Vec<ConditionalType>,
    /// Lazy alias instantiations (M25). Addressed by the `payload` of an
    /// `Instantiation`-tagged row. Identity is `(base, sorted args)`.
    instantiations: Vec<InstantiationType>,
    /// Immutable class applications, distinct from lazy alias instantiations.
    class_instances: Vec<ClassInstanceType>,
    /// Immutable deferred indexed-access operand pairs.
    deferred_indexed_accesses: Vec<DeferredIndexedAccessType>,
    /// Mapped types (M26). Addressed by the `payload` of a `Mapped`-tagged row. The
    /// whole [`MappedType`] is its structural identity. A `MappedValue` placeholder row
    /// carries no side-table entry (payload `0`), like an intrinsic.
    mapped: Vec<MappedType>,
    /// Template literal types (M27). Addressed by the `payload` of a `Template`-tagged
    /// row. The whole [`TemplateType`] (its text segments + hole ids) is its structural
    /// identity.
    templates: Vec<TemplateType>,

    /// **Type-parameter constraint column** (M24): a type parameter's `extends`
    /// bound, keyed by its [`TypeParamId`]. A **side column**, NOT part of the
    /// interned `TypeParamType` identity (the ids are already unique per declaration,
    /// and folding the constraint in would only churn identity and complicate the
    /// de Bruijn re-key scheduled for the conditional-types milestone — invariants §2).
    /// Populated by the checker when lowering each generic declaration's parameter
    /// list (with the parameter frame active, so `<T, U extends T>` resolves), and
    /// read by both the checker (apparent type + `TK2344`) and the relation engine
    /// (`TypeParam(T) → X` via its constraint). A parameter with no `extends`, or an
    /// unlowerable one, simply has no entry (no constraint — the safe direction).
    type_param_constraints: FxHashMap<TypeParamId, TypeId>,

    /// Declaration binders whose side-column metadata is immutable. Class
    /// publication freezes the complete SCC batch before any surface is exposed.
    frozen_type_params: FxHashSet<TypeParamId>,

    /// Template display names (M28 round 3), keyed by reserved template id.
    /// Rendering-only: deferred instantiations print as alias names, not raw bodies.
    template_names: FxHashMap<TypeId, String>,

    /// Reserved cross-run identity column (architecture §3.2). NOT populated in
    /// the MVP (mvp-plan §7.1) — kept so Phase 4 can fill it at intern time
    /// without changing the arena shape.
    #[allow(dead_code)] // TODO(Phase 4): populate alongside each push.
    stable_hash: Vec<StableHash>,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    /// Number of interned types.
    pub fn len(&self) -> usize {
        self.tag.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tag.is_empty()
    }

    #[inline]
    pub fn tag(&self, id: TypeId) -> TypeTag {
        self.tag[id.index()]
    }

    #[inline]
    pub fn flags(&self, id: TypeId) -> TypeFlags {
        self.flags[id.index()]
    }

    #[inline]
    fn payload(&self, id: TypeId) -> u32 {
        self.payload[id.index()]
    }

    /// The `IntrinsicKind` of an intrinsic type, or `None` if `id` is not an
    /// intrinsic. Reconstructed from the `payload` discriminant.
    pub fn intrinsic_kind(&self, id: TypeId) -> Option<IntrinsicKind> {
        if self.tag(id) != TypeTag::Intrinsic {
            return None;
        }
        let raw = self.payload(id);
        // The payload is exactly `IntrinsicKind as u32`; map it back via the
        // canonical list so the match stays exhaustive and panic-free.
        IntrinsicKind::ALL.into_iter().find(|k| *k as u32 == raw)
    }

    /// The `LiteralValue` of a literal type, or `None` if `id` is not a literal.
    pub fn literal_value(&self, id: TypeId) -> Option<&LiteralValue> {
        if self.tag(id) != TypeTag::Literal {
            return None;
        }
        self.literals.get(self.payload(id) as usize)
    }

    /// The `ObjectType` of an object type, or `None` if `id` is not an object.
    pub fn object_type(&self, id: TypeId) -> Option<&ObjectType> {
        if self.tag(id) != TypeTag::Object {
            return None;
        }
        self.objects.get(self.payload(id) as usize)
    }

    /// The `FunctionType` of a function type, or `None` if `id` is not a function.
    pub fn function_type(&self, id: TypeId) -> Option<&FunctionType> {
        if self.tag(id) != TypeTag::Function {
            return None;
        }
        self.functions.get(self.payload(id) as usize)
    }

    /// The `TypeParamType` of a type-parameter type, or `None` if `id` is not a
    /// type parameter (M9).
    pub fn type_param(&self, id: TypeId) -> Option<&TypeParamType> {
        if self.tag(id) != TypeTag::TypeParam {
            return None;
        }
        self.type_params.get(self.payload(id) as usize)
    }

    /// The display name of a declared type parameter by its stable id. Generic
    /// signature binders keep ids in their structural representation while names
    /// remain rendering-only on the interned parameter node.
    pub fn type_param_name(&self, id: TypeParamId) -> Option<&str> {
        self.type_params
            .iter()
            .find(|param| param.id == id)
            .map(|param| param.name.as_str())
    }

    /// The `ArrayType` (its element id) of an array type, or `None` if `id` is not
    /// an array (M17).
    pub fn array_type(&self, id: TypeId) -> Option<&ArrayType> {
        if self.tag(id) != TypeTag::Array {
            return None;
        }
        self.arrays.get(self.payload(id) as usize)
    }

    /// The `TupleType` of a tuple type, or `None` if `id` is not a tuple (M18).
    pub fn tuple_type(&self, id: TypeId) -> Option<&TupleType> {
        if self.tag(id) != TypeTag::Tuple {
            return None;
        }
        self.tuples.get(self.payload(id) as usize)
    }

    /// The wrapped array/tuple operand of a readonly wrapper, or `None` if `id` is
    /// not a readonly node. The operand id is stored inline in the payload.
    pub fn readonly_operand(&self, id: TypeId) -> Option<TypeId> {
        if self.tag(id) != TypeTag::Readonly {
            return None;
        }
        Some(TypeId(self.payload(id)))
    }

    /// The `ConditionalType` of a conditional type (M25), or `None` if `id` is not a
    /// conditional.
    pub fn conditional_type(&self, id: TypeId) -> Option<&ConditionalType> {
        if self.tag(id) != TypeTag::Conditional {
            return None;
        }
        self.conditionals.get(self.payload(id) as usize)
    }

    /// The `InstantiationType` of a lazy instantiation (M25), or `None` if `id` is not
    /// an instantiation.
    pub fn instantiation_type(&self, id: TypeId) -> Option<&InstantiationType> {
        if self.tag(id) != TypeTag::Instantiation {
            return None;
        }
        self.instantiations.get(self.payload(id) as usize)
    }

    /// The immutable class application payload, or `None` for another tag.
    pub fn class_instance_type(&self, id: TypeId) -> Option<&ClassInstanceType> {
        if self.tag(id) != TypeTag::ClassInstance {
            return None;
        }
        self.class_instances
            .get(usize::try_from(self.payload(id)).ok()?)
    }

    /// The deferred indexed-access operands, or `None` for another tag.
    pub fn deferred_indexed_access_type(&self, id: TypeId) -> Option<&DeferredIndexedAccessType> {
        if self.tag(id) != TypeTag::DeferredIndexedAccess {
            return None;
        }
        self.deferred_indexed_accesses
            .get(usize::try_from(self.payload(id)).ok()?)
    }

    /// The de Bruijn index of an `infer` binder (M25), or `None` if `id` is not one.
    /// The index is reconstructed from the `payload` (stored inline like an intrinsic
    /// kind).
    pub fn infer_index(&self, id: TypeId) -> Option<u32> {
        if self.tag(id) != TypeTag::Infer {
            return None;
        }
        Some(self.payload(id))
    }

    /// The `MappedType` of a mapped type (M26), or `None` if `id` is not a mapped type.
    pub fn mapped_type(&self, id: TypeId) -> Option<&MappedType> {
        if self.tag(id) != TypeTag::Mapped {
            return None;
        }
        self.mapped.get(self.payload(id) as usize)
    }

    /// The `TemplateType` of a template literal type (M27), or `None` if `id` is not a
    /// template.
    pub fn template_type(&self, id: TypeId) -> Option<&TemplateType> {
        if self.tag(id) != TypeTag::Template {
            return None;
        }
        self.templates.get(self.payload(id) as usize)
    }

    /// The operand of a deferred `keyof` (M28), or `None` if `id` is not one. The
    /// operand id is reconstructed from the `payload` (stored inline, like an `Infer`
    /// index).
    pub fn keyof_operand(&self, id: TypeId) -> Option<TypeId> {
        if self.tag(id) != TypeTag::Keyof {
            return None;
        }
        Some(TypeId(self.payload(id)))
    }

    /// The `extends` constraint of a type parameter (M24), or `None` if the
    /// parameter is unconstrained. Keyed by [`TypeParamId`] — the side column, not the
    /// interned type's identity. Read by the checker (apparent type / `TK2344`) and the
    /// relation engine (`TypeParam(T) → constraint`).
    pub fn type_param_constraint(&self, id: TypeParamId) -> Option<TypeId> {
        self.type_param_constraints.get(&id).copied()
    }

    /// Record a type parameter's `extends` constraint (M24). Internal — the checker
    /// calls it through `Interner::set_type_param_constraint` once, when the
    /// declaration's parameter list is lowered with the frame active.
    pub(crate) fn set_type_param_constraint(
        &mut self,
        id: TypeParamId,
        constraint: TypeId,
    ) -> bool {
        if self.frozen_type_params.contains(&id) {
            return false;
        }
        self.type_param_constraints.insert(id, constraint);
        true
    }

    /// Erase a circular type-parameter constraint so the degenerate cycle never
    /// reaches the relation engine's assume-true stack.
    pub(crate) fn remove_type_param_constraint(&mut self, id: TypeParamId) -> bool {
        if self.frozen_type_params.contains(&id) {
            return false;
        }
        self.type_param_constraints.remove(&id);
        true
    }

    pub(crate) fn type_param_metadata_is_frozen(&self, id: TypeParamId) -> bool {
        self.frozen_type_params.contains(&id)
    }

    /// Freeze a whole declaration batch after prevalidation. A failed batch
    /// changes nothing, so publication cannot expose a partially frozen SCC.
    pub(crate) fn freeze_type_param_metadata(
        &mut self,
        ids: &[TypeParamId],
    ) -> Result<(), TypeParamFreezeError> {
        let mut batch = FxHashSet::default();
        if let Some(&id) = ids.iter().find(|id| !batch.insert(**id)) {
            return Err(TypeParamFreezeError::Duplicate(id));
        }
        if let Some(&id) = ids.iter().find(|id| self.frozen_type_params.contains(id)) {
            return Err(TypeParamFreezeError::AlreadyFrozen(id));
        }
        self.frozen_type_params.extend(ids.iter().copied());
        Ok(())
    }

    /// The display name of a reserved template row (M28 round 3), or `None` for an
    /// unnamed/ordinary type. Rendering-only — see the `template_names` column.
    pub fn template_name(&self, id: TypeId) -> Option<&str> {
        self.template_names.get(&id).map(String::as_str)
    }

    /// Record a reserved template row's display name (M28 round 3). Internal — the
    /// checker calls it through `Interner::set_template_name` at reserve time.
    pub(crate) fn set_template_name(&mut self, id: TypeId, name: String) {
        self.template_names.insert(id, name);
    }

    /// The members of a union type (canonical: flattened, sorted by `TypeId`,
    /// deduped, `never`-free), or `None` if `id` is not a union.
    pub fn union_members(&self, id: TypeId) -> Option<&[TypeId]> {
        if self.tag(id) != TypeTag::Union {
            return None;
        }
        self.unions.get(self.payload(id) as usize).map(|m| &m[..])
    }

    /// The members of an intersection type (canonical: flattened, sorted by
    /// `TypeId`, deduped, `unknown`-free), or `None` if `id` is not an intersection
    /// (M31).
    pub fn intersection_members(&self, id: TypeId) -> Option<&[TypeId]> {
        if self.tag(id) != TypeTag::Intersection {
            return None;
        }
        self.intersections
            .get(self.payload(id) as usize)
            .map(|m| &m[..])
    }

    // --- raw append helpers (used only by the interner) ---

    /// Push a row with the given hot attributes and return its id. Internal;
    /// callers go through `Interner` so hash-consing is never bypassed.
    fn push(&mut self, tag: TypeTag, flags: TypeFlags, payload: u32) -> TypeId {
        let id = TypeId(self.tag.len() as u32);
        self.tag.push(tag);
        self.flags.push(flags);
        self.payload.push(payload);
        // Keep the reserved column length-aligned even though it is unread.
        self.stable_hash.push(StableHash::default());
        id
    }

    /// Append an intrinsic row. Internal — `Interner` owns dedup.
    pub(crate) fn push_intrinsic(&mut self, kind: IntrinsicKind, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Intrinsic, flags, kind as u32)
    }

    /// Append a literal row (value into the side-table, index into payload).
    /// Internal — `Interner` owns dedup.
    pub(crate) fn push_literal(&mut self, value: LiteralValue, flags: TypeFlags) -> TypeId {
        let payload = self.literals.len() as u32;
        self.literals.push(value);
        self.push(TypeTag::Literal, flags, payload)
    }

    /// Append an object row (object into the side-table, index into payload).
    /// The caller (`Interner`) passes an already-canonicalized `ObjectType` and
    /// owns dedup.
    pub(crate) fn push_object(&mut self, object: ObjectType, flags: TypeFlags) -> TypeId {
        let payload = self.objects.len() as u32;
        self.objects.push(object);
        self.push(TypeTag::Object, flags, payload)
    }

    /// Replace a prevalidated reserved object body while preserving its `TypeId`.
    pub(super) fn set_object(&mut self, id: TypeId, object: ObjectType) {
        assert_eq!(self.tag(id), TypeTag::Object);
        let payload = self.payload(id) as usize;
        let slot = self
            .objects
            .get_mut(payload)
            .expect("validated object row must have a side-table entry");
        *slot = object;
    }

    /// Append a function row (function into the side-table, index into payload).
    /// Internal — `Interner` owns dedup. Parameters are stored positionally (the
    /// caller does not sort them).
    pub(crate) fn push_function(&mut self, function: FunctionType, flags: TypeFlags) -> TypeId {
        let payload = self.functions.len() as u32;
        self.functions.push(function);
        self.push(TypeTag::Function, flags, payload)
    }

    /// Append a union row (members into the side-table, index into payload).
    /// Internal — `Interner` owns canonicalization and dedup; the caller passes
    /// an already-canonical (flattened, sorted, deduped, `never`-free) member
    /// slice of length ≥ 2.
    pub(crate) fn push_union(&mut self, members: Box<[TypeId]>, flags: TypeFlags) -> TypeId {
        let payload = self.unions.len() as u32;
        self.unions.push(members);
        self.push(TypeTag::Union, flags, payload)
    }

    /// Append an intersection row (members into the side-table, index into
    /// payload). Internal — `Interner` owns canonicalization and dedup; the caller
    /// passes an already-canonical (flattened, sorted, deduped, `unknown`-free)
    /// member slice of length ≥ 2 (M31).
    pub(crate) fn push_intersection(&mut self, members: Box<[TypeId]>, flags: TypeFlags) -> TypeId {
        let payload = self.intersections.len() as u32;
        self.intersections.push(members);
        self.push(TypeTag::Intersection, flags, payload)
    }

    /// Append a type-parameter row (M9). Internal — `Interner` owns dedup (by
    /// `TypeParamId`).
    pub(crate) fn push_type_param(&mut self, param: TypeParamType, flags: TypeFlags) -> TypeId {
        let payload = self.type_params.len() as u32;
        self.type_params.push(param);
        self.push(TypeTag::TypeParam, flags, payload)
    }

    /// Append an array row (M17). Internal — `Interner` owns dedup (by element id).
    pub(crate) fn push_array(&mut self, array: ArrayType, flags: TypeFlags) -> TypeId {
        let payload = self.arrays.len() as u32;
        self.arrays.push(array);
        self.push(TypeTag::Array, flags, payload)
    }

    /// Append a tuple row (M18). Internal — `Interner` owns dedup (by the ordered
    /// element list). The caller passes elements in source order (never sorted).
    pub(crate) fn push_tuple(&mut self, tuple: TupleType, flags: TypeFlags) -> TypeId {
        let payload = self.tuples.len() as u32;
        self.tuples.push(tuple);
        self.push(TypeTag::Tuple, flags, payload)
    }

    /// Append a readonly wrapper row. Internal — `Interner` owns dedup by operand.
    /// The wrapped array/tuple id is stored inline in `payload`.
    pub(crate) fn push_readonly(&mut self, operand: TypeId, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Readonly, flags, operand.0)
    }

    /// Append a conditional row (M25). Internal — `Interner` owns dedup (by all four
    /// component ids + `infer_count` + `distributive`, in order).
    pub(crate) fn push_conditional(
        &mut self,
        conditional: ConditionalType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = self.conditionals.len() as u32;
        self.conditionals.push(conditional);
        self.push(TypeTag::Conditional, flags, payload)
    }

    /// Replace a prevalidated reserved conditional body while preserving its `TypeId`.
    pub(super) fn set_conditional(&mut self, id: TypeId, conditional: ConditionalType) {
        assert_eq!(self.tag(id), TypeTag::Conditional);
        let payload = self.payload(id) as usize;
        let slot = self
            .conditionals
            .get_mut(payload)
            .expect("validated conditional row must have a side-table entry");
        *slot = conditional;
    }

    /// Append an instantiation row (M25). Internal — `Interner` owns dedup.
    pub(crate) fn push_instantiation(
        &mut self,
        instantiation: InstantiationType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = self.instantiations.len() as u32;
        self.instantiations.push(instantiation);
        self.push(TypeTag::Instantiation, flags, payload)
    }

    /// Append an immutable class application. Internal — `Interner` owns dedup.
    pub(crate) fn push_class_instance(
        &mut self,
        instance: ClassInstanceType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = u32::try_from(self.class_instances.len())
            .expect("class-instance side table exceeds the u32 payload range");
        self.class_instances.push(instance);
        self.push(TypeTag::ClassInstance, flags, payload)
    }

    /// Append an immutable deferred indexed access. Internal — `Interner` owns dedup.
    pub(crate) fn push_deferred_indexed_access(
        &mut self,
        access: DeferredIndexedAccessType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = u32::try_from(self.deferred_indexed_accesses.len())
            .expect("deferred-indexed-access side table exceeds the u32 payload range");
        self.deferred_indexed_accesses.push(access);
        self.push(TypeTag::DeferredIndexedAccess, flags, payload)
    }

    /// Append an infer-binder row (M25). Internal — `Interner` owns dedup (by index).
    /// The de Bruijn index is stored inline in `payload`.
    pub(crate) fn push_infer(&mut self, index: u32, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Infer, flags, index)
    }

    /// Append a mapped-type row (M26). Internal — `Interner` owns dedup (by the whole
    /// [`MappedType`]).
    pub(crate) fn push_mapped(&mut self, mapped: MappedType, flags: TypeFlags) -> TypeId {
        let payload = self.mapped.len() as u32;
        self.mapped.push(mapped);
        self.push(TypeTag::Mapped, flags, payload)
    }

    /// Replace a prevalidated reserved mapped body while preserving its `TypeId`.
    pub(super) fn set_mapped(&mut self, id: TypeId, mapped: MappedType) {
        assert_eq!(self.tag(id), TypeTag::Mapped);
        let payload = self.payload(id) as usize;
        let slot = self
            .mapped
            .get_mut(payload)
            .expect("validated mapped row must have a side-table entry");
        *slot = mapped;
    }

    /// Append a deferred-`keyof` row (M28). Internal — `Interner` owns dedup (by
    /// operand). The operand id is stored inline in `payload`.
    pub(crate) fn push_keyof(&mut self, operand: TypeId, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Keyof, flags, operand.0)
    }

    /// Append a mapped-value placeholder row (M26). Internal — `Interner` owns dedup
    /// (identity is the tag alone; payload `0`).
    pub(crate) fn push_mapped_value(&mut self, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::MappedValue, flags, 0)
    }

    /// Append a template-literal row (M27). Internal — `Interner` owns dedup (by the
    /// whole [`TemplateType`]).
    pub(crate) fn push_template(&mut self, template: TemplateType, flags: TypeFlags) -> TypeId {
        let payload = self.templates.len() as u32;
        self.templates.push(template);
        self.push(TypeTag::Template, flags, payload)
    }
}

#[cfg(test)]
impl Store {
    pub(crate) fn snapshot_template_name_ids_for_test(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.template_names.keys().copied()
    }

    pub(crate) fn write_snapshot_for_test(
        &self,
        writer: &mut SnapshotWriter,
    ) -> Result<(), SnapshotCodecError> {
        writer.u32(1);
        writer.usize(self.tag.len())?;
        for index in 0..self.tag.len() {
            writer.u8(self.tag[index].discriminant());
            writer.u32(self.flags[index].0);
            writer.u32(self.payload[index]);
            writer.raw(&self.stable_hash[index].0);
        }

        writer.usize(self.literals.len())?;
        for literal in &self.literals {
            match literal {
                LiteralValue::Number(value) => {
                    writer.u8(0);
                    writer.u64(value.to_bits());
                }
                LiteralValue::String(value) => {
                    writer.u8(1);
                    writer.string(value)?;
                }
                LiteralValue::Boolean(value) => {
                    writer.u8(2);
                    writer.bool(*value);
                }
            }
        }

        writer.usize(self.objects.len())?;
        for object in &self.objects {
            write_object(writer, object)?;
        }
        write_type_id_slices(writer, &self.unions)?;
        write_type_id_slices(writer, &self.intersections)?;

        writer.usize(self.functions.len())?;
        for function in &self.functions {
            write_function(writer, function)?;
        }

        writer.usize(self.type_params.len())?;
        for parameter in &self.type_params {
            writer.u32(parameter.id.0);
            writer.string(&parameter.name)?;
        }

        writer.usize(self.arrays.len())?;
        for array in &self.arrays {
            writer.u32(array.element.0);
        }

        writer.usize(self.tuples.len())?;
        for tuple in &self.tuples {
            write_type_ids(writer, &tuple.elements)?;
            writer.bool(tuple.rest.is_some());
            if let Some(rest) = tuple.rest {
                writer.usize(rest.position)?;
                writer.u32(rest.ty.0);
            }
        }

        writer.usize(self.conditionals.len())?;
        for conditional in &self.conditionals {
            writer.u32(conditional.check.0);
            writer.u32(conditional.extends_ty.0);
            writer.u32(conditional.true_branch.0);
            writer.u32(conditional.false_branch.0);
            writer.u32(conditional.infer_count);
            writer.bool(conditional.distributive);
            writer.bool(conditional.poisoned);
        }

        writer.usize(self.instantiations.len())?;
        for instantiation in &self.instantiations {
            writer.u32(instantiation.base.0);
            writer.usize(instantiation.args.len())?;
            for (parameter, argument) in &instantiation.args {
                writer.u32(parameter.0);
                writer.u32(argument.0);
            }
        }

        writer.usize(self.class_instances.len())?;
        for instance in &self.class_instances {
            writer.u32(instance.class.0);
            write_type_ids(writer, &instance.args)?;
        }

        writer.usize(self.deferred_indexed_accesses.len())?;
        for access in &self.deferred_indexed_accesses {
            writer.u32(access.object.0);
            writer.u32(access.index.0);
        }

        writer.usize(self.mapped.len())?;
        for mapped in &self.mapped {
            writer.bool(mapped.homomorphic);
            writer.u32(mapped.key_source.0);
            writer.u32(mapped.value_template.0);
            write_optional_type_id(writer, mapped.modifiers_source);
            writer.u8(modifier_discriminant(mapped.optional_modifier));
            writer.u8(modifier_discriminant(mapped.readonly_modifier));
        }

        writer.usize(self.templates.len())?;
        for template in &self.templates {
            writer.usize(template.texts.len())?;
            for text in &template.texts {
                writer.string(text)?;
            }
            write_type_ids(writer, &template.holes)?;
        }

        let mut constraints = self.type_param_constraints.iter().collect::<Vec<_>>();
        constraints.sort_by_key(|(parameter, _)| parameter.0);
        writer.usize(constraints.len())?;
        for (parameter, constraint) in constraints {
            writer.u32(parameter.0);
            writer.u32(constraint.0);
        }

        let mut frozen = self.frozen_type_params.iter().copied().collect::<Vec<_>>();
        frozen.sort_by_key(|parameter| parameter.0);
        writer.usize(frozen.len())?;
        for parameter in frozen {
            writer.u32(parameter.0);
        }

        let mut names = self.template_names.iter().collect::<Vec<_>>();
        names.sort_by_key(|(id, _)| id.0);
        writer.usize(names.len())?;
        for (id, name) in names {
            writer.u32(id.0);
            writer.string(name)?;
        }
        Ok(())
    }

    pub(crate) fn read_snapshot_for_test(
        reader: &mut SnapshotReader<'_>,
    ) -> Result<Self, SnapshotCodecError> {
        let version_offset = reader.position();
        if reader.u32()? != 1 {
            return Err(SnapshotCodecError::invalid(
                version_offset,
                "unsupported store snapshot version",
            ));
        }

        let row_count = reader.collection_len(1 + 4 + 4 + 32)?;
        let mut tag = Vec::with_capacity(row_count);
        let mut flags = Vec::with_capacity(row_count);
        let mut payload = Vec::with_capacity(row_count);
        let mut stable_hash = Vec::with_capacity(row_count);
        for _ in 0..row_count {
            let offset = reader.position();
            tag.push(read_type_tag(reader.u8()?, offset)?);
            flags.push(TypeFlags(reader.u32()?));
            payload.push(reader.u32()?);
            let digest: [u8; 32] = reader
                .raw(32)?
                .try_into()
                .expect("the strict reader returned one stable hash");
            stable_hash.push(StableHash(digest));
        }

        let literal_count = reader.collection_len(1)?;
        let mut literals = Vec::with_capacity(literal_count);
        for _ in 0..literal_count {
            let offset = reader.position();
            literals.push(match reader.u8()? {
                0 => LiteralValue::Number(f64::from_bits(reader.u64()?)),
                1 => LiteralValue::String(reader.string()?.to_owned()),
                2 => LiteralValue::Boolean(reader.bool()?),
                _ => {
                    return Err(SnapshotCodecError::invalid(
                        offset,
                        "invalid literal discriminant",
                    ))
                }
            });
        }

        let object_count = reader.collection_len(8)?;
        let mut objects = Vec::with_capacity(object_count);
        for _ in 0..object_count {
            objects.push(read_object(reader)?);
        }
        let unions = read_type_id_slices(reader)?;
        let intersections = read_type_id_slices(reader)?;

        let function_count = reader.collection_len(8)?;
        let mut functions = Vec::with_capacity(function_count);
        for _ in 0..function_count {
            functions.push(read_function(reader)?);
        }

        let type_param_count = reader.collection_len(12)?;
        let mut type_params = Vec::with_capacity(type_param_count);
        for _ in 0..type_param_count {
            type_params.push(TypeParamType {
                id: TypeParamId(reader.u32()?),
                name: reader.string()?.to_owned(),
            });
        }

        let array_count = reader.collection_len(4)?;
        let mut arrays = Vec::with_capacity(array_count);
        for _ in 0..array_count {
            arrays.push(ArrayType {
                element: TypeId(reader.u32()?),
            });
        }

        let tuple_count = reader.collection_len(9)?;
        let mut tuples = Vec::with_capacity(tuple_count);
        for _ in 0..tuple_count {
            let elements = read_type_ids(reader)?;
            let rest = if reader.bool()? {
                Some(TupleRestType {
                    position: reader.usize()?,
                    ty: TypeId(reader.u32()?),
                })
            } else {
                None
            };
            tuples.push(TupleType { elements, rest });
        }

        let conditional_count = reader.collection_len(22)?;
        let mut conditionals = Vec::with_capacity(conditional_count);
        for _ in 0..conditional_count {
            conditionals.push(ConditionalType {
                check: TypeId(reader.u32()?),
                extends_ty: TypeId(reader.u32()?),
                true_branch: TypeId(reader.u32()?),
                false_branch: TypeId(reader.u32()?),
                infer_count: reader.u32()?,
                distributive: reader.bool()?,
                poisoned: reader.bool()?,
            });
        }

        let instantiation_count = reader.collection_len(12)?;
        let mut instantiations = Vec::with_capacity(instantiation_count);
        for _ in 0..instantiation_count {
            let base = TypeId(reader.u32()?);
            let argument_count = reader.collection_len(8)?;
            let mut args = Vec::with_capacity(argument_count);
            for _ in 0..argument_count {
                args.push((TypeParamId(reader.u32()?), TypeId(reader.u32()?)));
            }
            instantiations.push(InstantiationType { base, args });
        }

        let class_instance_count = reader.collection_len(12)?;
        let mut class_instances = Vec::with_capacity(class_instance_count);
        for _ in 0..class_instance_count {
            class_instances.push(ClassInstanceType {
                class: crate::types::repr::ClassId(reader.u32()?),
                args: read_type_ids(reader)?,
            });
        }

        let deferred_count = reader.collection_len(8)?;
        let mut deferred_indexed_accesses = Vec::with_capacity(deferred_count);
        for _ in 0..deferred_count {
            deferred_indexed_accesses.push(DeferredIndexedAccessType {
                object: TypeId(reader.u32()?),
                index: TypeId(reader.u32()?),
            });
        }

        let mapped_count = reader.collection_len(12)?;
        let mut mapped = Vec::with_capacity(mapped_count);
        for _ in 0..mapped_count {
            let homomorphic = reader.bool()?;
            let key_source = TypeId(reader.u32()?);
            let value_template = TypeId(reader.u32()?);
            let modifiers_source = read_optional_type_id(reader)?;
            let optional_offset = reader.position();
            let optional_modifier = read_modifier(reader.u8()?, optional_offset)?;
            let readonly_offset = reader.position();
            let readonly_modifier = read_modifier(reader.u8()?, readonly_offset)?;
            mapped.push(MappedType {
                homomorphic,
                key_source,
                value_template,
                modifiers_source,
                optional_modifier,
                readonly_modifier,
            });
        }

        let template_count = reader.collection_len(16)?;
        let mut templates = Vec::with_capacity(template_count);
        for _ in 0..template_count {
            let text_count = reader.collection_len(8)?;
            let mut texts = Vec::with_capacity(text_count);
            for _ in 0..text_count {
                texts.push(reader.string()?.to_owned());
            }
            templates.push(TemplateType {
                texts,
                holes: read_type_ids(reader)?,
            });
        }

        let constraint_count = reader.collection_len(8)?;
        let mut type_param_constraints = FxHashMap::default();
        let mut previous_constraint = None;
        for _ in 0..constraint_count {
            let parameter = TypeParamId(reader.u32()?);
            let constraint = TypeId(reader.u32()?);
            if previous_constraint.is_some_and(|previous| previous >= parameter.0)
                || type_param_constraints
                    .insert(parameter, constraint)
                    .is_some()
            {
                return Err(SnapshotCodecError::invalid(
                    reader.position(),
                    "type parameter constraints are not strictly ordered",
                ));
            }
            previous_constraint = Some(parameter.0);
        }

        let frozen_count = reader.collection_len(4)?;
        let mut frozen_type_params = FxHashSet::default();
        let mut previous_frozen = None;
        for _ in 0..frozen_count {
            let parameter = TypeParamId(reader.u32()?);
            if previous_frozen.is_some_and(|previous| previous >= parameter.0)
                || !frozen_type_params.insert(parameter)
            {
                return Err(SnapshotCodecError::invalid(
                    reader.position(),
                    "frozen type parameters are not strictly ordered",
                ));
            }
            previous_frozen = Some(parameter.0);
        }

        let template_name_count = reader.collection_len(12)?;
        let mut template_names = FxHashMap::default();
        let mut previous_template = None;
        for _ in 0..template_name_count {
            let id = TypeId(reader.u32()?);
            let name = reader.string()?.to_owned();
            if previous_template.is_some_and(|previous| previous >= id.0)
                || template_names.insert(id, name).is_some()
            {
                return Err(SnapshotCodecError::invalid(
                    reader.position(),
                    "template names are not strictly ordered",
                ));
            }
            previous_template = Some(id.0);
        }

        let store = Store {
            tag,
            flags,
            payload,
            literals,
            objects,
            unions,
            intersections,
            functions,
            type_params,
            arrays,
            tuples,
            conditionals,
            instantiations,
            class_instances,
            deferred_indexed_accesses,
            mapped,
            templates,
            type_param_constraints,
            frozen_type_params,
            template_names,
            stable_hash,
        };
        store.validate_snapshot_layout_for_test()?;
        Ok(store)
    }

    fn validate_snapshot_layout_for_test(&self) -> Result<(), SnapshotCodecError> {
        let len = self.len();
        let valid_type = |id: TypeId| id.index() < len;
        let mut payload_counts = [0usize; 14];
        for index in 0..len {
            if self.stable_hash[index] != StableHash::default() {
                return Err(snapshot_validation(
                    "stable hash column is unsupported by this schema",
                ));
            }
            if self.flags[index].0 & !TypeFlags::CONTAINS_ERROR.0 != 0 {
                return Err(snapshot_validation("unknown type flag bit"));
            }
            let id = TypeId(
                u32::try_from(index)
                    .map_err(|_| snapshot_validation("store length exceeds the TypeId range"))?,
            );
            let expected_flags = if self.intrinsic_kind(id) == Some(IntrinsicKind::Error) {
                TypeFlags::CONTAINS_ERROR
            } else {
                TypeFlags::EMPTY
            };
            if self.flags[index] != expected_flags {
                return Err(snapshot_validation("type flags do not match the row"));
            }
            let cold = cold_table_index(self.tag[index]);
            if let Some(cold) = cold {
                if usize::try_from(self.payload[index]).ok() != Some(payload_counts[cold]) {
                    return Err(snapshot_validation("cold payload rows are not dense"));
                }
                payload_counts[cold] += 1;
            }
            match self.tag[index] {
                TypeTag::Intrinsic if self.intrinsic_kind(id).is_none() => {
                    return Err(snapshot_validation("invalid intrinsic payload"));
                }
                TypeTag::Readonly | TypeTag::Keyof if !valid_type(TypeId(self.payload[index])) => {
                    return Err(snapshot_validation("inline type reference is out of range"));
                }
                TypeTag::MappedValue if self.payload[index] != 0 => {
                    return Err(snapshot_validation("mapped-value payload is not zero"));
                }
                _ => {}
            }
        }
        let actual_counts = [
            self.literals.len(),
            self.objects.len(),
            self.unions.len(),
            self.intersections.len(),
            self.functions.len(),
            self.type_params.len(),
            self.arrays.len(),
            self.tuples.len(),
            self.conditionals.len(),
            self.instantiations.len(),
            self.class_instances.len(),
            self.deferred_indexed_accesses.len(),
            self.mapped.len(),
            self.templates.len(),
        ];
        if payload_counts != actual_counts {
            return Err(snapshot_validation("cold payload table length mismatch"));
        }

        for object in &self.objects {
            if !object
                .properties
                .windows(2)
                .all(|pair| pair[0].name <= pair[1].name)
            {
                return Err(snapshot_validation("object properties are not canonical"));
            }
            for property in &object.properties {
                validate_type_id(property.ty, len)?;
                validate_optional_type_id(property.write_ty, len)?;
            }
            validate_optional_type_id(object.string_index, len)?;
            validate_optional_type_id(object.number_index, len)?;
            validate_type_ids(&object.call_signatures, len)?;
            validate_type_ids(&object.construct_signatures, len)?;
        }
        for members in &self.unions {
            validate_canonical_set(members, len, TypeTag::Union, &self.tag)?;
        }
        for members in &self.intersections {
            validate_canonical_set(members, len, TypeTag::Intersection, &self.tag)?;
        }
        for function in &self.functions {
            for parameter in &function.type_params {
                validate_optional_type_id(parameter.constraint, len)?;
                validate_optional_type_id(parameter.default, len)?;
            }
            validate_optional_type_id(function.receiver, len)?;
            for parameter in &function.params {
                validate_type_id(parameter.ty, len)?;
            }
            validate_type_id(function.ret, len)?;
        }
        for array in &self.arrays {
            validate_type_id(array.element, len)?;
        }
        for tuple in &self.tuples {
            validate_type_ids(&tuple.elements, len)?;
            if let Some(rest) = tuple.rest {
                if rest.position > tuple.elements.len() {
                    return Err(snapshot_validation("tuple rest position is out of range"));
                }
                validate_type_id(rest.ty, len)?;
            }
        }
        for conditional in &self.conditionals {
            validate_type_ids(
                &[
                    conditional.check,
                    conditional.extends_ty,
                    conditional.true_branch,
                    conditional.false_branch,
                ],
                len,
            )?;
        }
        for instantiation in &self.instantiations {
            validate_type_id(instantiation.base, len)?;
            if !instantiation
                .args
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
            {
                return Err(snapshot_validation(
                    "instantiation arguments are not canonical",
                ));
            }
            for (_, argument) in &instantiation.args {
                validate_type_id(*argument, len)?;
            }
        }
        for instance in &self.class_instances {
            validate_type_ids(&instance.args, len)?;
        }
        for access in &self.deferred_indexed_accesses {
            validate_type_id(access.object, len)?;
            validate_type_id(access.index, len)?;
        }
        for mapped in &self.mapped {
            validate_type_id(mapped.key_source, len)?;
            validate_type_id(mapped.value_template, len)?;
            validate_optional_type_id(mapped.modifiers_source, len)?;
        }
        for template in &self.templates {
            if template.texts.len() != template.holes.len() + 1 {
                return Err(snapshot_validation("template text/hole arity mismatch"));
            }
            validate_type_ids(&template.holes, len)?;
        }

        let parameter_ids = self
            .type_params
            .iter()
            .map(|parameter| parameter.id)
            .collect::<FxHashSet<_>>();
        if parameter_ids.len() != self.type_params.len() {
            return Err(snapshot_validation("duplicate TypeParamId rows"));
        }
        for (parameter, constraint) in &self.type_param_constraints {
            if !parameter_ids.contains(parameter) {
                return Err(snapshot_validation(
                    "constraint owner has no type parameter row",
                ));
            }
            validate_type_id(*constraint, len)?;
        }
        if !self
            .frozen_type_params
            .iter()
            .all(|parameter| parameter_ids.contains(parameter))
        {
            return Err(snapshot_validation(
                "frozen owner has no type parameter row",
            ));
        }
        for id in self.template_names.keys() {
            validate_type_id(*id, len)?;
            if !matches!(self.tag(*id), TypeTag::Conditional | TypeTag::Mapped) {
                return Err(snapshot_validation(
                    "template name is attached to a non-template row",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn snapshot_validation(message: &'static str) -> SnapshotCodecError {
    SnapshotCodecError::invalid(0, message)
}

#[cfg(test)]
fn validate_type_id(id: TypeId, len: usize) -> Result<(), SnapshotCodecError> {
    if id.index() < len {
        Ok(())
    } else {
        Err(snapshot_validation("TypeId reference is out of range"))
    }
}

#[cfg(test)]
fn validate_optional_type_id(id: Option<TypeId>, len: usize) -> Result<(), SnapshotCodecError> {
    if let Some(id) = id {
        validate_type_id(id, len)?;
    }
    Ok(())
}

#[cfg(test)]
fn validate_type_ids(ids: &[TypeId], len: usize) -> Result<(), SnapshotCodecError> {
    for id in ids {
        validate_type_id(*id, len)?;
    }
    Ok(())
}

#[cfg(test)]
fn validate_canonical_set(
    members: &[TypeId],
    len: usize,
    nested_tag: TypeTag,
    tags: &[TypeTag],
) -> Result<(), SnapshotCodecError> {
    if members.len() < 2 || !members.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(snapshot_validation(
            "set-operator members are not canonical",
        ));
    }
    validate_type_ids(members, len)?;
    if members
        .iter()
        .any(|member| tags[member.index()] == nested_tag)
    {
        return Err(snapshot_validation(
            "set-operator members are not flattened",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn cold_table_index(tag: TypeTag) -> Option<usize> {
    match tag {
        TypeTag::Literal => Some(0),
        TypeTag::Object => Some(1),
        TypeTag::Union => Some(2),
        TypeTag::Intersection => Some(3),
        TypeTag::Function => Some(4),
        TypeTag::TypeParam => Some(5),
        TypeTag::Array => Some(6),
        TypeTag::Tuple => Some(7),
        TypeTag::Conditional => Some(8),
        TypeTag::Instantiation => Some(9),
        TypeTag::ClassInstance => Some(10),
        TypeTag::DeferredIndexedAccess => Some(11),
        TypeTag::Mapped => Some(12),
        TypeTag::Template => Some(13),
        TypeTag::Intrinsic
        | TypeTag::Readonly
        | TypeTag::Infer
        | TypeTag::MappedValue
        | TypeTag::Keyof => None,
    }
}

#[cfg(test)]
fn write_type_ids(writer: &mut SnapshotWriter, ids: &[TypeId]) -> Result<(), SnapshotCodecError> {
    writer.usize(ids.len())?;
    for id in ids {
        writer.u32(id.0);
    }
    Ok(())
}

#[cfg(test)]
fn read_type_ids(reader: &mut SnapshotReader<'_>) -> Result<Vec<TypeId>, SnapshotCodecError> {
    let count = reader.collection_len(4)?;
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(TypeId(reader.u32()?));
    }
    Ok(ids)
}

#[cfg(test)]
fn write_type_id_slices(
    writer: &mut SnapshotWriter,
    rows: &[Box<[TypeId]>],
) -> Result<(), SnapshotCodecError> {
    writer.usize(rows.len())?;
    for row in rows {
        write_type_ids(writer, row)?;
    }
    Ok(())
}

#[cfg(test)]
fn read_type_id_slices(
    reader: &mut SnapshotReader<'_>,
) -> Result<Vec<Box<[TypeId]>>, SnapshotCodecError> {
    let count = reader.collection_len(8)?;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        rows.push(read_type_ids(reader)?.into_boxed_slice());
    }
    Ok(rows)
}

#[cfg(test)]
fn write_optional_type_id(writer: &mut SnapshotWriter, id: Option<TypeId>) {
    writer.bool(id.is_some());
    if let Some(id) = id {
        writer.u32(id.0);
    }
}

#[cfg(test)]
fn read_optional_type_id(
    reader: &mut SnapshotReader<'_>,
) -> Result<Option<TypeId>, SnapshotCodecError> {
    if reader.bool()? {
        Ok(Some(TypeId(reader.u32()?)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
fn write_object(
    writer: &mut SnapshotWriter,
    object: &ObjectType,
) -> Result<(), SnapshotCodecError> {
    writer.usize(object.properties.len())?;
    for property in &object.properties {
        writer.string(&property.name)?;
        writer.u32(property.ty.0);
        write_optional_type_id(writer, property.write_ty);
        writer.bool(property.optional);
        writer.u8(match property.visibility {
            Visibility::Public => 0,
            Visibility::Private => 1,
            Visibility::Protected => 2,
        });
        writer.bool(property.declaring_class.is_some());
        if let Some(class) = property.declaring_class {
            writer.u32(class.0);
        }
        writer.bool(property.readonly);
        writer.bool(property.is_accessor);
    }
    write_optional_type_id(writer, object.string_index);
    write_optional_type_id(writer, object.number_index);
    write_type_ids(writer, &object.call_signatures)?;
    write_type_ids(writer, &object.construct_signatures)
}

#[cfg(test)]
fn read_object(reader: &mut SnapshotReader<'_>) -> Result<ObjectType, SnapshotCodecError> {
    let property_count = reader.collection_len(16)?;
    let mut properties = Vec::with_capacity(property_count);
    for _ in 0..property_count {
        let name = reader.string()?.to_owned();
        let ty = TypeId(reader.u32()?);
        let write_ty = read_optional_type_id(reader)?;
        let optional = reader.bool()?;
        let visibility_offset = reader.position();
        let visibility = match reader.u8()? {
            0 => Visibility::Public,
            1 => Visibility::Private,
            2 => Visibility::Protected,
            _ => {
                return Err(SnapshotCodecError::invalid(
                    visibility_offset,
                    "invalid visibility discriminant",
                ))
            }
        };
        let declaring_class = if reader.bool()? {
            Some(crate::types::repr::ClassId(reader.u32()?))
        } else {
            None
        };
        properties.push(PropertyType {
            name,
            ty,
            write_ty,
            optional,
            visibility,
            declaring_class,
            readonly: reader.bool()?,
            is_accessor: reader.bool()?,
        });
    }
    Ok(ObjectType {
        properties,
        string_index: read_optional_type_id(reader)?,
        number_index: read_optional_type_id(reader)?,
        call_signatures: read_type_ids(reader)?,
        construct_signatures: read_type_ids(reader)?,
    })
}

#[cfg(test)]
fn write_function(
    writer: &mut SnapshotWriter,
    function: &FunctionType,
) -> Result<(), SnapshotCodecError> {
    writer.usize(function.type_params.len())?;
    for parameter in &function.type_params {
        writer.u32(parameter.id.0);
        write_optional_type_id(writer, parameter.constraint);
        write_optional_type_id(writer, parameter.default);
    }
    write_optional_type_id(writer, function.receiver);
    writer.usize(function.params.len())?;
    for parameter in &function.params {
        writer.string(&parameter.name)?;
        writer.u32(parameter.ty.0);
        writer.bool(parameter.optional);
        writer.bool(parameter.has_default);
        writer.bool(parameter.rest);
    }
    writer.u32(function.ret.0);
    Ok(())
}

#[cfg(test)]
fn read_function(reader: &mut SnapshotReader<'_>) -> Result<FunctionType, SnapshotCodecError> {
    let type_param_count = reader.collection_len(6)?;
    let mut type_params = Vec::with_capacity(type_param_count);
    for _ in 0..type_param_count {
        type_params.push(GenericTypeParam {
            id: TypeParamId(reader.u32()?),
            constraint: read_optional_type_id(reader)?,
            default: read_optional_type_id(reader)?,
        });
    }
    let receiver = read_optional_type_id(reader)?;
    let parameter_count = reader.collection_len(15)?;
    let mut params = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        params.push(ParameterType {
            name: reader.string()?.to_owned(),
            ty: TypeId(reader.u32()?),
            optional: reader.bool()?,
            has_default: reader.bool()?,
            rest: reader.bool()?,
        });
    }
    Ok(FunctionType {
        type_params,
        receiver,
        params,
        ret: TypeId(reader.u32()?),
    })
}

#[cfg(test)]
fn modifier_discriminant(modifier: ModifierOp) -> u8 {
    match modifier {
        ModifierOp::Keep => 0,
        ModifierOp::Add => 1,
        ModifierOp::Remove => 2,
    }
}

#[cfg(test)]
fn read_modifier(value: u8, offset: usize) -> Result<ModifierOp, SnapshotCodecError> {
    match value {
        0 => Ok(ModifierOp::Keep),
        1 => Ok(ModifierOp::Add),
        2 => Ok(ModifierOp::Remove),
        _ => Err(SnapshotCodecError::invalid(
            offset,
            "invalid mapped modifier discriminant",
        )),
    }
}

#[cfg(test)]
fn read_type_tag(value: u8, offset: usize) -> Result<TypeTag, SnapshotCodecError> {
    match value {
        0 => Ok(TypeTag::Intrinsic),
        1 => Ok(TypeTag::Literal),
        2 => Ok(TypeTag::Object),
        3 => Ok(TypeTag::Union),
        4 => Ok(TypeTag::Intersection),
        5 => Ok(TypeTag::Function),
        6 => Ok(TypeTag::TypeParam),
        7 => Ok(TypeTag::Array),
        8 => Ok(TypeTag::Tuple),
        9 => Ok(TypeTag::Readonly),
        10 => Ok(TypeTag::Conditional),
        11 => Ok(TypeTag::Instantiation),
        12 => Ok(TypeTag::Infer),
        13 => Ok(TypeTag::Mapped),
        14 => Ok(TypeTag::MappedValue),
        15 => Ok(TypeTag::Template),
        16 => Ok(TypeTag::Keyof),
        17 => Ok(TypeTag::ClassInstance),
        18 => Ok(TypeTag::DeferredIndexedAccess),
        _ => Err(SnapshotCodecError::invalid(
            offset,
            "invalid type tag discriminant",
        )),
    }
}
