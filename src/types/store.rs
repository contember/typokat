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
    FunctionType, IntrinsicKind, LiteralValue, ObjectType, TypeFlags, TypeTag,
};

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
    // The cold side-tables below are reserved foundation state: the SoA layout
    // is fixed from day 1 (mvp-plan §1.3) so later milestones add `push_*`
    // helpers without restructuring the arena. They have no readers yet.
    /// TODO(M2): object types.
    #[allow(dead_code)]
    objects: Vec<ObjectType>,
    /// TODO(M4): union members (already canonicalized: sorted, deduped, flat).
    #[allow(dead_code)]
    unions: Vec<Box<[TypeId]>>,
    /// TODO(M3): function types.
    #[allow(dead_code)]
    functions: Vec<FunctionType>,

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

    // TODO(M2/M3/M4): push_object / push_function / push_union helpers that
    // write the cold side-table then the hot row, mirroring push_literal.
}
