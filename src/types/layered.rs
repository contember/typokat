//! Immutable-prefix containers with an isolated mutable suffix.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::ops::Index;
use std::sync::Arc;
#[cfg(test)]
use std::{cell::RefCell, collections::BTreeMap};

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum BaseWorkOperationForTest {
    SequentialScan,
    Materialize,
    Clone,
    Remap,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BaseWorkLedgerForTest {
    pub(crate) sequential_scans: BTreeMap<&'static str, u64>,
    pub(crate) materializations: BTreeMap<&'static str, u64>,
    pub(crate) clones: BTreeMap<&'static str, u64>,
    pub(crate) remaps: BTreeMap<&'static str, u64>,
}

#[cfg(test)]
thread_local! {
    static BASE_WORK_LEDGER: RefCell<Option<BaseWorkLedgerForTest>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn record_base_work_for_test(
    family: &'static str,
    operation: BaseWorkOperationForTest,
    rows: usize,
) {
    let rows = u64::try_from(rows).unwrap_or(u64::MAX);
    BASE_WORK_LEDGER.with_borrow_mut(|active| {
        let Some(ledger) = active.as_mut() else {
            return;
        };
        let counters = match operation {
            BaseWorkOperationForTest::SequentialScan => &mut ledger.sequential_scans,
            BaseWorkOperationForTest::Materialize => &mut ledger.materializations,
            BaseWorkOperationForTest::Clone => &mut ledger.clones,
            BaseWorkOperationForTest::Remap => &mut ledger.remaps,
        };
        let counter = counters.entry(family).or_default();
        *counter = counter.saturating_add(rows);
    });
}

#[cfg(test)]
fn record_full_view_base_scan_for_test(rows: usize) {
    if rows > 0 {
        record_base_work_for_test(
            "unattributed",
            BaseWorkOperationForTest::SequentialScan,
            rows,
        );
    }
}

#[cfg(test)]
pub(crate) struct BaseWorkScopeForTest;

#[cfg(test)]
impl BaseWorkScopeForTest {
    pub(crate) fn start(families: impl IntoIterator<Item = &'static str>) -> Self {
        let empty = families
            .into_iter()
            .map(|family| (family, 0))
            .collect::<BTreeMap<_, _>>();
        BASE_WORK_LEDGER.with_borrow_mut(|active| {
            assert!(active.is_none(), "base-work scopes do not nest");
            *active = Some(BaseWorkLedgerForTest {
                sequential_scans: empty.clone(),
                materializations: empty.clone(),
                clones: empty.clone(),
                remaps: empty,
            });
        });
        Self
    }

    pub(crate) fn finish(self) -> BaseWorkLedgerForTest {
        BASE_WORK_LEDGER
            .with_borrow_mut(Option::take)
            .expect("base-work scope is active")
    }
}

#[cfg(test)]
pub(crate) fn calibrate_base_work_ledger_for_test(
    sequential_scans: u64,
    materializations: u64,
    clones: u64,
    remaps: u64,
) -> BaseWorkLedgerForTest {
    const FAMILY: &str = "store.rows";
    let scope = BaseWorkScopeForTest::start([FAMILY]);
    let base = [1_u8];
    for _ in 0..sequential_scans {
        record_base_work_for_test(FAMILY, BaseWorkOperationForTest::SequentialScan, base.len());
        let _ = base.iter().count();
    }
    for _ in 0..materializations {
        record_base_work_for_test(FAMILY, BaseWorkOperationForTest::Materialize, base.len());
        let _ = base.to_vec();
    }
    for _ in 0..clones {
        record_base_work_for_test(FAMILY, BaseWorkOperationForTest::Clone, base.len());
        let _ = base.to_vec();
    }
    for _ in 0..remaps {
        record_base_work_for_test(FAMILY, BaseWorkOperationForTest::Remap, base.len());
        let _ = base.iter().enumerate().collect::<BTreeMap<_, _>>();
    }
    scope.finish()
}

#[cfg(test)]
thread_local! {
    static BASE_WRITE_ATTEMPTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_base_write_attempt_for_test() {
    BASE_WRITE_ATTEMPTS.set(BASE_WRITE_ATTEMPTS.get().saturating_add(1));
}

#[cfg(test)]
pub(crate) struct BaseWriteAttemptScopeForTest(u64);

#[cfg(test)]
impl BaseWriteAttemptScopeForTest {
    pub(crate) fn start() -> Self {
        Self(BASE_WRITE_ATTEMPTS.get())
    }

    pub(crate) fn finish(self) -> u64 {
        BASE_WRITE_ATTEMPTS.get().saturating_sub(self.0)
    }
}

struct LayeredIter<'a, T> {
    base: std::slice::Iter<'a, T>,
    local: std::slice::Iter<'a, T>,
}

impl<T> Clone for LayeredIter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            local: self.local.clone(),
        }
    }
}

impl<'a, T> Iterator for LayeredIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().or_else(|| self.local.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.base.len() + self.local.len();
        (len, Some(len))
    }
}

impl<T> DoubleEndedIterator for LayeredIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.local.next_back().or_else(|| self.base.next_back())
    }
}

impl<T> ExactSizeIterator for LayeredIter<'_, T> {}

pub(crate) struct LayeredVec<T> {
    base: Arc<[T]>,
    local: Vec<T>,
    sealed: bool,
}

impl<T: Clone> Clone for LayeredVec<T> {
    fn clone(&self) -> Self {
        Self {
            base: Arc::clone(&self.base),
            local: self.local.clone(),
            sealed: self.sealed,
        }
    }
}

impl<T> Default for LayeredVec<T> {
    fn default() -> Self {
        Self {
            base: Arc::from([]),
            local: Vec::new(),
            sealed: false,
        }
    }
}

impl<T> LayeredVec<T> {
    pub(crate) fn len(&self) -> usize {
        self.base.len() + self.local.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn base_len(&self) -> usize {
        self.base.len()
    }

    pub(crate) fn local_len(&self) -> usize {
        self.local.len()
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        if index < self.base.len() {
            self.base.get(index)
        } else {
            self.local.get(index - self.base.len())
        }
    }

    pub(crate) fn get_mut_local(&mut self, index: usize) -> Option<&mut T> {
        #[cfg(test)]
        if index < self.base.len() {
            record_base_write_attempt_for_test();
        }
        self.local.get_mut(index.checked_sub(self.base.len())?)
    }

    pub(crate) fn push_local(&mut self, value: T) -> usize {
        let index = self.len();
        self.local.push(value);
        index
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator + Clone {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        LayeredIter {
            base: self.base.iter(),
            local: self.local.iter(),
        }
    }

    pub(crate) fn local_iter(&self) -> std::slice::Iter<'_, T> {
        self.local.iter()
    }

    pub(crate) fn local_iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.local.iter_mut()
    }

    pub(crate) fn local_slice_mut(&mut self) -> &mut [T] {
        &mut self.local
    }

    pub(crate) fn clear_local(&mut self) {
        self.local.clear();
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        if self.sealed || !self.base.is_empty() {
            return Err("layered vector is already sealed");
        }
        self.base = Arc::from(std::mem::take(&mut self.local));
        self.sealed = true;
        Ok(())
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        if !self.sealed {
            return Err("layered vector base is not sealed");
        }
        if !self.local.is_empty() {
            return Err("layered vector has an unpublished suffix");
        }
        Ok(Self {
            base: Arc::clone(&self.base),
            local: Vec::new(),
            sealed: true,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.base, &other.base)
    }
}

impl<T> From<Vec<T>> for LayeredVec<T> {
    fn from(local: Vec<T>) -> Self {
        Self {
            base: Arc::from([]),
            local,
            sealed: false,
        }
    }
}

impl<T> Index<usize> for LayeredVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("layered vector index in range")
    }
}

impl<'a, T> IntoIterator for &'a LayeredVec<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Chain<std::slice::Iter<'a, T>, std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.iter().chain(self.local.iter())
    }
}

pub(crate) struct LayeredMap<K, V> {
    base: Arc<FxHashMap<K, V>>,
    local: FxHashMap<K, V>,
    sealed: bool,
}

impl<K: Clone, V: Clone> Clone for LayeredMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            base: Arc::clone(&self.base),
            local: self.local.clone(),
            sealed: self.sealed,
        }
    }
}

impl<K, V> Default for LayeredMap<K, V> {
    fn default() -> Self {
        Self {
            base: Arc::new(FxHashMap::default()),
            local: FxHashMap::default(),
            sealed: false,
        }
    }
}

impl<K: Eq + Hash, V> LayeredMap<K, V> {
    pub(crate) fn len(&self) -> usize {
        self.base.len() + self.local.len()
    }

    pub(crate) fn local_len(&self) -> usize {
        self.local.len()
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.base.get(key).or_else(|| self.local.get(key))
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.base.contains_key(key) || self.local.contains_key(key)
    }

    pub(crate) fn insert_local(&mut self, key: K, value: V) -> Result<Option<V>, &'static str> {
        if self.base.contains_key(&key) {
            #[cfg(test)]
            record_base_write_attempt_for_test();
            return Err("layered map cannot replace a sealed entry");
        }
        Ok(self.local.insert(key, value))
    }

    pub(crate) fn remove_local(&mut self, key: &K) -> Result<Option<V>, &'static str> {
        if self.base.contains_key(key) {
            #[cfg(test)]
            record_base_write_attempt_for_test();
            return Err("layered map cannot remove a sealed entry");
        }
        Ok(self.local.remove(key))
    }

    pub(crate) fn entry_local(&mut self, key: K) -> Result<Entry<'_, K, V>, &'static str> {
        if self.base.contains_key(&key) {
            #[cfg(test)]
            record_base_write_attempt_for_test();
            return Err("layered map cannot replace a sealed entry");
        }
        Ok(self.local.entry(key))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.iter().chain(self.local.iter())
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.keys().chain(self.local.keys())
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.values().chain(self.local.values())
    }

    pub(crate) fn local_iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.local.iter()
    }

    pub(crate) fn local_values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.local.values_mut()
    }

    pub(crate) fn clear_local(&mut self) {
        self.local.clear();
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        if self.sealed || !self.base.is_empty() {
            return Err("layered map is already sealed");
        }
        self.base = Arc::new(std::mem::take(&mut self.local));
        self.sealed = true;
        Ok(())
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        if !self.sealed {
            return Err("layered map base is not sealed");
        }
        if !self.local.is_empty() {
            return Err("layered map has an unpublished suffix");
        }
        Ok(Self {
            base: Arc::clone(&self.base),
            local: FxHashMap::default(),
            sealed: true,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.base, &other.base)
    }
}

impl<K, V> From<FxHashMap<K, V>> for LayeredMap<K, V> {
    fn from(local: FxHashMap<K, V>) -> Self {
        Self {
            base: Arc::new(FxHashMap::default()),
            local,
            sealed: false,
        }
    }
}

impl<'a, K, V> IntoIterator for &'a LayeredMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::iter::Chain<
        std::collections::hash_map::Iter<'a, K, V>,
        std::collections::hash_map::Iter<'a, K, V>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.iter().chain(self.local.iter())
    }
}

pub(crate) struct LayeredSet<T> {
    base: Arc<FxHashSet<T>>,
    local: FxHashSet<T>,
    sealed: bool,
}

impl<T: Clone> Clone for LayeredSet<T> {
    fn clone(&self) -> Self {
        Self {
            base: Arc::clone(&self.base),
            local: self.local.clone(),
            sealed: self.sealed,
        }
    }
}

impl<T> Default for LayeredSet<T> {
    fn default() -> Self {
        Self {
            base: Arc::new(FxHashSet::default()),
            local: FxHashSet::default(),
            sealed: false,
        }
    }
}

impl<T: Eq + Hash> LayeredSet<T> {
    pub(crate) fn local_len(&self) -> usize {
        self.local.len()
    }

    pub(crate) fn contains(&self, value: &T) -> bool {
        self.base.contains(value) || self.local.contains(value)
    }

    pub(crate) fn insert_local(&mut self, value: T) -> Result<bool, &'static str> {
        if self.base.contains(&value) {
            #[cfg(test)]
            record_base_write_attempt_for_test();
            return Err("layered set cannot replace a sealed entry");
        }
        Ok(self.local.insert(value))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.iter().chain(self.local.iter())
    }

    pub(crate) fn local_iter(&self) -> impl Iterator<Item = &T> {
        self.local.iter()
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        if self.sealed || !self.base.is_empty() {
            return Err("layered set is already sealed");
        }
        self.base = Arc::new(std::mem::take(&mut self.local));
        self.sealed = true;
        Ok(())
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        if !self.sealed {
            return Err("layered set base is not sealed");
        }
        if !self.local.is_empty() {
            return Err("layered set has an unpublished suffix");
        }
        Ok(Self {
            base: Arc::clone(&self.base),
            local: FxHashSet::default(),
            sealed: true,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.base, &other.base)
    }
}

impl<T> From<FxHashSet<T>> for LayeredSet<T> {
    fn from(local: FxHashSet<T>) -> Self {
        Self {
            base: Arc::new(FxHashSet::default()),
            local,
            sealed: false,
        }
    }
}

impl<'a, T> IntoIterator for &'a LayeredSet<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Chain<
        std::collections::hash_set::Iter<'a, T>,
        std::collections::hash_set::Iter<'a, T>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        #[cfg(test)]
        record_full_view_base_scan_for_test(self.base.len());
        self.base.iter().chain(self.local.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_clone_shares_base_and_isolates_local_suffix() {
        let mut base = LayeredVec::default();
        base.push_local(String::from("base"));
        base.freeze_as_base().expect("vector base seals");

        let mut original = base.fork_delta().expect("vector delta forks");
        original.push_local(String::from("original"));
        let mut cloned = original.clone();

        assert!(original.shares_base_with(&cloned));
        assert!(cloned.is_sealed());
        assert!(cloned.freeze_as_base().is_err());
        assert!(cloned.fork_delta().is_err());
        *cloned
            .get_mut_local(1)
            .expect("cloned suffix remains mutable") = String::from("cloned");
        cloned.push_local(String::from("clone-only"));

        assert_eq!(
            original.iter().map(String::as_str).collect::<Vec<_>>(),
            ["base", "original"]
        );
        assert_eq!(
            cloned.iter().map(String::as_str).collect::<Vec<_>>(),
            ["base", "cloned", "clone-only"]
        );
    }

    #[test]
    fn map_clone_shares_base_and_isolates_local_suffix() {
        let mut base = LayeredMap::default();
        base.insert_local(String::from("base"), 1)
            .expect("standalone base inserts");
        base.freeze_as_base().expect("map base seals");

        let mut original = base.fork_delta().expect("map delta forks");
        original
            .insert_local(String::from("local"), 2)
            .expect("local entry inserts");
        let mut cloned = original.clone();

        assert!(original.shares_base_with(&cloned));
        assert!(cloned.freeze_as_base().is_err());
        assert!(cloned.fork_delta().is_err());
        cloned
            .insert_local(String::from("local"), 3)
            .expect("clone replaces its suffix entry");
        cloned
            .insert_local(String::from("clone-only"), 4)
            .expect("clone adds a suffix entry");

        assert_eq!(original.get(&String::from("base")), Some(&1));
        assert_eq!(original.get(&String::from("local")), Some(&2));
        assert!(!original.contains_key(&String::from("clone-only")));
        assert_eq!(cloned.get(&String::from("local")), Some(&3));
        assert_eq!(cloned.get(&String::from("clone-only")), Some(&4));
    }

    #[test]
    fn set_clone_shares_base_and_isolates_local_suffix() {
        let mut base = LayeredSet::default();
        base.insert_local(String::from("base"))
            .expect("standalone base inserts");
        base.freeze_as_base().expect("set base seals");

        let mut original = base.fork_delta().expect("set delta forks");
        original
            .insert_local(String::from("local"))
            .expect("local value inserts");
        let mut cloned = original.clone();

        assert!(original.shares_base_with(&cloned));
        assert!(cloned.freeze_as_base().is_err());
        assert!(cloned.fork_delta().is_err());
        cloned
            .insert_local(String::from("clone-only"))
            .expect("clone adds a suffix value");

        assert!(original.contains(&String::from("base")));
        assert!(original.contains(&String::from("local")));
        assert!(!original.contains(&String::from("clone-only")));
        assert!(cloned.contains(&String::from("clone-only")));
    }

    #[test]
    fn unsealed_clone_remains_unsealed_with_an_independent_suffix() {
        let mut original = LayeredVec::default();
        original.push_local(1_u32);
        let mut cloned = original.clone();
        cloned.push_local(2);

        assert!(original.shares_base_with(&cloned));
        assert!(!original.is_sealed());
        assert!(!cloned.is_sealed());
        assert!(original.fork_delta().is_err());
        assert!(cloned.fork_delta().is_err());
        assert_eq!(original.iter().copied().collect::<Vec<_>>(), [1]);
        assert_eq!(cloned.iter().copied().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn borrowed_into_iter_records_every_full_frozen_view() {
        let mut vector = LayeredVec::from(vec![1_u8]);
        vector.freeze_as_base().expect("vector base seals");
        let vector = vector.fork_delta().expect("vector delta forks");

        let mut map = LayeredMap::default();
        map.insert_local(1_u8, 1_u8).expect("map base inserts");
        map.freeze_as_base().expect("map base seals");
        let map = map.fork_delta().expect("map delta forks");

        let mut set = LayeredSet::default();
        set.insert_local(1_u8).expect("set base inserts");
        set.freeze_as_base().expect("set base seals");
        let set = set.fork_delta().expect("set delta forks");

        let scope = BaseWorkScopeForTest::start(["unattributed"]);
        assert_eq!((&vector).into_iter().count(), 1);
        assert_eq!((&map).into_iter().count(), 1);
        assert_eq!((&set).into_iter().count(), 1);
        let ledger = scope.finish();

        assert_eq!(ledger.sequential_scans["unattributed"], 3);
    }

    #[test]
    fn local_iterators_do_not_report_frozen_scans() {
        let mut vector = LayeredVec::from(vec![1_u8]);
        vector.freeze_as_base().expect("vector base seals");
        let vector = vector.fork_delta().expect("vector delta forks");

        let mut map = LayeredMap::default();
        map.insert_local(1_u8, 1_u8).expect("map base inserts");
        map.freeze_as_base().expect("map base seals");
        let map = map.fork_delta().expect("map delta forks");

        let mut set = LayeredSet::default();
        set.insert_local(1_u8).expect("set base inserts");
        set.freeze_as_base().expect("set base seals");
        let set = set.fork_delta().expect("set delta forks");

        let scope = BaseWorkScopeForTest::start(["unattributed"]);
        assert_eq!(vector.local_iter().count(), 0);
        assert_eq!(map.local_iter().count(), 0);
        assert_eq!(set.local_iter().count(), 0);
        let ledger = scope.finish();

        assert_eq!(ledger.sequential_scans["unattributed"], 0);
    }
}
