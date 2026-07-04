//! The type store: a struct-of-arrays (SoA) arena keyed by `TypeId(u32)`.
//!
//! Architecture §3.1 / mvp-plan §4.1. The hot attributes (`tag`, `flags`) live
//! in parallel vecs indexed directly by `TypeId`, so scanning/filtering is
//! cache-friendly. The variable-size data lives in cold side-tables
//! (`literals`, `objects`, `unions`, `functions`) selected by `tag`, addressed
//! by `payload`.
//!
//! The store is append-only: a `TypeId` is a stable arena index for the life of
//! the process. No `Rc`, no `RefCell`, no cycles — exactly the index-into-arena
//! discipline the architecture insists on.

use crate::types::hash::StableHash;
use crate::types::repr::{
    ArrayType, FunctionType, IntrinsicKind, LiteralValue, ObjectType, TupleType, TypeFlags,
    TypeParamId, TypeParamType, TypeTag,
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

    /// Erase a type parameter's constraint (M24 — the `TK2313` circularity fix): a
    /// parameter whose constraint chain revisits itself records **no** constraint, so
    /// the degenerate cycle never feeds the relation engine's assume-true stack.
    /// Internal — the checker calls it through
    /// `Interner::remove_type_param_constraint` right after the circularity pass.
    pub(crate) fn remove_type_param_constraint(&mut self, id: TypeParamId) {
        self.type_param_constraints.remove(&id);
    }

    /// The members of a union type (canonical: flattened, sorted by `TypeId`,
    /// deduped, `never`-free), or `None` if `id` is not a union.
    pub fn union_members(&self, id: TypeId) -> Option<&[TypeId]> {
        if self.tag(id) != TypeTag::Union {
            return None;
        }
        self.unions.get(self.payload(id) as usize).map(|m| &m[..])
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

    /// Replace the body of an existing `Object` row in place (M5 nominal-interface
    /// reserve-then-fill — `Interner::fill_object`). The `tag`/`flags`/`payload`
    /// columns are untouched, so the type's `TypeId` and identity are preserved;
    /// only the cold side-table entry the payload points at is overwritten. A no-op
    /// if `id` is not an `Object` row (defensive — never expected, the interner
    /// only calls this on ids it reserved as objects).
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
}
