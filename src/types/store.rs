//! Append-only struct-of-arrays type arena keyed by `TypeId(u32)`.
//!
//! Hot columns are indexed directly by `TypeId`; variable-size data lives in
//! tag-selected side tables. `TypeId`s stay stable for the process lifetime.

use crate::types::hash::StableHash;
use crate::types::repr::{
    ArrayType, ConditionalType, FunctionType, InstantiationType, IntrinsicKind, LiteralValue,
    MappedType, ObjectType, TemplateType, TupleType, TypeFlags, TypeParamId, TypeParamType,
    TypeTag,
};
use rustc_hash::FxHashMap;

/// A run-local handle to a type: an index into the SoA arena. Cheap to copy and
/// compare; structural equality of two interned types is `a == b`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TypeId(pub u32);

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
    /// Tuple types (M18). Addressed by the `payload` of a `Tuple`-tagged row. Each
    /// entry's identity is its **ordered** element list (so `[number, string]`
    /// hash-conses to one id and `[number, string]` ≠ `[string, number]` ≠
    /// `[number]`).
    tuples: Vec<TupleType>,
    /// Conditional types (M25). Addressed by the `payload` of a `Conditional`-tagged
    /// row. Field order is significant (position is meaning); a recursive alias
    /// template row is reserved empty and filled in place (`fill_conditional`), like a
    /// nominal object.
    conditionals: Vec<ConditionalType>,
    /// Lazy alias instantiations (M25). Addressed by the `payload` of an
    /// `Instantiation`-tagged row. Identity is `(base, sorted args)`.
    instantiations: Vec<InstantiationType>,
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
        IntrinsicKind::ALL
            .into_iter()
            .find(|k| *k as u32 == raw)
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

    /// The `ArrayType` (its element id) of an array type, or `None` if `id` is not
    /// an array (M17).
    pub fn array_type(&self, id: TypeId) -> Option<&ArrayType> {
        if self.tag(id) != TypeTag::Array {
            return None;
        }
        self.arrays.get(self.payload(id) as usize)
    }

    /// The `TupleType` (its ordered element list) of a tuple type, or `None` if
    /// `id` is not a tuple (M18).
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
    pub(crate) fn set_type_param_constraint(&mut self, id: TypeParamId, constraint: TypeId) {
        self.type_param_constraints.insert(id, constraint);
    }

    /// Erase a circular type-parameter constraint so the degenerate cycle never
    /// reaches the relation engine's assume-true stack.
    pub(crate) fn remove_type_param_constraint(&mut self, id: TypeParamId) {
        self.type_param_constraints.remove(&id);
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

    /// Replace a reserved object body in place, preserving the row's `TypeId` and
    /// hot columns. No-op if `id` is not an object row.
    pub(crate) fn set_object(&mut self, id: TypeId, object: ObjectType) {
        if self.tag(id) != TypeTag::Object {
            return;
        }
        let payload = self.payload(id) as usize;
        if let Some(slot) = self.objects.get_mut(payload) {
            *slot = object;
        }
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

    /// Replace the body of an existing `Conditional` row in place (M25 recursive
    /// conditional-alias reserve-then-fill — `Interner::fill_conditional`). Preserves
    /// the row's `TypeId`; a no-op if `id` is not a conditional row.
    pub(crate) fn set_conditional(&mut self, id: TypeId, conditional: ConditionalType) {
        if self.tag(id) != TypeTag::Conditional {
            return;
        }
        let payload = self.payload(id) as usize;
        if let Some(slot) = self.conditionals.get_mut(payload) {
            *slot = conditional;
        }
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

    /// Replace the body of an existing `Mapped` row in place (M28 recursive
    /// mapped-alias reserve-then-fill — `Interner::fill_mapped`, mirroring
    /// [`Store::set_conditional`]). Preserves the row's `TypeId`; a no-op if `id` is
    /// not a mapped row.
    pub(crate) fn set_mapped(&mut self, id: TypeId, mapped: MappedType) {
        if self.tag(id) != TypeTag::Mapped {
            return;
        }
        let payload = self.payload(id) as usize;
        if let Some(slot) = self.mapped.get_mut(payload) {
            *slot = mapped;
        }
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
