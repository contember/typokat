//! Relation verdict cache (architecture §6.1).
//!
//! Stable `TypeId`s make each key three cheap integers. Cache lifetime stays local
//! to `RelationCache`, separating durable relations from short-lived narrowed types.

use crate::relate::relation::RelationKind;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

#[cfg(test)]
thread_local! {
    static RELATION_CACHE_WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_relation_cache_writes_for_test(count: usize) {
    RELATION_CACHE_WRITES.set(
        RELATION_CACHE_WRITES
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(crate) struct RelationCacheWriteScopeForTest(u64);

#[cfg(test)]
impl RelationCacheWriteScopeForTest {
    pub(crate) fn start() -> Self {
        Self(RELATION_CACHE_WRITES.get())
    }

    pub(crate) fn finish(self) -> u64 {
        RELATION_CACHE_WRITES.get().saturating_sub(self.0)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelationCacheWriteCalibrationForTest {
    pub(crate) insert: u64,
    pub(crate) promote: u64,
}

#[cfg(test)]
impl RelationCacheWriteCalibrationForTest {
    pub(crate) fn total(self) -> u64 {
        self.insert.saturating_add(self.promote)
    }
}

#[cfg(test)]
pub(crate) fn calibrate_relation_cache_writes_for_test() -> RelationCacheWriteCalibrationForTest {
    let insert_scope = RelationCacheWriteScopeForTest::start();
    let mut cache = RelationCache::new();
    cache.insert(
        RelationKey::new(TypeId(1), TypeId(2), RelationKind::Assignable),
        true,
    );
    let insert = insert_scope.finish();

    let promote_scope = RelationCacheWriteScopeForTest::start();
    let mut pending = RelationCache::new();
    pending.entries.insert(
        RelationKey::new(TypeId(3), TypeId(4), RelationKind::Assignable),
        true,
    );
    cache.promote(pending);
    let promote = promote_scope.finish();

    RelationCacheWriteCalibrationForTest { insert, promote }
}

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
#[derive(Clone, Default)]
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
        #[cfg(test)]
        record_relation_cache_writes_for_test(1);
        self.entries.insert(key, verdict);
    }

    /// Promote a completed query's local decisions into this durable cache.
    /// Exhausted or otherwise tainted queries drop their local cache instead.
    pub(crate) fn promote(&mut self, pending: RelationCache) {
        #[cfg(test)]
        record_relation_cache_writes_for_test(pending.entries.len());
        self.entries.extend(pending.entries);
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

#[cfg(test)]
mod calibration_tests {
    use super::*;

    #[test]
    fn calibration_exercises_insert_and_promote_hooks() {
        assert_eq!(
            calibrate_relation_cache_writes_for_test(),
            RelationCacheWriteCalibrationForTest {
                insert: 1,
                promote: 1,
            }
        );
    }
}
