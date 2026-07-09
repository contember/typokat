//! Relation verdict cache (architecture §6.1).
//!
//! Stable `TypeId`s make each key three cheap integers. Cache lifetime stays local
//! to `RelationCache`, separating durable relations from short-lived narrowed types.

use crate::relate::relation::RelationKind;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

/// The three-`u32` cache key: `(src, tgt, relation)`. Different `RelationKind`s
/// are distinct relations and must not share an entry (architecture §6.1).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RelationKey {
    pub src: u32,
    pub tgt: u32,
    pub kind: RelationKind,
}

impl RelationKey {
    #[inline]
    pub fn new(src: TypeId, tgt: TypeId, kind: RelationKind) -> Self {
        RelationKey {
            src: src.0,
            tgt: tgt.0,
            kind,
        }
    }
}

/// Maps a decided relation to its boolean verdict; failure reasons are rebuilt by
/// the engine so the hot cache stays compact.
#[derive(Default)]
pub struct RelationCache {
    entries: FxHashMap<RelationKey, bool>,
}

impl RelationCache {
    pub fn new() -> Self {
        RelationCache::default()
    }

    #[inline]
    pub fn get(&self, key: RelationKey) -> Option<bool> {
        self.entries.get(&key).copied()
    }

    #[inline]
    pub fn insert(&mut self, key: RelationKey, verdict: bool) {
        self.entries.insert(key, verdict);
    }

    /// Number of cached relations (used by tests / future cache-pressure tuning).
    #[allow(dead_code)] // TODO(§6.2): cache-lifetime instrumentation.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no decided relations.
    #[allow(dead_code)] // TODO(§6.2): cache-lifetime instrumentation.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
