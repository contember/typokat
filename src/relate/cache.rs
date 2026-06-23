//! The relation cache (architecture §6.1, mvp-plan §4.4).
//!
//! With stable `TypeId`s the cache key is three `u32`s — extremely cheap — and
//! good hash-consing makes it brutally effective because structurally equal
//! types collapse to one key. This may be the single largest perf element of the
//! whole checker, so it gets its own module from day 1 even though M0 barely
//! populates it.
//!
//! Cache *lifetime* (architecture §6.2) — separating durable relations from the
//! swarm of short-lived narrowed types — is a known later concern. For the MVP a
//! single map is correct (no narrowing yet); the seam is the `RelationCache`
//! boundary, so introducing a volatile/durable split later is a local change.

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

/// Maps a decided relation to its boolean verdict. The reason chain for a
/// failure is reconstructed by the engine (M0 reasons are leaves); when reason
/// caching becomes worthwhile the value type is widened here without touching
/// call sites.
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
