//! Append-only struct-of-arrays type arena keyed by `TypeId(u32)`.
//!
//! Hot columns are indexed directly by `TypeId`; variable-size data lives in
//! tag-selected side tables. `TypeId`s stay stable for the process lifetime.

use crate::types::hash::StableHash;
use crate::types::layered::{LayeredMap, LayeredSet, LayeredVec};
use crate::types::repr::{
    ArrayType, ClassInstanceType, ConditionalType, DeclaredRecipeId, DeclaredType,
    DeclaredTypeRecipe, DeferredIndexedAccessType, FunctionType, InstantiationType, IntrinsicKind,
    LiteralValue, MappedType, ObjectType, TemplateType, TupleType, TypeFlags, TypeParamId,
    TypeParamType, TypeTag,
};
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// A run-local handle to a type: an index into the SoA arena. Cheap to copy and
/// compare; structural equality of two interned types is `a == b`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TypeId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypeParamFreezeError {
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
    /// Identity of the current outgoing-edge graph. Append-only rows preserve it;
    /// filling a reserved row or changing a side-column edge replaces it.
    semantic_graph_identity: Arc<()>,
    /// Exact declaration identities owned by the sealed type-parameter prefix.
    sealed_type_param_ids: Arc<FxHashSet<TypeParamId>>,

    // --- hot, parallel, indexed by TypeId ---
    tag: LayeredVec<TypeTag>,
    flags: LayeredVec<TypeFlags>,
    /// For `Intrinsic`: the `IntrinsicKind` discriminant. Otherwise: an index
    /// into the cold side-table selected by `tag`.
    payload: LayeredVec<u32>,

    // --- cold side-tables ---
    literals: LayeredVec<LiteralValue>,
    /// Object types (M2). Addressed by the `payload` of an `Object`-tagged row.
    objects: LayeredVec<ObjectType>,
    /// Union members (M4), already canonicalized: flattened, sorted by `TypeId`,
    /// deduped, with `never` dropped (mvp-plan §3.3). Addressed by the `payload`
    /// of a `Union`-tagged row. A union row always has at least two members — the
    /// 0- and 1-member cases collapse in the interner and never reach the store.
    unions: LayeredVec<Box<[TypeId]>>,
    /// Intersection members (M31), already canonicalized by the interner. Like a
    /// union row, always at least two members.
    intersections: LayeredVec<Box<[TypeId]>>,
    /// Function types (M3). Addressed by the `payload` of a `Function`-tagged row.
    functions: LayeredVec<FunctionType>,
    /// Type-parameter types (M9). Addressed by the `payload` of a
    /// `TypeParam`-tagged row. Each entry's identity is its `TypeParamId`.
    type_params: LayeredVec<TypeParamType>,
    /// Array types (M17). Addressed by the `payload` of an `Array`-tagged row. Each
    /// entry's identity is its element `TypeId` (so `number[]` hash-conses to one id
    /// and `number[]` ≠ `string[]`).
    arrays: LayeredVec<ArrayType>,
    /// Tuple types (M18, rest-shape expanded in M32/WU2). Addressed by the
    /// `payload` of a `Tuple`-tagged row. Each entry's identity is its ordered
    /// fixed element list plus any rest position/type.
    tuples: LayeredVec<TupleType>,
    /// Conditional types (M25). Addressed by the `payload` of a `Conditional`-tagged
    /// row. Field order is significant (position is meaning); a recursive alias
    /// template row is reserved empty and filled in place (`fill_conditional`), like a
    /// nominal object.
    conditionals: LayeredVec<ConditionalType>,
    /// Lazy alias instantiations (M25). Addressed by the `payload` of an
    /// `Instantiation`-tagged row. Identity is `(base, sorted args)`.
    instantiations: LayeredVec<InstantiationType>,
    /// Immutable class applications, distinct from lazy alias instantiations.
    class_instances: LayeredVec<ClassInstanceType>,
    /// Immutable deferred indexed-access operand pairs.
    deferred_indexed_accesses: LayeredVec<DeferredIndexedAccessType>,
    /// AST-free declaration recipes, shared independently of their applications.
    declared_recipes: LayeredVec<DeclaredTypeRecipe>,
    /// Declaration-recipe applications addressed by `TypeTag::Declared`.
    declared_types: LayeredVec<DeclaredType>,
    /// Mapped types (M26). Addressed by the `payload` of a `Mapped`-tagged row. The
    /// whole [`MappedType`] is its structural identity. A `MappedValue` placeholder row
    /// carries no side-table entry (payload `0`), like an intrinsic.
    mapped: LayeredVec<MappedType>,
    /// Template literal types (M27). Addressed by the `payload` of a `Template`-tagged
    /// row. The whole [`TemplateType`] (its text segments + hole ids) is its structural
    /// identity.
    templates: LayeredVec<TemplateType>,

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
    type_param_constraints: LayeredMap<TypeParamId, TypeId>,

    /// Declaration binders whose side-column metadata is immutable. Class
    /// publication freezes the complete SCC batch before any surface is exposed.
    frozen_type_params: LayeredSet<TypeParamId>,

    /// Template display names (M28 round 3), keyed by reserved template id.
    /// Rendering-only: deferred instantiations print as alias names, not raw bodies.
    template_names: LayeredMap<TypeId, String>,

    /// Reserved cross-run identity column (architecture §3.2). NOT populated in
    /// the MVP (mvp-plan §7.1) — kept so Phase 4 can fill it at intern time
    /// without changing the arena shape.
    #[allow(dead_code)] // TODO(Phase 4): populate alongside each push.
    stable_hash: LayeredVec<StableHash>,
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    /// Number of interned types.
    pub fn len(&self) -> usize {
        self.tag.len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn frozen_prefix_len_for_test(&self) -> usize {
        self.tag.base_len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_type_ids_for_test(&self) -> impl Iterator<Item = TypeId> + '_ {
        let base_len = self.tag.base_len();
        self.tag.local_iter().enumerate().map(move |(index, _)| {
            TypeId(u32::try_from(base_len + index).expect("type id fits u32"))
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_type_param_ids_for_test(&self) -> impl Iterator<Item = TypeParamId> + '_ {
        self.type_params.local_iter().map(|parameter| parameter.id)
    }

    pub fn base_len(&self) -> usize {
        self.tag.base_len()
    }

    pub fn is_empty(&self) -> bool {
        self.tag.is_empty()
    }

    pub fn semantic_graph_identity(&self) -> &Arc<()> {
        &self.semantic_graph_identity
    }

    pub fn mark_semantic_graph_mutation(&mut self) {
        self.semantic_graph_identity = Arc::new(());
    }

    /// Seal this standalone store as an immutable prefix.
    pub fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        if self.tag.is_sealed() {
            return Err("store is already sealed");
        }
        self.sealed_type_param_ids = Arc::new(
            self.type_params
                .iter()
                .map(|parameter| parameter.id)
                .collect(),
        );
        self.tag.freeze_as_base()?;
        self.flags.freeze_as_base()?;
        self.payload.freeze_as_base()?;
        self.literals.freeze_as_base()?;
        self.objects.freeze_as_base()?;
        self.unions.freeze_as_base()?;
        self.intersections.freeze_as_base()?;
        self.functions.freeze_as_base()?;
        self.type_params.freeze_as_base()?;
        self.arrays.freeze_as_base()?;
        self.tuples.freeze_as_base()?;
        self.conditionals.freeze_as_base()?;
        self.instantiations.freeze_as_base()?;
        self.class_instances.freeze_as_base()?;
        self.deferred_indexed_accesses.freeze_as_base()?;
        self.declared_recipes.freeze_as_base()?;
        self.declared_types.freeze_as_base()?;
        self.mapped.freeze_as_base()?;
        self.templates.freeze_as_base()?;
        self.type_param_constraints.freeze_as_base()?;
        self.frozen_type_params.freeze_as_base()?;
        self.template_names.freeze_as_base()?;
        self.stable_hash.freeze_as_base()?;
        Ok(())
    }

    /// Create a private empty suffix over a sealed immutable prefix.
    pub fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            semantic_graph_identity: Arc::new(()),
            sealed_type_param_ids: Arc::clone(&self.sealed_type_param_ids),
            tag: self.tag.fork_delta()?,
            flags: self.flags.fork_delta()?,
            payload: self.payload.fork_delta()?,
            literals: self.literals.fork_delta()?,
            objects: self.objects.fork_delta()?,
            unions: self.unions.fork_delta()?,
            intersections: self.intersections.fork_delta()?,
            functions: self.functions.fork_delta()?,
            type_params: self.type_params.fork_delta()?,
            arrays: self.arrays.fork_delta()?,
            tuples: self.tuples.fork_delta()?,
            conditionals: self.conditionals.fork_delta()?,
            instantiations: self.instantiations.fork_delta()?,
            class_instances: self.class_instances.fork_delta()?,
            deferred_indexed_accesses: self.deferred_indexed_accesses.fork_delta()?,
            declared_recipes: self.declared_recipes.fork_delta()?,
            declared_types: self.declared_types.fork_delta()?,
            mapped: self.mapped.fork_delta()?,
            templates: self.templates.fork_delta()?,
            type_param_constraints: self.type_param_constraints.fork_delta()?,
            frozen_type_params: self.frozen_type_params.fork_delta()?,
            template_names: self.template_names.fork_delta()?,
            stable_hash: self.stable_hash.fork_delta()?,
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn has_nonempty_delta(&self) -> bool {
        self.tag.is_sealed()
            && (self.tag.local_len() != 0
                || self.flags.local_len() != 0
                || self.payload.local_len() != 0
                || self.literals.local_len() != 0
                || self.objects.local_len() != 0
                || self.unions.local_len() != 0
                || self.intersections.local_len() != 0
                || self.functions.local_len() != 0
                || self.type_params.local_len() != 0
                || self.arrays.local_len() != 0
                || self.tuples.local_len() != 0
                || self.conditionals.local_len() != 0
                || self.instantiations.local_len() != 0
                || self.class_instances.local_len() != 0
                || self.deferred_indexed_accesses.local_len() != 0
                || self.declared_recipes.local_len() != 0
                || self.declared_types.local_len() != 0
                || self.mapped.local_len() != 0
                || self.templates.local_len() != 0
                || self.type_param_constraints.local_len() != 0
                || self.frozen_type_params.local_len() != 0
                || self.template_names.local_len() != 0
                || self.stable_hash.local_len() != 0)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_sealed_base(&self) -> bool {
        self.tag.is_sealed()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn shares_base_rows_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.sealed_type_param_ids, &other.sealed_type_param_ids)
            && self.tag.shares_base_with(&other.tag)
            && self.flags.shares_base_with(&other.flags)
            && self.payload.shares_base_with(&other.payload)
            && self.literals.shares_base_with(&other.literals)
            && self.objects.shares_base_with(&other.objects)
            && self.unions.shares_base_with(&other.unions)
            && self.intersections.shares_base_with(&other.intersections)
            && self.functions.shares_base_with(&other.functions)
            && self.type_params.shares_base_with(&other.type_params)
            && self.arrays.shares_base_with(&other.arrays)
            && self.tuples.shares_base_with(&other.tuples)
            && self.conditionals.shares_base_with(&other.conditionals)
            && self.instantiations.shares_base_with(&other.instantiations)
            && self
                .class_instances
                .shares_base_with(&other.class_instances)
            && self
                .deferred_indexed_accesses
                .shares_base_with(&other.deferred_indexed_accesses)
            && self
                .declared_recipes
                .shares_base_with(&other.declared_recipes)
            && self.declared_types.shares_base_with(&other.declared_types)
            && self.mapped.shares_base_with(&other.mapped)
            && self.templates.shares_base_with(&other.templates)
            && self
                .type_param_constraints
                .shares_base_with(&other.type_param_constraints)
            && self
                .frozen_type_params
                .shares_base_with(&other.frozen_type_params)
            && self.template_names.shares_base_with(&other.template_names)
            && self.stable_hash.shares_base_with(&other.stable_hash)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn base_family_sharing_with(&self, other: &Self) -> [bool; 5] {
        let rows = self.tag.shares_base_with(&other.tag)
            && self.flags.shares_base_with(&other.flags)
            && self.payload.shares_base_with(&other.payload);
        let payload_tables = self.literals.shares_base_with(&other.literals)
            && self.objects.shares_base_with(&other.objects)
            && self.unions.shares_base_with(&other.unions)
            && self.intersections.shares_base_with(&other.intersections)
            && self.functions.shares_base_with(&other.functions)
            && self.type_params.shares_base_with(&other.type_params)
            && self.arrays.shares_base_with(&other.arrays)
            && self.tuples.shares_base_with(&other.tuples)
            && self.conditionals.shares_base_with(&other.conditionals)
            && self.instantiations.shares_base_with(&other.instantiations)
            && self
                .class_instances
                .shares_base_with(&other.class_instances)
            && self
                .deferred_indexed_accesses
                .shares_base_with(&other.deferred_indexed_accesses)
            && self
                .declared_recipes
                .shares_base_with(&other.declared_recipes)
            && self.declared_types.shares_base_with(&other.declared_types)
            && self.mapped.shares_base_with(&other.mapped)
            && self.templates.shares_base_with(&other.templates)
            && self.stable_hash.shares_base_with(&other.stable_hash);
        [
            rows,
            payload_tables,
            self.type_param_constraints
                .shares_base_with(&other.type_param_constraints),
            Arc::ptr_eq(&self.sealed_type_param_ids, &other.sealed_type_param_ids)
                && self
                    .frozen_type_params
                    .shares_base_with(&other.frozen_type_params),
            self.template_names.shares_base_with(&other.template_names),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_family_row_counts_for_test(&self) -> [usize; 5] {
        [
            self.tag.local_len(),
            self.literals.local_len()
                + self.objects.local_len()
                + self.unions.local_len()
                + self.intersections.local_len()
                + self.functions.local_len()
                + self.type_params.local_len()
                + self.arrays.local_len()
                + self.tuples.local_len()
                + self.conditionals.local_len()
                + self.instantiations.local_len()
                + self.class_instances.local_len()
                + self.deferred_indexed_accesses.local_len()
                + self.declared_recipes.local_len()
                + self.declared_types.local_len()
                + self.mapped.local_len()
                + self.templates.local_len()
                + self.stable_hash.local_len(),
            self.type_param_constraints.local_len(),
            self.frozen_type_params.local_len(),
            self.template_names.local_len(),
        ]
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

    pub fn declared_recipe(&self, id: DeclaredRecipeId) -> Option<&DeclaredTypeRecipe> {
        self.declared_recipes.get(id.index())
    }

    pub fn declared_type(&self, id: TypeId) -> Option<&DeclaredType> {
        if self.tag(id) != TypeTag::Declared {
            return None;
        }
        self.declared_types
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

    fn type_param_belongs_to_base(&self, id: TypeParamId) -> bool {
        self.sealed_type_param_ids.contains(&id)
    }

    /// Record a type parameter's `extends` constraint (M24). Internal — the checker
    /// calls it through `Interner::set_type_param_constraint` once, when the
    /// declaration's parameter list is lowered with the frame active.
    pub fn set_type_param_constraint(&mut self, id: TypeParamId, constraint: TypeId) -> bool {
        if self.type_param_belongs_to_base(id) || self.frozen_type_params.contains(&id) {
            return false;
        }
        let Ok(previous) = self.type_param_constraints.insert_local(id, constraint) else {
            return false;
        };
        let changed = previous != Some(constraint);
        if changed {
            self.mark_semantic_graph_mutation();
        }
        true
    }

    /// Erase a circular type-parameter constraint so the degenerate cycle never
    /// reaches the relation engine's assume-true stack.
    pub fn remove_type_param_constraint(&mut self, id: TypeParamId) -> bool {
        if self.type_param_belongs_to_base(id) || self.frozen_type_params.contains(&id) {
            return false;
        }
        let Ok(removed) = self.type_param_constraints.remove_local(&id) else {
            return false;
        };
        if removed.is_some() {
            self.mark_semantic_graph_mutation();
        }
        true
    }

    pub fn type_param_metadata_is_frozen(&self, id: TypeParamId) -> bool {
        self.frozen_type_params.contains(&id)
    }

    /// Freeze a whole declaration batch after prevalidation. A failed batch
    /// changes nothing, so publication cannot expose a partially frozen SCC.
    pub fn freeze_type_param_metadata(
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
        if let Some(&id) = ids.iter().find(|id| self.type_param_belongs_to_base(**id)) {
            return Err(TypeParamFreezeError::AlreadyFrozen(id));
        }
        for &id in ids {
            if self.frozen_type_params.insert_local(id).is_err() {
                return Err(TypeParamFreezeError::AlreadyFrozen(id));
            }
        }
        Ok(())
    }

    /// The display name of a reserved template row (M28 round 3), or `None` for an
    /// unnamed/ordinary type. Rendering-only — see the `template_names` column.
    pub fn template_name(&self, id: TypeId) -> Option<&str> {
        self.template_names.get(&id).map(String::as_str)
    }

    /// Record a reserved template row's display name (M28 round 3). Internal — the
    /// checker calls it through `Interner::set_template_name` at reserve time.
    pub fn set_template_name(&mut self, id: TypeId, name: String) {
        if id.index() < self.tag.base_len()
            || id.index() >= self.len()
            || !matches!(self.tag(id), TypeTag::Conditional | TypeTag::Mapped)
        {
            return;
        }
        let _ = self.template_names.insert_local(id, name);
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
        self.tag.push_local(tag);
        self.flags.push_local(flags);
        self.payload.push_local(payload);
        // Keep the reserved column length-aligned even though it is unread.
        self.stable_hash.push_local(StableHash::default());
        id
    }

    /// Append an intrinsic row. Internal — `Interner` owns dedup.
    pub fn push_intrinsic(&mut self, kind: IntrinsicKind, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Intrinsic, flags, kind as u32)
    }

    /// Append a literal row (value into the side-table, index into payload).
    /// Internal — `Interner` owns dedup.
    pub fn push_literal(&mut self, value: LiteralValue, flags: TypeFlags) -> TypeId {
        let payload = self.literals.len() as u32;
        self.literals.push_local(value);
        self.push(TypeTag::Literal, flags, payload)
    }

    /// Append an object row (object into the side-table, index into payload).
    /// The caller (`Interner`) passes an already-canonicalized `ObjectType` and
    /// owns dedup.
    pub fn push_object(&mut self, object: ObjectType, flags: TypeFlags) -> TypeId {
        let payload = self.objects.len() as u32;
        self.objects.push_local(object);
        self.push(TypeTag::Object, flags, payload)
    }

    /// Replace a prevalidated reserved object body while preserving its `TypeId`.
    pub(super) fn set_object(&mut self, id: TypeId, object: ObjectType) {
        assert_eq!(self.tag(id), TypeTag::Object);
        let payload = self.payload(id) as usize;
        let slot = self
            .objects
            .get_mut_local(payload)
            .expect("validated object row must have a side-table entry");
        *slot = object;
    }

    /// Append a function row (function into the side-table, index into payload).
    /// Internal — `Interner` owns dedup. Parameters are stored positionally (the
    /// caller does not sort them).
    pub fn push_function(&mut self, function: FunctionType, flags: TypeFlags) -> TypeId {
        let payload = self.functions.len() as u32;
        self.functions.push_local(function);
        self.push(TypeTag::Function, flags, payload)
    }

    /// Append a union row (members into the side-table, index into payload).
    /// Internal — `Interner` owns canonicalization and dedup; the caller passes
    /// an already-canonical (flattened, sorted, deduped, `never`-free) member
    /// slice of length ≥ 2.
    pub fn push_union(&mut self, members: Box<[TypeId]>, flags: TypeFlags) -> TypeId {
        let payload = self.unions.len() as u32;
        self.unions.push_local(members);
        self.push(TypeTag::Union, flags, payload)
    }

    /// Append an intersection row (members into the side-table, index into
    /// payload). Internal — `Interner` owns canonicalization and dedup; the caller
    /// passes an already-canonical (flattened, sorted, deduped, `unknown`-free)
    /// member slice of length ≥ 2 (M31).
    pub fn push_intersection(&mut self, members: Box<[TypeId]>, flags: TypeFlags) -> TypeId {
        let payload = self.intersections.len() as u32;
        self.intersections.push_local(members);
        self.push(TypeTag::Intersection, flags, payload)
    }

    /// Append a type-parameter row (M9). Internal — `Interner` owns dedup (by
    /// `TypeParamId`).
    pub fn push_type_param(&mut self, param: TypeParamType, flags: TypeFlags) -> TypeId {
        let payload = self.type_params.len() as u32;
        self.type_params.push_local(param);
        self.push(TypeTag::TypeParam, flags, payload)
    }

    /// Append an array row (M17). Internal — `Interner` owns dedup (by element id).
    pub fn push_array(&mut self, array: ArrayType, flags: TypeFlags) -> TypeId {
        let payload = self.arrays.len() as u32;
        self.arrays.push_local(array);
        self.push(TypeTag::Array, flags, payload)
    }

    /// Append a tuple row (M18). Internal — `Interner` owns dedup (by the ordered
    /// element list). The caller passes elements in source order (never sorted).
    pub fn push_tuple(&mut self, tuple: TupleType, flags: TypeFlags) -> TypeId {
        let payload = self.tuples.len() as u32;
        self.tuples.push_local(tuple);
        self.push(TypeTag::Tuple, flags, payload)
    }

    /// Append a readonly wrapper row. Internal — `Interner` owns dedup by operand.
    /// The wrapped array/tuple id is stored inline in `payload`.
    pub fn push_readonly(&mut self, operand: TypeId, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Readonly, flags, operand.0)
    }

    /// Append a conditional row (M25). Internal — `Interner` owns dedup (by all four
    /// component ids + `infer_count` + `distributive`, in order).
    pub fn push_conditional(&mut self, conditional: ConditionalType, flags: TypeFlags) -> TypeId {
        let payload = self.conditionals.len() as u32;
        self.conditionals.push_local(conditional);
        self.push(TypeTag::Conditional, flags, payload)
    }

    /// Replace a prevalidated reserved conditional body while preserving its `TypeId`.
    pub(super) fn set_conditional(&mut self, id: TypeId, conditional: ConditionalType) {
        assert_eq!(self.tag(id), TypeTag::Conditional);
        let payload = self.payload(id) as usize;
        let slot = self
            .conditionals
            .get_mut_local(payload)
            .expect("validated conditional row must have a side-table entry");
        *slot = conditional;
    }

    /// Append an instantiation row (M25). Internal — `Interner` owns dedup.
    pub fn push_instantiation(
        &mut self,
        instantiation: InstantiationType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = self.instantiations.len() as u32;
        self.instantiations.push_local(instantiation);
        self.push(TypeTag::Instantiation, flags, payload)
    }

    /// Append an immutable class application. Internal — `Interner` owns dedup.
    pub fn push_class_instance(&mut self, instance: ClassInstanceType, flags: TypeFlags) -> TypeId {
        let payload = u32::try_from(self.class_instances.len())
            .expect("class-instance side table exceeds the u32 payload range");
        self.class_instances.push_local(instance);
        self.push(TypeTag::ClassInstance, flags, payload)
    }

    /// Append an immutable deferred indexed access. Internal — `Interner` owns dedup.
    pub fn push_deferred_indexed_access(
        &mut self,
        access: DeferredIndexedAccessType,
        flags: TypeFlags,
    ) -> TypeId {
        let payload = u32::try_from(self.deferred_indexed_accesses.len())
            .expect("deferred-indexed-access side table exceeds the u32 payload range");
        self.deferred_indexed_accesses.push_local(access);
        self.push(TypeTag::DeferredIndexedAccess, flags, payload)
    }

    pub fn push_declared_recipe(&mut self, recipe: DeclaredTypeRecipe) -> DeclaredRecipeId {
        let id = DeclaredRecipeId(
            u32::try_from(self.declared_recipes.len()).expect("declared recipe id fits u32"),
        );
        self.declared_recipes.push_local(recipe);
        id
    }

    pub fn push_declared(&mut self, declared: DeclaredType, flags: TypeFlags) -> TypeId {
        let payload = u32::try_from(self.declared_types.len()).expect("declared payload fits u32");
        self.declared_types.push_local(declared);
        self.push(TypeTag::Declared, flags, payload)
    }

    /// Append an infer-binder row (M25). Internal — `Interner` owns dedup (by index).
    /// The de Bruijn index is stored inline in `payload`.
    pub fn push_infer(&mut self, index: u32, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Infer, flags, index)
    }

    /// Append a mapped-type row (M26). Internal — `Interner` owns dedup (by the whole
    /// [`MappedType`]).
    pub fn push_mapped(&mut self, mapped: MappedType, flags: TypeFlags) -> TypeId {
        let payload = self.mapped.len() as u32;
        self.mapped.push_local(mapped);
        self.push(TypeTag::Mapped, flags, payload)
    }

    /// Replace a prevalidated reserved mapped body while preserving its `TypeId`.
    pub(super) fn set_mapped(&mut self, id: TypeId, mapped: MappedType) {
        assert_eq!(self.tag(id), TypeTag::Mapped);
        let payload = self.payload(id) as usize;
        let slot = self
            .mapped
            .get_mut_local(payload)
            .expect("validated mapped row must have a side-table entry");
        *slot = mapped;
    }

    /// Append a deferred-`keyof` row (M28). Internal — `Interner` owns dedup (by
    /// operand). The operand id is stored inline in `payload`.
    pub fn push_keyof(&mut self, operand: TypeId, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::Keyof, flags, operand.0)
    }

    /// Append a mapped-value placeholder row (M26). Internal — `Interner` owns dedup
    /// (identity is the tag alone; payload `0`).
    pub fn push_mapped_value(&mut self, flags: TypeFlags) -> TypeId {
        self.push(TypeTag::MappedValue, flags, 0)
    }

    /// Append a template-literal row (M27). Internal — `Interner` owns dedup (by the
    /// whole [`TemplateType`]).
    pub fn push_template(&mut self, template: TemplateType, flags: TypeFlags) -> TypeId {
        let payload = self.templates.len() as u32;
        self.templates.push_local(template);
        self.push(TypeTag::Template, flags, payload)
    }
}

impl Store {
    pub fn all_type_param_constraints(&self) -> Vec<(TypeParamId, TypeId)> {
        let mut constraints = self
            .type_param_constraints
            .iter()
            .map(|(&parameter, &constraint)| (parameter, constraint))
            .collect::<Vec<_>>();
        constraints.sort_unstable_by_key(|(parameter, _)| *parameter);
        constraints
    }

    pub fn local_type_param_constraints_for_test(
        &self,
    ) -> impl Iterator<Item = (TypeParamId, TypeId)> + '_ {
        self.type_param_constraints
            .local_iter()
            .map(|(&parameter, &constraint)| (parameter, constraint))
    }

    pub fn all_frozen_type_params(&self) -> Vec<TypeParamId> {
        let mut parameters = self.frozen_type_params.iter().copied().collect::<Vec<_>>();
        parameters.sort_unstable();
        parameters
    }

    pub fn local_frozen_type_params_for_test(&self) -> impl Iterator<Item = TypeParamId> + '_ {
        self.frozen_type_params.local_iter().copied()
    }

    pub fn all_template_name_ids(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.template_names.keys().copied()
    }

    pub fn all_declared_recipes(
        &self,
    ) -> impl Iterator<Item = (DeclaredRecipeId, &DeclaredTypeRecipe)> + '_ {
        self.declared_recipes
            .iter()
            .enumerate()
            .map(|(index, recipe)| {
                (
                    DeclaredRecipeId(u32::try_from(index).expect("recipe id fits u32")),
                    recipe,
                )
            })
    }

    pub fn declared_recipe_base_len(&self) -> usize {
        self.declared_recipes.base_len()
    }

    pub fn local_template_name_ids_for_test(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.template_names.local_iter().map(|(&id, _)| id)
    }
}
