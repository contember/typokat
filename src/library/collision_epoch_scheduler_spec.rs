//! RED contract for sparse snapshot-native collision closure scheduling.

use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::binder::namespace::NamespaceId;
use crate::check::checker::replay_index::{
    schedule_sparse_collision_closure, AdmittedCollisionReplayIndex, ReplayOwner, RootSlotKind,
    SparseReplayGraphAccess,
};
use crate::types::repr::ClassId;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

const POISON_SCCS: u32 = 50_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SchedulerWork {
    seed_probes: u64,
    owner_to_scc_probes: u64,
    queue_pushes: u64,
    queue_pops: u64,
    reverse_scc_slice_probes: u64,
    reverse_edge_probes: u64,
    affected_scc_inserts: u64,
    owner_slice_probes: u64,
    affected_owner_emissions: u64,
    full_owner_scans: u64,
    full_scc_scans: u64,
    full_edge_scans: u64,
    base_sized_allocations: u64,
}

#[derive(Default)]
struct WorkCells {
    seed_probes: Cell<u64>,
    owner_to_scc_probes: Cell<u64>,
    queue_pushes: Cell<u64>,
    queue_pops: Cell<u64>,
    reverse_scc_slice_probes: Cell<u64>,
    reverse_edge_probes: Cell<u64>,
    affected_scc_inserts: Cell<u64>,
    owner_slice_probes: Cell<u64>,
    affected_owner_emissions: Cell<u64>,
    full_owner_scans: Cell<u64>,
    full_scc_scans: Cell<u64>,
    full_edge_scans: Cell<u64>,
    base_sized_allocations: Cell<u64>,
}

impl WorkCells {
    fn increment(cell: &Cell<u64>) {
        cell.set(cell.get() + 1);
    }

    fn snapshot(&self) -> SchedulerWork {
        SchedulerWork {
            seed_probes: self.seed_probes.get(),
            owner_to_scc_probes: self.owner_to_scc_probes.get(),
            queue_pushes: self.queue_pushes.get(),
            queue_pops: self.queue_pops.get(),
            reverse_scc_slice_probes: self.reverse_scc_slice_probes.get(),
            reverse_edge_probes: self.reverse_edge_probes.get(),
            affected_scc_inserts: self.affected_scc_inserts.get(),
            owner_slice_probes: self.owner_slice_probes.get(),
            affected_owner_emissions: self.affected_owner_emissions.get(),
            full_owner_scans: self.full_owner_scans.get(),
            full_scc_scans: self.full_scc_scans.get(),
            full_edge_scans: self.full_edge_scans.get(),
            base_sized_allocations: self.base_sized_allocations.get(),
        }
    }
}

struct RecordingSparseGraph {
    owner_to_scc: BTreeMap<ReplayOwner, u32>,
    scc_owners: BTreeMap<u32, Vec<ReplayOwner>>,
    reverse_sccs: BTreeMap<u32, Vec<u32>>,
    poison_sccs: Vec<u32>,
    work: WorkCells,
}

impl RecordingSparseGraph {
    fn synthetic() -> (Self, ReplayOwner, Vec<ReplayOwner>) {
        let seed_owner = ReplayOwner::TypeGroup(TypeGroupId(101));
        let second = ReplayOwner::Value(ValueStorageId(102));
        let third = ReplayOwner::Namespace(NamespaceId(103));
        let fourth = ReplayOwner::Class(ClassId(104));

        let graph = Self {
            owner_to_scc: BTreeMap::from([(seed_owner, 0), (second, 1), (third, 1), (fourth, 2)]),
            scc_owners: BTreeMap::from([
                (0, vec![seed_owner]),
                (1, vec![second, third]),
                (2, vec![fourth]),
            ]),
            reverse_sccs: BTreeMap::from([(0, vec![1]), (1, vec![2]), (2, Vec::new())]),
            poison_sccs: (3..3 + POISON_SCCS).collect(),
            work: WorkCells::default(),
        };
        (graph, seed_owner, vec![seed_owner, second, third, fourth])
    }

    fn assert_live_scc(&self, scc: u32) {
        // A guessed numeric full scan necessarily enters the materially retained poison range.
        assert!(
            self.poison_sccs.binary_search(&scc).is_err(),
            "the scheduler touched disconnected poison SCC {scc}"
        );
    }
}

impl SparseReplayGraphAccess for RecordingSparseGraph {
    fn observe_seed_probe(&self, owner: ReplayOwner) {
        assert!(self.owner_to_scc.contains_key(&owner));
        WorkCells::increment(&self.work.seed_probes);
    }

    fn owner_scc(&self, owner: ReplayOwner) -> Option<u32> {
        WorkCells::increment(&self.work.owner_to_scc_probes);
        self.owner_to_scc.get(&owner).copied()
    }

    fn reverse_sccs(&self, scc: u32) -> Option<&[u32]> {
        self.assert_live_scc(scc);
        WorkCells::increment(&self.work.reverse_scc_slice_probes);
        self.reverse_sccs.get(&scc).map(Vec::as_slice)
    }

    fn scc_owners(&self, scc: u32) -> Option<&[ReplayOwner]> {
        self.assert_live_scc(scc);
        WorkCells::increment(&self.work.owner_slice_probes);
        self.scc_owners.get(&scc).map(Vec::as_slice)
    }

    fn observe_queue_push(&self, scc: u32) {
        self.assert_live_scc(scc);
        WorkCells::increment(&self.work.queue_pushes);
    }

    fn observe_queue_pop(&self, scc: u32) {
        self.assert_live_scc(scc);
        WorkCells::increment(&self.work.queue_pops);
    }

    fn observe_reverse_edge_probe(&self, dependency: u32, consumer: u32) {
        self.assert_live_scc(dependency);
        self.assert_live_scc(consumer);
        WorkCells::increment(&self.work.reverse_edge_probes);
    }

    fn observe_affected_scc_insert(&self, scc: u32) {
        self.assert_live_scc(scc);
        WorkCells::increment(&self.work.affected_scc_inserts);
    }

    fn observe_affected_owner_emission(&self, owner: ReplayOwner) {
        assert!(self.owner_to_scc.contains_key(&owner));
        WorkCells::increment(&self.work.affected_owner_emissions);
    }
}

struct FullScanOracle {
    owners: Vec<ReplayOwner>,
    sccs: usize,
    reverse_edges: usize,
}

fn full_scan_oracle(index: &AdmittedCollisionReplayIndex, seed: ReplayOwner) -> FullScanOracle {
    let mut affected = BTreeSet::from([seed]);
    loop {
        let before = affected.len();
        for component in &index.scc_membership {
            if component
                .owners
                .iter()
                .any(|owner| affected.contains(owner))
            {
                affected.extend(component.owners.iter().copied());
            }
        }
        for edge in &index.reverse_edges {
            if affected.contains(&edge.dependency) {
                affected.insert(edge.consumer);
            }
        }
        if affected.len() == before {
            break;
        }
    }

    let owner_to_scc = index
        .scc_membership
        .iter()
        .flat_map(|component| {
            component
                .owners
                .iter()
                .map(move |owner| (*owner, component.replay_ordinal))
        })
        .collect::<BTreeMap<_, _>>();
    let affected_sccs = affected
        .iter()
        .map(|owner| owner_to_scc[owner])
        .collect::<BTreeSet<_>>();
    let reverse_edges = index
        .reverse_edges
        .iter()
        .filter_map(|edge| {
            let dependency = owner_to_scc[&edge.dependency];
            let consumer = owner_to_scc[&edge.consumer];
            (dependency != consumer
                && affected_sccs.contains(&dependency)
                && affected_sccs.contains(&consumer))
            .then_some((dependency, consumer))
        })
        .collect::<BTreeSet<_>>();
    FullScanOracle {
        owners: affected.into_iter().collect(),
        sccs: affected_sccs.len(),
        reverse_edges: reverse_edges.len(),
    }
}

fn canonical_array_type_group(index: &AdmittedCollisionReplayIndex) -> ReplayOwner {
    let root = index
        .root_slots
        .iter()
        .find(|root| root.name == "Array")
        .expect("authenticated Array root");
    root.ty
        .map(ReplayOwner::TypeGroup)
        .expect("authenticated Array type group")
}

fn require_compact_runtime_indexes(index: &AdmittedCollisionReplayIndex) {
    let _ = (
        &index.owner_to_scc,
        &index.scc_owner_ranges,
        &index.scc_owners,
        &index.reverse_scc_offsets,
        &index.reverse_scc_edges,
        &index.root_slot_lookup,
        &index.owner_site_ranges,
        &index.baseline_record_ranges,
    );
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedRootLookup {
    root_ordinal: u32,
    value_seeds: Vec<ReplayOwner>,
    type_seeds: Vec<ReplayOwner>,
    namespace_seeds: Vec<ReplayOwner>,
}

impl ExpectedRootLookup {
    fn seeds_mut(&mut self, slot: RootSlotKind) -> &mut Vec<ReplayOwner> {
        match slot {
            RootSlotKind::Value => &mut self.value_seeds,
            RootSlotKind::Type => &mut self.type_seeds,
            RootSlotKind::Namespace => &mut self.namespace_seeds,
        }
    }

    fn has_seed(&self) -> bool {
        !self.value_seeds.is_empty()
            || !self.type_seeds.is_empty()
            || !self.namespace_seeds.is_empty()
    }
}

fn derive_root_lookup_from_wire(
    index: &AdmittedCollisionReplayIndex,
) -> BTreeMap<String, ExpectedRootLookup> {
    let mut expected = index
        .root_slots
        .iter()
        .enumerate()
        .map(|(ordinal, root)| {
            (
                root.name.clone(),
                ExpectedRootLookup {
                    root_ordinal: u32::try_from(ordinal).expect("root ordinal fits u32"),
                    value_seeds: root.value.map(ReplayOwner::Value).into_iter().collect(),
                    type_seeds: root.ty.map(ReplayOwner::TypeGroup).into_iter().collect(),
                    namespace_seeds: root
                        .namespace
                        .map(ReplayOwner::Namespace)
                        .into_iter()
                        .collect(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(expected.len(), index.root_slots.len());

    for consumer in &index.root_slot_consumers {
        expected
            .get_mut(&consumer.name)
            .expect("consumer root exists in authenticated rows")
            .seeds_mut(consumer.slot)
            .push(consumer.consumer);
    }
    for lookup in expected.values_mut() {
        for seeds in [
            &mut lookup.value_seeds,
            &mut lookup.type_seeds,
            &mut lookup.namespace_seeds,
        ] {
            seeds.sort_unstable();
            seeds.dedup();
        }
    }
    expected
}

#[test]
fn admitted_root_lookup_exactly_projects_authenticated_roots_and_consumers() {
    let base = super::provider::shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base");
    let index = base.replay_index_for_test();
    let expected = derive_root_lookup_from_wire(index);

    assert_eq!(index.root_slot_lookup.len(), expected.len());
    assert_eq!(
        index
            .root_slot_lookup
            .keys()
            .map(|name| name.as_ref())
            .collect::<BTreeSet<_>>(),
        expected.keys().map(String::as_str).collect::<BTreeSet<_>>()
    );
    for (name, expected) in expected {
        let actual = index
            .root_slot_lookup
            .get(name.as_str())
            .expect("every authenticated root has a runtime lookup");
        assert_eq!(actual.root_ordinal, expected.root_ordinal, "{name}");
        assert_eq!(
            actual.value_seeds.as_slice(),
            expected.value_seeds,
            "{name}"
        );
        assert_eq!(actual.type_seeds.as_slice(), expected.type_seeds, "{name}");
        assert_eq!(
            actual.namespace_seeds.as_slice(),
            expected.namespace_seeds,
            "{name}"
        );
        for seeds in [
            actual.value_seeds.as_slice(),
            actual.type_seeds.as_slice(),
            actual.namespace_seeds.as_slice(),
        ] {
            assert!(
                seeds.windows(2).all(|pair| pair[0] < pair[1]),
                "{name} seeds must be sorted and deduplicated"
            );
        }
    }
}

#[test]
fn root_lookup_ordinals_recover_placeholder_roots_and_global_flags() {
    let base = super::provider::shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base");
    let index = base.replay_index_for_test();
    let expected = derive_root_lookup_from_wire(index);
    let mut placeholder_witnesses = BTreeSet::new();
    let mut global_contributor_witnesses = BTreeSet::new();
    let expected_flags = index
        .root_slots
        .iter()
        .map(|root| {
            (
                root.name.as_str(),
                (root.global_object_contributor, root.explicit_global_this),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut recovered_flags = BTreeMap::new();

    for (name, expected) in &expected {
        let lookup = &index.root_slot_lookup[name.as_str()];
        let root = &index.root_slots
            [usize::try_from(lookup.root_ordinal).expect("root ordinal fits usize")];
        assert_eq!(&root.name, name);
        assert_eq!(lookup.root_ordinal, expected.root_ordinal);

        if root.value.is_none()
            && root.ty.is_none()
            && root.namespace.is_none()
            && expected.has_seed()
        {
            placeholder_witnesses.insert(name.as_str());
        }
        if root.global_object_contributor {
            global_contributor_witnesses.insert(name.as_str());
        }
        recovered_flags.insert(
            name.as_str(),
            (root.global_object_contributor, root.explicit_global_this),
        );
    }

    assert_eq!(recovered_flags, expected_flags);
    assert!(
        !placeholder_witnesses.is_empty(),
        "consumer-only placeholder roots remain reachable through the lookup"
    );
    assert!(
        !global_contributor_witnesses.is_empty(),
        "global-object contributor flags remain recoverable through root ordinals"
    );
}

#[test]
fn sparse_graph_access_exposes_no_base_cardinality_for_bitmap_allocation() {
    let source = include_str!("../check/checker/replay_index.rs");
    let declaration = source
        .split_once("pub(crate) trait SparseReplayGraphAccess")
        .expect("production sparse graph access")
        .1
        .split_once("\n}")
        .expect("sparse graph access body")
        .0;
    for forbidden in [
        "owner_count",
        "scc_count",
        "edge_count",
        "base_len",
        "all_owners",
        "all_scc",
        "all_edges",
    ] {
        assert!(
            !declaration.contains(forbidden),
            "sparse scheduler access must not expose {forbidden}"
        );
    }
}

#[test]
fn sparse_scheduler_and_admitted_graph_implementation_are_not_test_only() {
    let source = include_str!("../check/checker/replay_index.rs");
    for declaration in [
        "pub(crate) trait SparseReplayGraphAccess",
        "pub(crate) fn schedule_sparse_collision_closure",
        "impl SparseReplayGraphAccess for AdmittedCollisionReplayIndex",
    ] {
        let offset = source.find(declaration).expect("production scheduler seam");
        let attributes = source[..offset]
            .lines()
            .rev()
            .take_while(|line| {
                let line = line.trim();
                line.starts_with("#[") || line.is_empty()
            })
            .collect::<Vec<_>>();
        assert!(
            attributes
                .iter()
                .all(|attribute| !attribute.contains("cfg(test)")),
            "{declaration} must be available outside cfg(test)"
        );
    }
}

#[test]
fn sparse_scheduler_touches_only_the_three_affected_sccs() {
    let (graph, seed, expected) = RecordingSparseGraph::synthetic();
    let actual = schedule_sparse_collision_closure(&graph, &[seed])
        .expect("the synthetic sparse closure is valid");

    assert_eq!(actual, expected);
    assert_eq!(
        graph.work.snapshot(),
        SchedulerWork {
            seed_probes: 1,
            owner_to_scc_probes: 1,
            queue_pushes: 3,
            queue_pops: 3,
            reverse_scc_slice_probes: 3,
            reverse_edge_probes: 2,
            affected_scc_inserts: 3,
            owner_slice_probes: 3,
            affected_owner_emissions: 4,
            full_owner_scans: 0,
            full_scc_scans: 0,
            full_edge_scans: 0,
            base_sized_allocations: 0,
        }
    );
    assert_eq!(
        graph.poison_sccs.len(),
        usize::try_from(POISON_SCCS).expect("poison SCC count fits usize")
    );
}

#[test]
fn canonical_array_type_group_seed_matches_the_independent_full_scan_oracle() {
    let base = super::provider::shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base");
    let index = base.replay_index_for_test();
    require_compact_runtime_indexes(index);
    // Root admission resolves the name once; the scheduler accepts only authenticated owners.
    let seed = canonical_array_type_group(index);

    let sparse = schedule_sparse_collision_closure(index, &[seed])
        .expect("canonical Array closure schedules");
    let oracle = full_scan_oracle(index, seed);

    assert_eq!(
        sparse.iter().copied().collect::<BTreeSet<_>>(),
        oracle.owners.iter().copied().collect::<BTreeSet<_>>()
    );
    assert_eq!(sparse.len(), 34);
    assert_eq!(oracle.sccs, 34);
    assert_eq!(oracle.reverse_edges, 42);
}
