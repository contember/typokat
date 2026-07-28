//! Relation verdict cache (architecture §6.1).
//!
//! Stable `TypeId`s make each key three cheap integers. Cache lifetime stays local
//! to `RelationCache`, separating durable relations from short-lived narrowed types.

use crate::relate::relation::RelationKind;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static RELATION_CACHE_WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static RELATION_CACHE_DEPTH: std::cell::RefCell<RelationCacheDepthMeasure> =
        std::cell::RefCell::new(RelationCacheDepthMeasure::new());
    static RELATION_CACHE_DEPTH_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Layer walk observed by `get`, so "depth stays bounded" is measured, not claimed.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug)]
pub struct RelationCacheDepthMeasure {
    pub lookups: u64,
    pub max_depth: u32,
    histogram: Vec<u64>,
}

#[cfg(any(test, feature = "test-utils"))]
impl RelationCacheDepthMeasure {
    const BUCKETS: usize = 33;

    fn new() -> Self {
        Self {
            lookups: 0,
            max_depth: 0,
            histogram: vec![0; Self::BUCKETS],
        }
    }

    /// Depth at or below which half the lookups resolved.
    pub fn median_depth(&self) -> usize {
        let mut seen = 0;
        for (depth, count) in self.histogram.iter().enumerate() {
            seen += count;
            if seen * 2 >= self.lookups {
                return depth;
            }
        }
        0
    }
}

#[cfg(not(any(test, feature = "test-utils")))]
#[inline(always)]
fn record_relation_cache_depth(_depth: u32) {}

#[cfg(any(test, feature = "test-utils"))]
fn record_relation_cache_depth(depth: u32) {
    if !RELATION_CACHE_DEPTH_ENABLED.with(std::cell::Cell::get) {
        return;
    }
    RELATION_CACHE_DEPTH.with(|measure| {
        let mut measure = measure.borrow_mut();
        measure.lookups += 1;
        measure.max_depth = measure.max_depth.max(depth);
        let bucket = (depth as usize).min(RelationCacheDepthMeasure::BUCKETS - 1);
        measure.histogram[bucket] += 1;
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn start_relation_cache_depth_measure() {
    RELATION_CACHE_DEPTH.with(|measure| *measure.borrow_mut() = RelationCacheDepthMeasure::new());
    RELATION_CACHE_DEPTH_ENABLED.set(true);
}

#[cfg(any(test, feature = "test-utils"))]
pub fn finish_relation_cache_depth_measure() -> RelationCacheDepthMeasure {
    RELATION_CACHE_DEPTH_ENABLED.set(false);
    RELATION_CACHE_DEPTH.with(|measure| measure.borrow().clone())
}

#[cfg(any(test, feature = "test-utils"))]
fn record_relation_cache_writes_for_test(count: usize) {
    RELATION_CACHE_WRITES.set(
        RELATION_CACHE_WRITES
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub struct RelationCacheWriteScopeForTest(u64);

#[cfg(any(test, feature = "test-utils"))]
impl RelationCacheWriteScopeForTest {
    pub fn start() -> Self {
        Self(RELATION_CACHE_WRITES.get())
    }

    pub fn finish(self) -> u64 {
        RELATION_CACHE_WRITES.get().saturating_sub(self.0)
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationCacheWriteCalibrationForTest {
    insert: u64,
    promote: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl RelationCacheWriteCalibrationForTest {
    pub fn total(self) -> u64 {
        self.insert.saturating_add(self.promote)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn calibrate_relation_cache_writes_for_test() -> RelationCacheWriteCalibrationForTest {
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
///
/// A speculative caller layers the cache instead of copying it: `savepoint` pushes
/// the current entries down into a shared immutable base, so `rollback` drops only
/// the speculative layer and `commit` folds it back down. Depth is bounded by open
/// transaction nesting because commit collapses each layer as it closes.
#[derive(Clone, Default)]
pub struct RelationCache {
    base: Option<Arc<RelationCache>>,
    entries: FxHashMap<RelationKey, bool>,
}

impl RelationCache {
    pub fn new() -> Self {
        RelationCache::default()
    }

    #[inline]
    pub fn get(&self, key: RelationKey) -> Option<bool> {
        let (verdict, depth) = self.lookup(key);
        record_relation_cache_depth(depth);
        verdict
    }

    /// The layer walk itself, uninstrumented so bookkeeping never counts as a
    /// relation lookup. Returns how many layers below the local one it visited.
    #[inline]
    fn lookup(&self, key: RelationKey) -> (Option<bool>, u32) {
        if let Some(verdict) = self.entries.get(&key) {
            return (Some(*verdict), 0);
        }
        let mut layer = self.base.as_deref();
        let mut depth = 0u32;
        while let Some(current) = layer {
            depth += 1;
            if let Some(verdict) = current.entries.get(&key) {
                return (Some(*verdict), depth);
            }
            layer = current.base.as_deref();
        }
        (None, depth)
    }

    #[inline]
    pub fn insert(&mut self, key: RelationKey, verdict: bool) {
        #[cfg(any(test, feature = "test-utils"))]
        record_relation_cache_writes_for_test(1);
        self.entries.insert(key, verdict);
    }

    /// Promote a completed query's local decisions into this durable cache.
    /// Exhausted or otherwise tainted queries drop their local cache instead.
    pub(crate) fn promote(&mut self, pending: RelationCache) {
        #[cfg(any(test, feature = "test-utils"))]
        record_relation_cache_writes_for_test(pending.entries.len());
        debug_assert!(
            pending.base.is_none(),
            "a pending query cache is always unlayered"
        );
        self.entries.extend(pending.entries);
    }

    /// Open a speculative layer. O(1): the settled entries become a shared base.
    pub fn savepoint(&mut self) {
        let settled = std::mem::take(self);
        self.base = Some(Arc::new(settled));
    }

    /// Fold the speculative layer into the layer below it. O(this layer).
    pub fn commit(&mut self) {
        let mut settled = self.take_savepoint_base();
        settled.entries.extend(self.entries.drain());
        *self = settled;
    }

    /// Drop the speculative layer, restoring the cache to its savepoint. O(1).
    pub fn rollback(&mut self) {
        *self = self.take_savepoint_base();
    }

    /// Reclaim the base a savepoint pushed down. The layer is never shared with
    /// another owner, so unwrapping the `Arc` stays a move rather than a copy.
    fn take_savepoint_base(&mut self) -> RelationCache {
        let base = self
            .base
            .take()
            .expect("a relation-cache layer closes exactly one savepoint");
        Arc::try_unwrap(base).unwrap_or_else(|shared| (*shared).clone())
    }

    /// Number of cached relations (used by tests / future cache-pressure tuning).
    #[allow(dead_code)] // TODO(§6.2): cache-lifetime instrumentation.
    pub fn len(&self) -> usize {
        let Some(base) = self.base.as_deref() else {
            return self.entries.len();
        };
        let shadowed = self
            .entries
            .keys()
            .filter(|key| base.lookup(**key).0.is_some())
            .count();
        base.len() + self.entries.len() - shadowed
    }

    /// Whether the cache holds no decided relations.
    #[allow(dead_code)] // TODO(§6.2): cache-lifetime instrumentation.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.base.as_deref().is_none_or(RelationCache::is_empty)
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
