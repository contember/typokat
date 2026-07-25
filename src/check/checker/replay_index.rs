//! Compiler-owned dependency evidence for selective library collision replay.

use super::classes::construction::dependency_first_sccs;
use super::events_library::LibraryEventKey;
use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::binder::namespace::NamespaceId;
use crate::check::query::PublishedClassLookup;
use crate::class_semantics::{DemandOutcome, PublishedClassSurface, PublishedClasses};
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader};
use crate::source::LibraryFileOrdinal;
use crate::span::Span;
use crate::types::repr::ClassId;
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

pub(crate) const COLLISION_REPLAY_MANIFEST_SHA256: [u8; 32] = [
    0xcc, 0x12, 0x5e, 0x22, 0xa5, 0x61, 0xb0, 0x69, 0xf6, 0x2f, 0x67, 0x07, 0xe5, 0xeb, 0x3f, 0x81,
    0x87, 0xbe, 0x09, 0x59, 0xbb, 0x75, 0xd8, 0xcb, 0xfb, 0x66, 0x52, 0x66, 0xd2, 0x1c, 0x2c, 0x95,
];
const COLLISION_REPLAY_MANIFEST_DOMAIN: &[u8] = b"typokat-collision-replay-index-v1";
const MAX_REPLAY_COLLECTION_ROWS: usize = 1_000_000;

/// Stable semantic publication domains. Tag order is part of the manifest wire contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ReplayOwner {
    TypeGroup(TypeGroupId),
    Value(ValueStorageId),
    Namespace(NamespaceId),
    Class(ClassId),
    GlobalObject,
    Statement(LibraryEventKey),
}

impl PartialOrd for ReplayOwner {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReplayOwner {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn payload(owner: ReplayOwner) -> (u64, u64, u64, u64) {
            match owner {
                ReplayOwner::TypeGroup(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Value(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Namespace(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Class(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::GlobalObject => (0, 0, 0, 0),
                ReplayOwner::Statement(key) => (
                    u64::try_from(key.file_ordinal.index()).unwrap_or(u64::MAX),
                    u64::from(key.source_start),
                    u64::try_from(key.event_ordinal).unwrap_or(u64::MAX),
                    u64::try_from(key.record_ordinal).unwrap_or(u64::MAX),
                ),
            }
        }
        self.tag()
            .cmp(&other.tag())
            .then_with(|| payload(*self).cmp(&payload(*other)))
    }
}

impl ReplayOwner {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::TypeGroup(_) => 0,
            Self::Value(_) => 1,
            Self::Namespace(_) => 2,
            Self::Class(_) => 3,
            Self::GlobalObject => 4,
            Self::Statement(_) => 5,
        }
    }

    fn encode(self, bytes: &mut ManifestBytes) -> Result<(), ReplayIndexGenerationError> {
        bytes.u8(self.tag());
        match self {
            Self::TypeGroup(id) => bytes.u32(id.0),
            Self::Value(id) => bytes.u32(id.0),
            Self::Namespace(id) => bytes.u32(id.0),
            Self::Class(id) => bytes.u32(id.0),
            Self::GlobalObject => {}
            Self::Statement(key) => {
                bytes.usize(key.file_ordinal.index())?;
                bytes.u32(key.source_start);
                bytes.usize(key.event_ordinal)?;
                bytes.usize(key.record_ordinal)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayOwnerSite {
    pub(crate) owner: ReplayOwner,
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayRootSlot {
    pub(crate) name: String,
    pub(crate) value: Option<ValueStorageId>,
    pub(crate) ty: Option<TypeGroupId>,
    pub(crate) namespace: Option<NamespaceId>,
    pub(crate) global_object_contributor: bool,
    pub(crate) explicit_global_this: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReplayReverseEdge {
    pub(crate) dependency: ReplayOwner,
    pub(crate) consumer: ReplayOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RootSlotKind {
    Value,
    Type,
    Namespace,
}

impl RootSlotKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Value => 0,
            Self::Type => 1,
            Self::Namespace => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ReplayDependencyKey {
    Owner(ReplayOwner),
    RootSlot { name: String, slot: RootSlotKind },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReplayRootConsumer {
    pub(crate) name: String,
    pub(crate) slot: RootSlotKind,
    pub(crate) consumer: ReplayOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayScc {
    pub(crate) replay_ordinal: u32,
    pub(crate) owners: SmallVec<[ReplayOwner; 1]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayBaselineRecord {
    pub(crate) owner: ReplayOwner,
    pub(crate) record_count: u64,
    pub(crate) digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollisionReplayIndex {
    pub(crate) schema: u32,
    pub(crate) owner_partition: Vec<ReplayOwner>,
    pub(crate) root_slots: Vec<ReplayRootSlot>,
    pub(crate) owner_sites: Vec<ReplayOwnerSite>,
    pub(crate) reverse_edges: Vec<ReplayReverseEdge>,
    pub(crate) root_slot_consumers: Vec<ReplayRootConsumer>,
    pub(crate) scc_membership: Vec<ReplayScc>,
    pub(crate) statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    pub(crate) baseline_records: Vec<ReplayBaselineRecord>,
    pub(crate) unowned_demand_count: u64,
    pub(crate) invalid_owner_site_count: u64,
    pub(crate) noncanonical_edge_count: u64,
    pub(crate) typed_reference_coverage_misses: u64,
    pub(crate) canonical_manifest_bytes: Vec<u8>,
    pub(crate) canonical_manifest_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdmittedCollisionReplayIndex {
    pub(crate) schema: u32,
    pub(crate) owner_partition: Vec<ReplayOwner>,
    pub(crate) root_slots: Vec<ReplayRootSlot>,
    pub(crate) owner_sites: Vec<ReplayOwnerSite>,
    pub(crate) reverse_edges: Vec<ReplayReverseEdge>,
    pub(crate) root_slot_consumers: Vec<ReplayRootConsumer>,
    pub(crate) scc_membership: Vec<ReplayScc>,
    pub(crate) statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    pub(crate) baseline_records: Vec<ReplayBaselineRecord>,
    pub(crate) unowned_demand_count: u64,
    pub(crate) invalid_owner_site_count: u64,
    pub(crate) noncanonical_edge_count: u64,
    pub(crate) typed_reference_coverage_misses: u64,
    pub(crate) owner_to_scc: Vec<u32>,
    pub(crate) scc_owner_ranges: Vec<ReplayRowRange>,
    pub(crate) scc_owners: Vec<ReplayOwner>,
    pub(crate) reverse_scc_offsets: Vec<u32>,
    pub(crate) reverse_scc_edges: Vec<u32>,
    pub(crate) root_slot_lookup: rustc_hash::FxHashMap<Box<str>, ReplayRootLookup>,
    pub(crate) owner_site_ranges: Vec<ReplayRowRange>,
    pub(crate) baseline_record_ranges: Vec<ReplayRowRange>,
    pub(crate) canonical_manifest_len: usize,
    pub(crate) canonical_manifest_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplayRowRange {
    start: u32,
    end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayRootLookup {
    pub(crate) root_ordinal: u32,
    pub(crate) value_seeds: SmallVec<[ReplayOwner; 1]>,
    pub(crate) type_seeds: SmallVec<[ReplayOwner; 1]>,
    pub(crate) namespace_seeds: SmallVec<[ReplayOwner; 1]>,
}

impl ReplayRootLookup {
    fn seeds_mut(&mut self, slot: RootSlotKind) -> &mut SmallVec<[ReplayOwner; 1]> {
        match slot {
            RootSlotKind::Value => &mut self.value_seeds,
            RootSlotKind::Type => &mut self.type_seeds,
            RootSlotKind::Namespace => &mut self.namespace_seeds,
        }
    }
}

impl ReplayRowRange {
    fn checked(start: usize, end: usize) -> Result<Self, ReplayIndexAdmissionError> {
        Ok(Self {
            start: u32::try_from(start).map_err(|_| ReplayIndexAdmissionError::InvalidEncoding)?,
            end: u32::try_from(end).map_err(|_| ReplayIndexAdmissionError::InvalidEncoding)?,
        })
    }

    #[allow(dead_code)] // Used when the ADR-0015 collision route consumes admitted ranges.
    fn indices(self) -> Option<std::ops::Range<usize>> {
        Some(usize::try_from(self.start).ok()?..usize::try_from(self.end).ok()?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub(crate) enum SparseReplayScheduleError {
    UnknownSeed(ReplayOwner),
    MissingScc(u32),
}

#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub(crate) trait SparseReplayGraphAccess {
    fn owner_scc(&self, owner: ReplayOwner) -> Option<u32>;
    fn reverse_sccs(&self, scc: u32) -> Option<&[u32]>;
    fn scc_owners(&self, scc: u32) -> Option<&[ReplayOwner]>;

    fn observe_seed_probe(&self, _owner: ReplayOwner) {}
    fn observe_queue_push(&self, _scc: u32) {}
    fn observe_queue_pop(&self, _scc: u32) {}
    fn observe_reverse_edge_probe(&self, _dependency: u32, _consumer: u32) {}
    fn observe_affected_scc_insert(&self, _scc: u32) {}
    fn observe_affected_owner_emission(&self, _owner: ReplayOwner) {}
}

#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub(crate) fn schedule_sparse_collision_closure<G: SparseReplayGraphAccess + ?Sized>(
    graph: &G,
    seeds: &[ReplayOwner],
) -> Result<Vec<ReplayOwner>, SparseReplayScheduleError> {
    let mut affected_sccs = rustc_hash::FxHashSet::default();
    let mut pending = Vec::new();
    for seed in seeds {
        graph.observe_seed_probe(*seed);
        let scc = graph
            .owner_scc(*seed)
            .ok_or(SparseReplayScheduleError::UnknownSeed(*seed))?;
        if affected_sccs.insert(scc) {
            graph.observe_affected_scc_insert(scc);
            graph.observe_queue_push(scc);
            pending.push(scc);
        }
    }

    let mut pending_cursor = 0;
    while let Some(dependency) = pending.get(pending_cursor).copied() {
        pending_cursor += 1;
        graph.observe_queue_pop(dependency);
        let consumers = graph
            .reverse_sccs(dependency)
            .ok_or(SparseReplayScheduleError::MissingScc(dependency))?;
        for consumer in consumers {
            graph.observe_reverse_edge_probe(dependency, *consumer);
            if affected_sccs.insert(*consumer) {
                graph.observe_affected_scc_insert(*consumer);
                graph.observe_queue_push(*consumer);
                pending.push(*consumer);
            }
        }
    }

    let mut affected_sccs = affected_sccs.into_iter().collect::<Vec<_>>();
    affected_sccs.sort_unstable();
    let mut affected_owners = Vec::new();
    for scc in affected_sccs {
        let owners = graph
            .scc_owners(scc)
            .ok_or(SparseReplayScheduleError::MissingScc(scc))?;
        for owner in owners {
            graph.observe_affected_owner_emission(*owner);
            affected_owners.push(*owner);
        }
    }
    Ok(affected_owners)
}

impl SparseReplayGraphAccess for AdmittedCollisionReplayIndex {
    fn owner_scc(&self, owner: ReplayOwner) -> Option<u32> {
        let ordinal = self.owner_partition.binary_search(&owner).ok()?;
        self.owner_to_scc.get(ordinal).copied()
    }

    fn reverse_sccs(&self, scc: u32) -> Option<&[u32]> {
        let scc = usize::try_from(scc).ok()?;
        let start = usize::try_from(*self.reverse_scc_offsets.get(scc)?).ok()?;
        let end = usize::try_from(*self.reverse_scc_offsets.get(scc.checked_add(1)?)?).ok()?;
        self.reverse_scc_edges.get(start..end)
    }

    fn scc_owners(&self, scc: u32) -> Option<&[ReplayOwner]> {
        let range = *self.scc_owner_ranges.get(usize::try_from(scc).ok()?)?;
        self.scc_owners.get(range.indices()?)
    }
}

impl AdmittedCollisionReplayIndex {
    #[cfg(test)]
    pub(crate) const fn canonical_manifest_len(&self) -> usize {
        self.canonical_manifest_len
    }

    #[cfg(test)]
    pub(crate) fn encode_manifest_for_test(&self) -> Result<Vec<u8>, ReplayIndexGenerationError> {
        CollisionReplayIndex {
            schema: self.schema,
            owner_partition: self.owner_partition.clone(),
            root_slots: self.root_slots.clone(),
            owner_sites: self.owner_sites.clone(),
            reverse_edges: self.reverse_edges.clone(),
            root_slot_consumers: self.root_slot_consumers.clone(),
            scc_membership: self.scc_membership.clone(),
            statement_owners: self.statement_owners.clone(),
            baseline_records: self.baseline_records.clone(),
            unowned_demand_count: self.unowned_demand_count,
            invalid_owner_site_count: self.invalid_owner_site_count,
            noncanonical_edge_count: self.noncanonical_edge_count,
            typed_reference_coverage_misses: self.typed_reference_coverage_misses,
            canonical_manifest_bytes: Vec::new(),
            canonical_manifest_sha256: self.canonical_manifest_sha256,
        }
        .encode()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplayIndexAdmissionError {
    InvalidEncoding,
    InvalidOwnerPartition,
    InvalidRootIndex,
    InvalidDependencyGraph,
    InvalidOwnerSites,
    InvalidSccPartition,
    InvalidStatementPartition,
    InvalidBaselinePartition,
    NonzeroGenerationHealthCounter,
    ManifestIdentityMismatch,
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayIndexAdmissionLimits<'roots> {
    pub(crate) type_groups: usize,
    pub(crate) value_storages: usize,
    pub(crate) namespaces: usize,
    pub(crate) classes: usize,
    pub(crate) source_files: usize,
    pub(crate) roots: &'roots [ReplayRootIdentity],
}

pub(crate) type ReplayRootIdentity = (
    String,
    Option<ValueStorageId>,
    Option<TypeGroupId>,
    Option<NamespaceId>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReplayIndexGenerationError {
    UnownedDemands {
        count: u64,
        samples: Vec<ReplayUnownedDemandSample>,
    },
    InvalidOwnerSites(u64),
    NoncanonicalEdges(u64),
    TypedReferenceCoverage {
        count: u64,
        samples: Vec<ReplayCoverageMiss>,
    },
    MissingOwnerSite(ReplayOwner),
    UnknownOwner(ReplayOwner),
    DuplicateOwner(ReplayOwner),
    DuplicateOwnerSite(ReplayOwner),
    DuplicateRootName(String),
    InvalidRootSlot(String),
    DuplicateStatement(LibraryEventKey),
    StatementPartitionMismatch,
    DuplicateBaseline(ReplayOwner),
    BaselinePartitionMismatch,
    ActiveOwnerScopes,
    SharedTraceAtFinalization,
    IntegerOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReplayUnownedDemandSample {
    pub(crate) dependency: ReplayDependencyKey,
    pub(crate) boundary: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReplayCoverageMiss {
    Boundary(&'static str),
    Dependency {
        consumer: ReplayOwner,
        dependency: ReplayOwner,
    },
}

#[derive(Default)]
struct ReplayTraceState {
    active_owners: Vec<ReplayOwner>,
    active_typed_observations: Vec<bool>,
    ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>,
    consumer_dependencies: BTreeMap<ReplayOwner, BTreeSet<ReplayDependencyKey>>,
    unowned_demand_count: u64,
    unowned_demand_samples: BTreeSet<ReplayUnownedDemandSample>,
    typed_reference_coverage_misses: u64,
    typed_reference_coverage_samples: BTreeSet<ReplayCoverageMiss>,
}

/// Pass-local, source-generation-only trace. Clones share one compiler-owned collector.
#[derive(Clone, Default)]
pub(crate) struct ReplayDependencyTrace {
    state: Rc<RefCell<ReplayTraceState>>,
}

impl ReplayDependencyTrace {
    pub(crate) fn scope(&self, owner: ReplayOwner) -> ReplayOwnerScope {
        self.enter(owner);
        ReplayOwnerScope {
            trace: self.clone(),
            owner,
        }
    }
    pub(crate) fn new(ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>) -> Self {
        Self {
            state: Rc::new(RefCell::new(ReplayTraceState {
                ticket_owners,
                ..ReplayTraceState::default()
            })),
        }
    }

    pub(crate) fn statement_owner(&self, ticket: (usize, usize)) -> Option<ReplayOwner> {
        self.state
            .borrow()
            .ticket_owners
            .get(&ticket)
            .copied()
            .map(ReplayOwner::Statement)
    }

    pub(crate) fn current_owner(&self) -> Option<ReplayOwner> {
        self.state.borrow().active_owners.last().copied()
    }

    pub(crate) fn enter(&self, owner: ReplayOwner) {
        self.state.borrow_mut().active_owners.push(owner);
    }

    pub(crate) fn leave(&self, expected: ReplayOwner) {
        let actual = self.state.borrow_mut().active_owners.pop();
        assert_eq!(
            actual,
            Some(expected),
            "replay owner scopes restore in LIFO order"
        );
    }

    pub(crate) fn demand(&self, dependency: ReplayOwner) {
        self.demand_at(dependency, "owner-demand");
    }

    pub(crate) fn demand_at(&self, dependency: ReplayOwner, boundary: &'static str) {
        self.demand_key(ReplayDependencyKey::Owner(dependency), boundary);
    }

    pub(crate) fn demand_root_slot(&self, name: &str, slot: RootSlotKind) {
        self.demand_key(
            ReplayDependencyKey::RootSlot {
                name: name.to_owned(),
                slot,
            },
            "root-slot",
        );
    }

    pub(crate) fn observe_typed_demand(
        &self,
        boundary: &'static str,
    ) -> ReplayTypedDemandObservation {
        self.state
            .borrow_mut()
            .active_typed_observations
            .push(false);
        ReplayTypedDemandObservation {
            trace: self.clone(),
            boundary,
        }
    }

    pub(crate) fn record_statement_dependency(
        &self,
        ticket: (usize, usize),
        producer: ReplayOwner,
    ) {
        let mut state = self.state.borrow_mut();
        let Some(key) = state.ticket_owners.get(&ticket).copied() else {
            state.unowned_demand_count = state.unowned_demand_count.saturating_add(1);
            return;
        };
        let consumer = ReplayOwner::Statement(key);
        if consumer == producer {
            return;
        }
        state
            .consumer_dependencies
            .entry(consumer)
            .or_default()
            .insert(ReplayDependencyKey::Owner(producer));
    }

    pub(crate) fn require_dependency(
        &self,
        consumer: ReplayOwner,
        dependency: ReplayOwner,
    ) -> bool {
        if consumer == dependency {
            return true;
        }
        let mut state = self.state.borrow_mut();
        let mut pending = vec![consumer];
        let mut visited = BTreeSet::new();
        let mut present = false;
        while let Some(owner) = pending.pop() {
            if !visited.insert(owner) {
                continue;
            }
            for target in state
                .consumer_dependencies
                .get(&owner)
                .into_iter()
                .flatten()
            {
                let ReplayDependencyKey::Owner(target) = target else {
                    continue;
                };
                if *target == dependency {
                    present = true;
                    break;
                }
                pending.push(*target);
            }
            if present {
                break;
            }
        }
        if !present {
            state.typed_reference_coverage_misses =
                state.typed_reference_coverage_misses.saturating_add(1);
            if state.typed_reference_coverage_samples.len() < 16 {
                state
                    .typed_reference_coverage_samples
                    .insert(ReplayCoverageMiss::Dependency {
                        consumer,
                        dependency,
                    });
            }
        }
        present
    }

    fn demand_key(&self, dependency: ReplayDependencyKey, boundary: &'static str) {
        let mut state = self.state.borrow_mut();
        for covered in &mut state.active_typed_observations {
            *covered = true;
        }
        let Some(consumer) = state.active_owners.last().copied() else {
            state.unowned_demand_count = state.unowned_demand_count.saturating_add(1);
            if state.unowned_demand_samples.len() < 16 {
                state
                    .unowned_demand_samples
                    .insert(ReplayUnownedDemandSample {
                        dependency,
                        boundary,
                    });
            }
            return;
        };
        if dependency == ReplayDependencyKey::Owner(consumer) {
            return;
        }
        state
            .consumer_dependencies
            .entry(consumer)
            .or_default()
            .insert(dependency);
    }

    fn finish_typed_observation(&self, boundary: &'static str) {
        let mut state = self.state.borrow_mut();
        let covered = state
            .active_typed_observations
            .pop()
            .expect("typed replay observations restore in LIFO order");
        if !covered {
            state.typed_reference_coverage_misses =
                state.typed_reference_coverage_misses.saturating_add(1);
            if state.typed_reference_coverage_samples.len() < 16 {
                state
                    .typed_reference_coverage_samples
                    .insert(ReplayCoverageMiss::Boundary(boundary));
            }
        }
    }

    #[cfg(test)]
    fn counters(&self) -> (u64, u64) {
        let state = self.state.borrow();
        (state.unowned_demand_count, 0)
    }

    pub(crate) fn finish(
        self,
        owner_partition: Vec<ReplayOwner>,
        mut root_slots: Vec<ReplayRootSlot>,
        owner_sites: Vec<ReplayOwnerSite>,
        statement_keys: Vec<LibraryEventKey>,
        baseline_records: Vec<ReplayBaselineRecord>,
        invalid_owner_site_count: u64,
    ) -> Result<CollisionReplayIndex, ReplayIndexGenerationError> {
        let state = Rc::try_unwrap(self.state)
            .map_err(|_| ReplayIndexGenerationError::SharedTraceAtFinalization)?
            .into_inner();
        if !state.active_owners.is_empty() {
            return Err(ReplayIndexGenerationError::ActiveOwnerScopes);
        }
        if state.unowned_demand_count != 0 {
            return Err(ReplayIndexGenerationError::UnownedDemands {
                count: state.unowned_demand_count,
                samples: state.unowned_demand_samples.into_iter().collect(),
            });
        }
        if !state.active_typed_observations.is_empty() {
            return Err(ReplayIndexGenerationError::ActiveOwnerScopes);
        }
        if state.typed_reference_coverage_misses != 0 {
            return Err(ReplayIndexGenerationError::TypedReferenceCoverage {
                count: state.typed_reference_coverage_misses,
                samples: state.typed_reference_coverage_samples.into_iter().collect(),
            });
        }

        let mut owner_partition = owner_partition;
        owner_partition.sort_unstable();
        if let Some(duplicate) = owner_partition
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ReplayIndexGenerationError::DuplicateOwner(duplicate));
        }
        let partition = owner_partition.iter().copied().collect::<BTreeSet<_>>();

        let mut owner_sites = owner_sites;
        owner_sites.sort_by_key(|site| {
            (
                site.owner,
                site.file_ordinal,
                site.span.start,
                site.span.end,
            )
        });
        let invalid_sites = owner_sites
            .iter()
            .filter(|site| !partition.contains(&site.owner) || site.span.start > site.span.end)
            .count();
        let invalid_sites = invalid_owner_site_count.saturating_add(
            u64::try_from(invalid_sites)
                .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
        );
        if invalid_sites != 0 {
            return Err(ReplayIndexGenerationError::InvalidOwnerSites(invalid_sites));
        }
        if let Some(duplicate) = owner_sites
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0].owner)
        {
            return Err(ReplayIndexGenerationError::DuplicateOwnerSite(duplicate));
        }
        let site_owners = owner_sites
            .iter()
            .map(|site| site.owner)
            .collect::<BTreeSet<_>>();
        for owner in &owner_partition {
            if !site_owners.contains(owner) {
                return Err(ReplayIndexGenerationError::MissingOwnerSite(*owner));
            }
        }

        let mut statement_keys = statement_keys;
        statement_keys.sort_unstable();
        if let Some(duplicate) = statement_keys
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ReplayIndexGenerationError::DuplicateStatement(duplicate));
        }
        let statement_partition = owner_partition
            .iter()
            .filter_map(|owner| match owner {
                ReplayOwner::Statement(key) => Some(*key),
                _ => None,
            })
            .collect::<Vec<_>>();
        if statement_partition != statement_keys {
            return Err(ReplayIndexGenerationError::StatementPartitionMismatch);
        }

        let mut baseline_records = baseline_records;
        baseline_records.sort_by_key(|record| record.owner);
        if let Some(duplicate) = baseline_records
            .windows(2)
            .find(|pair| pair[0].owner == pair[1].owner)
            .map(|pair| pair[0].owner)
        {
            return Err(ReplayIndexGenerationError::DuplicateBaseline(duplicate));
        }
        if baseline_records
            .iter()
            .map(|record| record.owner)
            .ne(owner_partition.iter().copied())
        {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }

        let mut reverse_edges = Vec::new();
        let mut root_slot_consumers = Vec::new();
        let mut graph = owner_partition
            .iter()
            .copied()
            .map(|owner| (owner, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (consumer, dependencies) in state.consumer_dependencies {
            if !partition.contains(&consumer) {
                return Err(ReplayIndexGenerationError::UnknownOwner(consumer));
            }
            for dependency in dependencies {
                match dependency {
                    ReplayDependencyKey::Owner(dependency) => {
                        if !partition.contains(&dependency) {
                            return Err(ReplayIndexGenerationError::UnknownOwner(dependency));
                        }
                        graph.entry(consumer).or_default().insert(dependency);
                        reverse_edges.push(ReplayReverseEdge {
                            dependency,
                            consumer,
                        });
                    }
                    ReplayDependencyKey::RootSlot { name, slot } => {
                        root_slot_consumers.push(ReplayRootConsumer {
                            name,
                            slot,
                            consumer,
                        });
                    }
                }
            }
        }
        reverse_edges.sort_unstable();
        root_slot_consumers.sort_unstable();
        let published_root_names = root_slots
            .iter()
            .map(|root| root.name.clone())
            .collect::<BTreeSet<_>>();
        for name in root_slot_consumers
            .iter()
            .map(|dependency| dependency.name.clone())
            .collect::<BTreeSet<_>>()
        {
            if !published_root_names.contains(&name) {
                root_slots.push(ReplayRootSlot {
                    name,
                    value: None,
                    ty: None,
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                });
            }
        }
        root_slots.sort_by(|left, right| left.name.cmp(&right.name));
        if let Some(duplicate) = root_slots
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.clone())
        {
            return Err(ReplayIndexGenerationError::DuplicateRootName(duplicate));
        }
        for root in &root_slots {
            let valid = !root.name.is_empty()
                && root
                    .value
                    .is_none_or(|id| partition.contains(&ReplayOwner::Value(id)))
                && root
                    .ty
                    .is_none_or(|id| partition.contains(&ReplayOwner::TypeGroup(id)))
                && root
                    .namespace
                    .is_none_or(|id| partition.contains(&ReplayOwner::Namespace(id)));
            if !valid {
                return Err(ReplayIndexGenerationError::InvalidRootSlot(
                    root.name.clone(),
                ));
            }
        }

        let noncanonical_edge_count =
            validate_edge_rows(&partition, &reverse_edges, &root_slot_consumers);
        if noncanonical_edge_count != 0 {
            return Err(ReplayIndexGenerationError::NoncanonicalEdges(
                noncanonical_edge_count,
            ));
        }

        let scc_membership = dependency_first_sccs(&graph)
            .into_iter()
            .enumerate()
            .map(|(index, owners)| {
                Ok(ReplayScc {
                    replay_ordinal: u32::try_from(index)
                        .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
                    owners: owners.into_iter().collect(),
                })
            })
            .collect::<Result<Vec<_>, ReplayIndexGenerationError>>()?;
        let statement_owners = statement_keys
            .into_iter()
            .map(|key| (key, ReplayOwner::Statement(key)))
            .collect::<Vec<_>>();
        let mut index = CollisionReplayIndex {
            schema: 1,
            owner_partition,
            root_slots,
            owner_sites,
            reverse_edges,
            root_slot_consumers,
            scc_membership,
            statement_owners,
            baseline_records,
            unowned_demand_count: 0,
            invalid_owner_site_count: 0,
            noncanonical_edge_count,
            typed_reference_coverage_misses: 0,
            canonical_manifest_bytes: Vec::new(),
            canonical_manifest_sha256: [0; 32],
        };
        index.canonical_manifest_bytes = index.encode()?;
        index.canonical_manifest_sha256 = Sha256::digest(&index.canonical_manifest_bytes).into();
        Ok(index)
    }
}

fn validate_edge_rows(
    partition: &BTreeSet<ReplayOwner>,
    edges: &[ReplayReverseEdge],
    root_consumers: &[ReplayRootConsumer],
) -> u64 {
    let malformed_edges = edges
        .iter()
        .enumerate()
        .filter(|(index, edge)| {
            edge.dependency == edge.consumer
                || !partition.contains(&edge.dependency)
                || !partition.contains(&edge.consumer)
                || index
                    .checked_sub(1)
                    .is_some_and(|previous| edges[previous] >= **edge)
        })
        .count();
    let malformed_roots = root_consumers
        .iter()
        .enumerate()
        .filter(|(index, dependency)| {
            dependency.name.is_empty()
                || !partition.contains(&dependency.consumer)
                || index
                    .checked_sub(1)
                    .is_some_and(|previous| root_consumers[previous] >= **dependency)
        })
        .count();
    u64::try_from(malformed_edges.saturating_add(malformed_roots)).unwrap_or(u64::MAX)
}

pub(crate) struct ReplayClassLookup<'a> {
    published: &'a PublishedClasses,
    trace: Option<ReplayDependencyTrace>,
}

impl<'a> ReplayClassLookup<'a> {
    pub(crate) fn new(
        published: &'a PublishedClasses,
        trace: Option<ReplayDependencyTrace>,
    ) -> Self {
        Self { published, trace }
    }
}

impl PublishedClassLookup for ReplayClassLookup<'_> {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        self.observe_class_demand(class);
        self.published.published_class(class)
    }

    fn publication_identity(&self) -> &Arc<()> {
        self.published.identity()
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        self.observe_class_demand(class);
        self.published.require(class)
    }

    fn observe_class_demand(&self, class: ClassId) {
        if let Some(trace) = &self.trace {
            let _observation = trace.observe_typed_demand("semantic-query-class-terminal");
            trace.demand_at(ReplayOwner::Class(class), "semantic-query-class-terminal");
        }
    }

    fn class_demand_observation_enabled(&self) -> bool {
        self.trace.is_some()
    }
}

pub(crate) struct ReplayOwnerScope {
    trace: ReplayDependencyTrace,
    owner: ReplayOwner,
}

pub(crate) struct ReplayTypedDemandObservation {
    trace: ReplayDependencyTrace,
    boundary: &'static str,
}

impl Drop for ReplayTypedDemandObservation {
    fn drop(&mut self) {
        self.trace.finish_typed_observation(self.boundary);
    }
}

impl Drop for ReplayOwnerScope {
    fn drop(&mut self) {
        self.trace.leave(self.owner);
    }
}

impl CollisionReplayIndex {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, ReplayIndexGenerationError> {
        let mut bytes = ManifestBytes::new(COLLISION_REPLAY_MANIFEST_DOMAIN);
        bytes.u32(self.schema);
        bytes.usize(self.owner_partition.len())?;
        for owner in &self.owner_partition {
            owner.encode(&mut bytes)?;
        }
        bytes.usize(self.root_slots.len())?;
        for root in &self.root_slots {
            bytes.string(&root.name)?;
            bytes.optional_u32(root.value.map(|id| id.0));
            bytes.optional_u32(root.ty.map(|id| id.0));
            bytes.optional_u32(root.namespace.map(|id| id.0));
            bytes.u8(u8::from(root.global_object_contributor));
            bytes.u8(u8::from(root.explicit_global_this));
        }
        bytes.usize(self.owner_sites.len())?;
        for site in &self.owner_sites {
            site.owner.encode(&mut bytes)?;
            bytes.usize(site.file_ordinal.index())?;
            bytes.u32(site.span.start);
            bytes.u32(site.span.end);
        }
        bytes.usize(self.reverse_edges.len())?;
        for edge in &self.reverse_edges {
            edge.dependency.encode(&mut bytes)?;
            edge.consumer.encode(&mut bytes)?;
        }
        bytes.usize(self.root_slot_consumers.len())?;
        for dependency in &self.root_slot_consumers {
            bytes.string(&dependency.name)?;
            bytes.u8(dependency.slot.tag());
            dependency.consumer.encode(&mut bytes)?;
        }
        bytes.usize(self.scc_membership.len())?;
        for component in &self.scc_membership {
            bytes.u32(component.replay_ordinal);
            bytes.usize(component.owners.len())?;
            for owner in &component.owners {
                owner.encode(&mut bytes)?;
            }
        }
        bytes.usize(self.statement_owners.len())?;
        for (key, owner) in &self.statement_owners {
            ReplayOwner::Statement(*key).encode(&mut bytes)?;
            owner.encode(&mut bytes)?;
        }
        bytes.usize(self.baseline_records.len())?;
        for baseline in &self.baseline_records {
            baseline.owner.encode(&mut bytes)?;
            bytes.u64(baseline.record_count);
            bytes.raw(&baseline.digest);
        }
        bytes.u64(self.unowned_demand_count);
        bytes.u64(self.invalid_owner_site_count);
        bytes.u64(self.noncanonical_edge_count);
        bytes.u64(self.typed_reference_coverage_misses);
        Ok(bytes.finish())
    }
}

fn invalid_encoding(_: SnapshotCodecError) -> ReplayIndexAdmissionError {
    ReplayIndexAdmissionError::InvalidEncoding
}

fn bounded_collection_len(
    reader: &mut SnapshotReader<'_>,
    minimum_item_bytes: usize,
) -> Result<usize, ReplayIndexAdmissionError> {
    let count = reader
        .collection_len(minimum_item_bytes)
        .map_err(invalid_encoding)?;
    if count > MAX_REPLAY_COLLECTION_ROWS {
        return Err(ReplayIndexAdmissionError::InvalidEncoding);
    }
    Ok(count)
}

fn read_owner(reader: &mut SnapshotReader<'_>) -> Result<ReplayOwner, ReplayIndexAdmissionError> {
    Ok(match reader.u8().map_err(invalid_encoding)? {
        0 => ReplayOwner::TypeGroup(TypeGroupId(reader.u32().map_err(invalid_encoding)?)),
        1 => ReplayOwner::Value(ValueStorageId(reader.u32().map_err(invalid_encoding)?)),
        2 => ReplayOwner::Namespace(NamespaceId(reader.u32().map_err(invalid_encoding)?)),
        3 => ReplayOwner::Class(ClassId(reader.u32().map_err(invalid_encoding)?)),
        4 => ReplayOwner::GlobalObject,
        5 => ReplayOwner::Statement(LibraryEventKey {
            file_ordinal: LibraryFileOrdinal::new(reader.usize().map_err(invalid_encoding)?),
            source_start: reader.u32().map_err(invalid_encoding)?,
            event_ordinal: reader.usize().map_err(invalid_encoding)?,
            record_ordinal: reader.usize().map_err(invalid_encoding)?,
        }),
        _ => return Err(ReplayIndexAdmissionError::InvalidEncoding),
    })
}

fn read_optional_id(
    reader: &mut SnapshotReader<'_>,
) -> Result<Option<u32>, ReplayIndexAdmissionError> {
    match reader.u8().map_err(invalid_encoding)? {
        0 => Ok(None),
        1 => reader.u32().map(Some).map_err(invalid_encoding),
        _ => Err(ReplayIndexAdmissionError::InvalidEncoding),
    }
}

fn owner_in_bounds(owner: ReplayOwner, limits: ReplayIndexAdmissionLimits<'_>) -> bool {
    match owner {
        ReplayOwner::TypeGroup(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.type_groups),
        ReplayOwner::Value(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.value_storages),
        ReplayOwner::Namespace(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.namespaces),
        ReplayOwner::Class(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.classes),
        ReplayOwner::GlobalObject => true,
        ReplayOwner::Statement(key) => key.file_ordinal.index() < limits.source_files,
    }
}

pub(crate) struct DecodedCollisionReplayIndex {
    index: CollisionReplayIndex,
    canonical_manifest_len: usize,
}

fn decode_collision_replay_index_with_sha256(
    bytes: &[u8],
    canonical_manifest_sha256: [u8; 32],
) -> Result<DecodedCollisionReplayIndex, ReplayIndexAdmissionError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .raw(COLLISION_REPLAY_MANIFEST_DOMAIN.len())
        .map_err(invalid_encoding)?
        != COLLISION_REPLAY_MANIFEST_DOMAIN
        || reader.u32().map_err(invalid_encoding)? != 1
    {
        return Err(ReplayIndexAdmissionError::InvalidEncoding);
    }
    let schema = 1;

    let owner_count = bounded_collection_len(&mut reader, 1)?;
    let mut owner_partition = Vec::with_capacity(owner_count);
    for _ in 0..owner_count {
        owner_partition.push(read_owner(&mut reader)?);
    }

    let root_count = bounded_collection_len(&mut reader, 13)?;
    let mut root_slots = Vec::with_capacity(root_count);
    for _ in 0..root_count {
        root_slots.push(ReplayRootSlot {
            name: reader.string().map_err(invalid_encoding)?.to_owned(),
            value: read_optional_id(&mut reader)?.map(ValueStorageId),
            ty: read_optional_id(&mut reader)?.map(TypeGroupId),
            namespace: read_optional_id(&mut reader)?.map(NamespaceId),
            global_object_contributor: reader.bool().map_err(invalid_encoding)?,
            explicit_global_this: reader.bool().map_err(invalid_encoding)?,
        });
    }

    let site_count = bounded_collection_len(&mut reader, 17)?;
    let mut owner_sites = Vec::with_capacity(site_count);
    for _ in 0..site_count {
        owner_sites.push(ReplayOwnerSite {
            owner: read_owner(&mut reader)?,
            file_ordinal: LibraryFileOrdinal::new(reader.usize().map_err(invalid_encoding)?),
            span: Span::new(
                reader.u32().map_err(invalid_encoding)?,
                reader.u32().map_err(invalid_encoding)?,
            ),
        });
    }

    let edge_count = bounded_collection_len(&mut reader, 2)?;
    let mut reverse_edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        reverse_edges.push(ReplayReverseEdge {
            dependency: read_owner(&mut reader)?,
            consumer: read_owner(&mut reader)?,
        });
    }

    let root_consumer_count = bounded_collection_len(&mut reader, 10)?;
    let mut root_slot_consumers = Vec::with_capacity(root_consumer_count);
    for _ in 0..root_consumer_count {
        let name = reader.string().map_err(invalid_encoding)?.to_owned();
        let slot = match reader.u8().map_err(invalid_encoding)? {
            0 => RootSlotKind::Value,
            1 => RootSlotKind::Type,
            2 => RootSlotKind::Namespace,
            _ => return Err(ReplayIndexAdmissionError::InvalidDependencyGraph),
        };
        root_slot_consumers.push(ReplayRootConsumer {
            name,
            slot,
            consumer: read_owner(&mut reader)?,
        });
    }

    let scc_count = bounded_collection_len(&mut reader, 12)?;
    let mut scc_membership = Vec::with_capacity(scc_count);
    for _ in 0..scc_count {
        let replay_ordinal = reader.u32().map_err(invalid_encoding)?;
        let count = bounded_collection_len(&mut reader, 1)?;
        let mut owners = SmallVec::<[ReplayOwner; 1]>::with_capacity(count);
        for _ in 0..count {
            owners.push(read_owner(&mut reader)?);
        }
        scc_membership.push(ReplayScc {
            replay_ordinal,
            owners,
        });
    }

    let statement_count = bounded_collection_len(&mut reader, 2)?;
    let mut statement_owners = Vec::with_capacity(statement_count);
    for _ in 0..statement_count {
        let key = match read_owner(&mut reader)? {
            ReplayOwner::Statement(key) => key,
            _ => return Err(ReplayIndexAdmissionError::InvalidEncoding),
        };
        statement_owners.push((key, read_owner(&mut reader)?));
    }

    let baseline_count = bounded_collection_len(&mut reader, 41)?;
    let mut baseline_records = Vec::with_capacity(baseline_count);
    for _ in 0..baseline_count {
        let owner = read_owner(&mut reader)?;
        let record_count = reader.u64().map_err(invalid_encoding)?;
        let mut digest = [0; 32];
        digest.copy_from_slice(reader.raw(32).map_err(invalid_encoding)?);
        baseline_records.push(ReplayBaselineRecord {
            owner,
            record_count,
            digest,
        });
    }
    let unowned_demand_count = reader.u64().map_err(invalid_encoding)?;
    let invalid_owner_site_count = reader.u64().map_err(invalid_encoding)?;
    let noncanonical_edge_count = reader.u64().map_err(invalid_encoding)?;
    let typed_reference_coverage_misses = reader.u64().map_err(invalid_encoding)?;
    reader.finish().map_err(invalid_encoding)?;

    let decoded = CollisionReplayIndex {
        schema,
        owner_partition,
        root_slots,
        owner_sites,
        reverse_edges,
        root_slot_consumers,
        scc_membership,
        statement_owners,
        baseline_records,
        unowned_demand_count,
        invalid_owner_site_count,
        noncanonical_edge_count,
        typed_reference_coverage_misses,
        canonical_manifest_bytes: Vec::new(),
        canonical_manifest_sha256,
    };
    // A successful parse already proves the unique wire form: integers and lengths are
    // fixed-width big-endian, every discriminant is exact, UTF-8 bytes are retained by
    // `String`, collection order is retained, and `finish` rejects every suffix. Therefore
    // decode-then-encode cannot reject anything that this reader accepted.
    Ok(DecodedCollisionReplayIndex {
        index: decoded,
        canonical_manifest_len: bytes.len(),
    })
}

#[cfg(test)]
pub(crate) fn decode_collision_replay_index(
    bytes: &[u8],
) -> Result<DecodedCollisionReplayIndex, ReplayIndexAdmissionError> {
    let decoded = decode_collision_replay_index_with_sha256(bytes, [0; 32])?;
    Ok(DecodedCollisionReplayIndex {
        index: CollisionReplayIndex {
            canonical_manifest_sha256: Sha256::digest(bytes).into(),
            ..decoded.index
        },
        canonical_manifest_len: decoded.canonical_manifest_len,
    })
}

pub(in crate::check::checker) fn decode_authenticated_collision_replay_index(
    bytes: &[u8],
    authenticated_sha256: [u8; 32],
) -> Result<DecodedCollisionReplayIndex, ReplayIndexAdmissionError> {
    decode_collision_replay_index_with_sha256(bytes, authenticated_sha256)
}

#[cfg(test)]
pub(crate) fn decode_collision_replay_index_for_test(
    bytes: &[u8],
) -> Result<CollisionReplayIndex, ReplayIndexAdmissionError> {
    decode_collision_replay_index(bytes).map(|decoded| decoded.index)
}

pub(crate) fn admit_decoded_collision_replay_index(
    decoded: DecodedCollisionReplayIndex,
    limits: ReplayIndexAdmissionLimits<'_>,
    expected_manifest_sha256: Option<[u8; 32]>,
) -> Result<AdmittedCollisionReplayIndex, ReplayIndexAdmissionError> {
    let canonical_manifest_len = decoded.canonical_manifest_len;
    let decoded = decoded.index;
    let expected_nonstatement_count = limits
        .type_groups
        .checked_add(limits.value_storages)
        .and_then(|count| count.checked_add(limits.namespaces))
        .and_then(|count| count.checked_add(limits.classes))
        .and_then(|count| count.checked_add(1))
        .ok_or(ReplayIndexAdmissionError::InvalidOwnerPartition)?;
    if decoded.owner_partition.len() < expected_nonstatement_count {
        return Err(ReplayIndexAdmissionError::InvalidOwnerPartition);
    }
    let mut expected_nonstatement = Vec::with_capacity(expected_nonstatement_count);
    for id in 0..limits.type_groups {
        expected_nonstatement.push(ReplayOwner::TypeGroup(TypeGroupId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.value_storages {
        expected_nonstatement.push(ReplayOwner::Value(ValueStorageId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.namespaces {
        expected_nonstatement.push(ReplayOwner::Namespace(NamespaceId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.classes {
        expected_nonstatement.push(ReplayOwner::Class(ClassId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    expected_nonstatement.push(ReplayOwner::GlobalObject);
    if decoded.owner_partition[..expected_nonstatement_count] != expected_nonstatement
        || decoded
            .owner_partition
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || decoded
            .owner_partition
            .iter()
            .copied()
            .any(|owner| !owner_in_bounds(owner, limits))
    {
        return Err(ReplayIndexAdmissionError::InvalidOwnerPartition);
    }
    let owner_ordinals = decoded
        .owner_partition
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, owner)| (owner, ordinal))
        .collect::<rustc_hash::FxHashMap<_, _>>();

    if decoded.root_slots.iter().any(|root| root.name.is_empty())
        || decoded
            .root_slots
            .windows(2)
            .any(|pair| pair[0].name >= pair[1].name)
        || decoded.root_slots.iter().any(|root| {
            !root
                .value
                .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.value_storages))
                || !root
                    .ty
                    .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.type_groups))
                || !root
                    .namespace
                    .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.namespaces))
        })
    {
        return Err(ReplayIndexAdmissionError::InvalidRootIndex);
    }
    let mut owner_site_ranges = Vec::with_capacity(decoded.owner_partition.len());
    let mut site_index = 0;
    let mut previous_site_key = None;
    for owner in &decoded.owner_partition {
        let range_start = site_index;
        while let Some(site) = decoded.owner_sites.get(site_index) {
            if site.owner != *owner {
                break;
            }
            let key = (
                site.owner,
                site.file_ordinal,
                site.span.start,
                site.span.end,
            );
            if previous_site_key.is_some_and(|previous| previous >= key)
                || site.file_ordinal.index() >= limits.source_files
                || site.span.start > site.span.end
            {
                return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
            }
            previous_site_key = Some(key);
            site_index += 1;
        }
        if site_index == range_start {
            return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
        }
        owner_site_ranges.push(
            ReplayRowRange::checked(range_start, site_index)
                .map_err(|_| ReplayIndexAdmissionError::InvalidOwnerSites)?,
        );
    }
    if site_index != decoded.owner_sites.len() {
        return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
    }

    let root_names = decoded
        .root_slots
        .iter()
        .map(|root| root.name.as_str())
        .collect::<rustc_hash::FxHashSet<_>>();
    if decoded
        .reverse_edges
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || decoded.reverse_edges.iter().any(|edge| {
            edge.dependency == edge.consumer
                || !owner_ordinals.contains_key(&edge.dependency)
                || !owner_ordinals.contains_key(&edge.consumer)
        })
        || decoded
            .root_slot_consumers
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || decoded.root_slot_consumers.iter().any(|edge| {
            !owner_ordinals.contains_key(&edge.consumer) || !root_names.contains(edge.name.as_str())
        })
    {
        return Err(ReplayIndexAdmissionError::InvalidDependencyGraph);
    }
    let canonical_roots = limits
        .roots
        .iter()
        .map(|root| (root.0.as_str(), (root.1, root.2, root.3)))
        .collect::<BTreeMap<_, _>>();
    let populated_roots = decoded
        .root_slots
        .iter()
        .filter(|root| root.value.is_some() || root.ty.is_some() || root.namespace.is_some())
        .map(|root| (root.name.as_str(), (root.value, root.ty, root.namespace)))
        .collect::<BTreeMap<_, _>>();
    let placeholder_names = decoded
        .root_slots
        .iter()
        .filter(|root| root.value.is_none() && root.ty.is_none() && root.namespace.is_none())
        .map(|root| root.name.as_str())
        .collect::<BTreeSet<_>>();
    let consumed_placeholder_names = decoded
        .root_slot_consumers
        .iter()
        .map(|consumer| consumer.name.as_str())
        .filter(|name| !canonical_roots.contains_key(name))
        .collect::<BTreeSet<_>>();
    if populated_roots != canonical_roots || placeholder_names != consumed_placeholder_names {
        return Err(ReplayIndexAdmissionError::InvalidRootIndex);
    }
    let mut root_slot_lookup = rustc_hash::FxHashMap::default();
    for (ordinal, root) in decoded.root_slots.iter().enumerate() {
        let lookup = ReplayRootLookup {
            root_ordinal: u32::try_from(ordinal)
                .map_err(|_| ReplayIndexAdmissionError::InvalidRootIndex)?,
            value_seeds: root.value.map(ReplayOwner::Value).into_iter().collect(),
            type_seeds: root.ty.map(ReplayOwner::TypeGroup).into_iter().collect(),
            namespace_seeds: root
                .namespace
                .map(ReplayOwner::Namespace)
                .into_iter()
                .collect(),
        };
        if root_slot_lookup
            .insert(root.name.clone().into_boxed_str(), lookup)
            .is_some()
        {
            return Err(ReplayIndexAdmissionError::InvalidRootIndex);
        }
    }
    for consumer in &decoded.root_slot_consumers {
        let lookup = root_slot_lookup
            .get_mut(consumer.name.as_str())
            .ok_or(ReplayIndexAdmissionError::InvalidRootIndex)?;
        lookup.seeds_mut(consumer.slot).push(consumer.consumer);
    }
    for lookup in root_slot_lookup.values_mut() {
        for owners in [
            &mut lookup.value_seeds,
            &mut lookup.type_seeds,
            &mut lookup.namespace_seeds,
        ] {
            owners.sort_unstable();
            owners.dedup();
        }
    }

    let mut owner_to_scc = vec![u32::MAX; decoded.owner_partition.len()];
    let mut scc_owner_ranges = Vec::with_capacity(decoded.scc_membership.len());
    let mut scc_owners = Vec::with_capacity(decoded.owner_partition.len());
    for (ordinal, component) in decoded.scc_membership.iter().enumerate() {
        let replay_ordinal =
            u32::try_from(ordinal).map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        if component.replay_ordinal != replay_ordinal
            || component.owners.is_empty()
            || component.owners.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        let range_start = scc_owners.len();
        for owner in &component.owners {
            let Some(owner_ordinal) = owner_ordinals.get(owner).copied() else {
                return Err(ReplayIndexAdmissionError::InvalidSccPartition);
            };
            if owner_to_scc[owner_ordinal] != u32::MAX {
                return Err(ReplayIndexAdmissionError::InvalidSccPartition);
            }
            owner_to_scc[owner_ordinal] = replay_ordinal;
            scc_owners.push(*owner);
        }
        scc_owner_ranges.push(
            ReplayRowRange::checked(range_start, scc_owners.len())
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
        );
    }
    if owner_to_scc.contains(&u32::MAX) {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }
    let mut forward = rustc_hash::FxHashMap::<ReplayOwner, Vec<ReplayOwner>>::default();
    let mut reverse = rustc_hash::FxHashMap::<ReplayOwner, Vec<ReplayOwner>>::default();
    let mut component_edges = Vec::new();
    for edge in &decoded.reverse_edges {
        let dependency_ordinal = owner_ordinals[&edge.dependency];
        let consumer_ordinal = owner_ordinals[&edge.consumer];
        let dependency_scc = owner_to_scc[dependency_ordinal];
        let consumer_scc = owner_to_scc[consumer_ordinal];
        if dependency_scc > consumer_scc {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        if dependency_scc == consumer_scc {
            forward
                .entry(edge.consumer)
                .or_default()
                .push(edge.dependency);
            reverse
                .entry(edge.dependency)
                .or_default()
                .push(edge.consumer);
        } else {
            component_edges.push((dependency_scc, consumer_scc));
        }
    }
    drop(owner_ordinals);
    component_edges.sort_unstable();
    component_edges.dedup();

    let component_count = decoded.scc_membership.len();
    let mut dependency_counts = vec![0usize; component_count];
    let mut reverse_scc_offsets = Vec::with_capacity(component_count.saturating_add(1));
    let mut reverse_scc_edges = Vec::with_capacity(component_edges.len());
    let mut edge_index = 0;
    for dependency in 0..component_count {
        reverse_scc_offsets.push(
            u32::try_from(reverse_scc_edges.len())
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
        );
        let dependency = u32::try_from(dependency)
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        while component_edges
            .get(edge_index)
            .is_some_and(|edge| edge.0 == dependency)
        {
            let consumer = component_edges[edge_index].1;
            let consumer_index = usize::try_from(consumer)
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
            dependency_counts[consumer_index] = dependency_counts[consumer_index]
                .checked_add(1)
                .ok_or(ReplayIndexAdmissionError::InvalidSccPartition)?;
            reverse_scc_edges.push(consumer);
            edge_index += 1;
        }
    }
    reverse_scc_offsets.push(
        u32::try_from(reverse_scc_edges.len())
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
    );
    if edge_index != component_edges.len() {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }
    fn reaches_component(
        start: ReplayOwner,
        adjacency: &rustc_hash::FxHashMap<ReplayOwner, Vec<ReplayOwner>>,
    ) -> rustc_hash::FxHashSet<ReplayOwner> {
        let mut reached = rustc_hash::FxHashSet::default();
        let mut pending = vec![start];
        while let Some(owner) = pending.pop() {
            if reached.insert(owner) {
                pending.extend(adjacency.get(&owner).into_iter().flatten().copied());
            }
        }
        reached
    }
    for component in decoded
        .scc_membership
        .iter()
        .filter(|component| component.owners.len() > 1)
    {
        let members = component
            .owners
            .iter()
            .copied()
            .collect::<rustc_hash::FxHashSet<_>>();
        let start = component.owners[0];
        if reaches_component(start, &forward) != members
            || reaches_component(start, &reverse) != members
        {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
    }
    let mut ready = std::collections::BinaryHeap::new();
    for (component, count) in dependency_counts.iter().copied().enumerate() {
        if count == 0 {
            ready.push(std::cmp::Reverse((
                decoded.scc_membership[component].owners[0],
                component,
            )));
        }
    }
    for expected in 0..decoded.scc_membership.len() {
        let Some(std::cmp::Reverse((_, component))) = ready.pop() else {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        };
        if component != expected {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        let edge_start = usize::try_from(reverse_scc_offsets[component])
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        let edge_end = usize::try_from(reverse_scc_offsets[component + 1])
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        for dependent in reverse_scc_edges[edge_start..edge_end].iter().copied() {
            let dependent = usize::try_from(dependent)
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
            dependency_counts[dependent] -= 1;
            if dependency_counts[dependent] == 0 {
                ready.push(std::cmp::Reverse((
                    decoded.scc_membership[dependent].owners[0],
                    dependent,
                )));
            }
        }
    }
    if !ready.is_empty() {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }

    let expected_statements = decoded.owner_partition[expected_nonstatement_count..]
        .iter()
        .filter_map(|owner| match owner {
            ReplayOwner::Statement(key) => Some((*key, *owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if expected_statements.len() != decoded.owner_partition.len() - expected_nonstatement_count
        || decoded.statement_owners != expected_statements
    {
        return Err(ReplayIndexAdmissionError::InvalidStatementPartition);
    }
    let empty_digest = empty_baseline_digest();
    if decoded.baseline_records.len() != decoded.owner_partition.len() {
        return Err(ReplayIndexAdmissionError::InvalidBaselinePartition);
    }
    let mut baseline_record_ranges = Vec::with_capacity(decoded.owner_partition.len());
    for (index, (owner, record)) in decoded
        .owner_partition
        .iter()
        .zip(&decoded.baseline_records)
        .enumerate()
    {
        if record.owner != *owner
            || (!matches!(record.owner, ReplayOwner::Statement(_))
                && (record.record_count != 0 || record.digest != empty_digest))
        {
            return Err(ReplayIndexAdmissionError::InvalidBaselinePartition);
        }
        let end = index
            .checked_add(1)
            .ok_or(ReplayIndexAdmissionError::InvalidBaselinePartition)?;
        let range = ReplayRowRange::checked(index, end)
            .map_err(|_| ReplayIndexAdmissionError::InvalidBaselinePartition)?;
        baseline_record_ranges.push(range);
    }
    if decoded.unowned_demand_count != 0
        || decoded.invalid_owner_site_count != 0
        || decoded.noncanonical_edge_count != 0
        || decoded.typed_reference_coverage_misses != 0
    {
        return Err(ReplayIndexAdmissionError::NonzeroGenerationHealthCounter);
    }
    if expected_manifest_sha256
        .is_some_and(|expected| decoded.canonical_manifest_sha256 != expected)
    {
        return Err(ReplayIndexAdmissionError::ManifestIdentityMismatch);
    }

    Ok(AdmittedCollisionReplayIndex {
        schema: decoded.schema,
        owner_partition: decoded.owner_partition,
        root_slots: decoded.root_slots,
        owner_sites: decoded.owner_sites,
        reverse_edges: decoded.reverse_edges,
        root_slot_consumers: decoded.root_slot_consumers,
        scc_membership: decoded.scc_membership,
        statement_owners: decoded.statement_owners,
        baseline_records: decoded.baseline_records,
        unowned_demand_count: decoded.unowned_demand_count,
        invalid_owner_site_count: decoded.invalid_owner_site_count,
        noncanonical_edge_count: decoded.noncanonical_edge_count,
        typed_reference_coverage_misses: decoded.typed_reference_coverage_misses,
        owner_to_scc,
        scc_owner_ranges,
        scc_owners,
        reverse_scc_offsets,
        reverse_scc_edges,
        root_slot_lookup,
        owner_site_ranges,
        baseline_record_ranges,
        canonical_manifest_len,
        canonical_manifest_sha256: decoded.canonical_manifest_sha256,
    })
}

#[cfg(test)]
pub(crate) fn admit_collision_replay_index(
    bytes: &[u8],
    limits: ReplayIndexAdmissionLimits<'_>,
    expected_manifest_sha256: Option<[u8; 32]>,
) -> Result<AdmittedCollisionReplayIndex, ReplayIndexAdmissionError> {
    let decoded = decode_collision_replay_index(bytes)?;
    admit_decoded_collision_replay_index(decoded, limits, expected_manifest_sha256)
}

fn empty_baseline_digest() -> [u8; 32] {
    let mut bytes = ManifestBytes::new(b"typokat-collision-replay-owner-records-v1");
    bytes.u64(0);
    Sha256::digest(bytes.finish()).into()
}

pub(crate) fn baseline_record(
    owner: ReplayOwner,
    records: &[Vec<u8>],
) -> Result<ReplayBaselineRecord, ReplayIndexGenerationError> {
    let mut bytes = ManifestBytes::new(b"typokat-collision-replay-owner-records-v1");
    bytes.u64(
        u64::try_from(records.len()).map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
    );
    for record in records {
        bytes.bytes(record)?;
    }
    Ok(ReplayBaselineRecord {
        owner,
        record_count: u64::try_from(records.len())
            .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
        digest: Sha256::digest(bytes.finish()).into(),
    })
}

struct ManifestBytes(Vec<u8>);

impl ManifestBytes {
    fn new(domain: &[u8]) -> Self {
        Self(domain.to_vec())
    }

    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) -> Result<(), ReplayIndexGenerationError> {
        self.u64(u64::try_from(value).map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?);
        Ok(())
    }

    fn optional_u32(&mut self, value: Option<u32>) {
        self.u8(u8::from(value.is_some()));
        if let Some(value) = value {
            self.u32(value);
        }
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), ReplayIndexGenerationError> {
        self.usize(value.len())?;
        self.raw(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), ReplayIndexGenerationError> {
        self.bytes(value.as_bytes())
    }

    fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(file: usize, start: u32) -> LibraryEventKey {
        LibraryEventKey {
            file_ordinal: LibraryFileOrdinal::new(file),
            source_start: start,
            event_ordinal: 0,
            record_ordinal: 0,
        }
    }

    fn site(owner: ReplayOwner) -> ReplayOwnerSite {
        ReplayOwnerSite {
            owner,
            file_ordinal: LibraryFileOrdinal::new(0),
            span: Span::new(0, 1),
        }
    }

    fn baselines(owners: &[ReplayOwner]) -> Vec<ReplayBaselineRecord> {
        owners
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).unwrap())
            .collect()
    }

    #[test]
    fn owner_tags_and_order_are_stable() {
        let owners = [
            ReplayOwner::TypeGroup(TypeGroupId(1)),
            ReplayOwner::Value(ValueStorageId(1)),
            ReplayOwner::Namespace(NamespaceId(1)),
            ReplayOwner::Class(ClassId(1)),
            ReplayOwner::GlobalObject,
            ReplayOwner::Statement(key(0, 1)),
        ];
        assert_eq!(owners.map(ReplayOwner::tag), [0, 1, 2, 3, 4, 5]);
        assert!(owners.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn owner_scope_restores_nesting_and_counts_unowned_demand() {
        let trace = ReplayDependencyTrace::default();
        let outer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let inner = ReplayOwner::Value(ValueStorageId(2));
        trace.enter(outer);
        trace.enter(inner);
        trace.demand(outer);
        trace.leave(inner);
        assert_eq!(trace.current_owner(), Some(outer));
        trace.leave(outer);
        trace.demand(inner);
        assert_eq!(trace.counters(), (1, 0));
    }

    #[test]
    fn owner_scope_restores_after_unwind() {
        let trace = ReplayDependencyTrace::default();
        let owner = ReplayOwner::GlobalObject;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
            let trace = trace.clone();
            move || {
                let _scope = trace.scope(owner);
                panic!("scope restoration witness");
            }
        }));
        assert!(outcome.is_err());
        assert_eq!(trace.current_owner(), None);
    }

    #[test]
    fn reverse_edges_are_oriented_dependency_to_consumer_and_deduplicated() {
        let trace = ReplayDependencyTrace::default();
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(2));
        let dependency = ReplayOwner::TypeGroup(TypeGroupId(1));
        trace.enter(consumer);
        trace.demand(dependency);
        trace.demand(dependency);
        trace.leave(consumer);
        let owners = vec![consumer, dependency];
        let baseline_records = baselines(&owners);
        let index = trace
            .finish(
                owners.clone(),
                Vec::new(),
                owners.into_iter().map(site).collect(),
                Vec::new(),
                baseline_records,
                0,
            )
            .unwrap();
        assert_eq!(
            index.reverse_edges,
            [ReplayReverseEdge {
                dependency,
                consumer
            }]
        );
    }

    #[test]
    fn dependency_first_scc_order_handles_a_long_chain_without_drift() {
        let trace = ReplayDependencyTrace::default();
        let owners = (0..20_000)
            .map(|id| ReplayOwner::TypeGroup(TypeGroupId(id)))
            .collect::<Vec<_>>();
        for pair in owners.windows(2) {
            trace.enter(pair[1]);
            trace.demand(pair[0]);
            trace.leave(pair[1]);
        }
        let baseline_records = baselines(&owners);
        let index = trace
            .finish(
                owners.clone(),
                Vec::new(),
                owners.iter().copied().map(site).collect(),
                Vec::new(),
                baseline_records,
                0,
            )
            .unwrap();
        assert_eq!(index.scc_membership.len(), owners.len());
        assert_eq!(index.scc_membership[0].owners.as_slice(), [owners[0]]);
        assert_eq!(
            index.scc_membership.last().unwrap().owners.as_slice(),
            [*owners.last().unwrap()]
        );
    }

    #[test]
    fn statement_owners_use_canonical_event_keys() {
        let statement = key(7, 41);
        let owner = ReplayOwner::Statement(statement);
        let trace = ReplayDependencyTrace::default();
        let index = trace
            .finish(
                vec![owner],
                Vec::new(),
                vec![site(owner)],
                vec![statement],
                vec![baseline_record(owner, &[]).unwrap()],
                0,
            )
            .unwrap();
        assert_eq!(index.statement_owners, [(statement, owner)]);
    }

    #[test]
    fn baseline_record_digest_is_domain_separated_and_count_sensitive() {
        let owner = ReplayOwner::GlobalObject;
        let empty = baseline_record(owner, &[]).unwrap();
        let one = baseline_record(owner, &[b"record".to_vec()]).unwrap();
        assert_eq!(empty.digest, empty_baseline_digest());
        assert_ne!(empty.digest, one.digest);
        assert_eq!(one.record_count, 1);
    }

    #[test]
    fn invalid_provenance_fails_generation() {
        let owner = ReplayOwner::GlobalObject;
        let error = ReplayDependencyTrace::default()
            .finish(
                vec![owner],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                baselines(&[owner]),
                0,
            )
            .unwrap_err();
        assert_eq!(error, ReplayIndexGenerationError::MissingOwnerSite(owner));
    }

    #[test]
    fn active_owner_without_a_typed_dependency_fails_coverage() {
        let owner = ReplayOwner::GlobalObject;
        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        drop(trace.observe_typed_demand("missing-test-edge"));
        trace.leave(owner);
        let error = trace
            .finish(
                vec![owner],
                Vec::new(),
                vec![site(owner)],
                Vec::new(),
                baselines(&[owner]),
                0,
            )
            .unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage {
                count: 1,
                samples: vec![ReplayCoverageMiss::Boundary("missing-test-edge")],
            }
        );
    }

    #[test]
    fn canonical_manifest_is_permutation_invariant() {
        let first = ReplayOwner::Value(ValueStorageId(1));
        let second = ReplayOwner::TypeGroup(TypeGroupId(2));
        let build = |reverse: bool| {
            let trace = ReplayDependencyTrace::default();
            trace.enter(second);
            trace.demand(first);
            trace.leave(second);
            let mut owners = vec![first, second];
            let mut sites = owners.iter().copied().map(site).collect::<Vec<_>>();
            let mut baselines = baselines(&owners);
            let mut roots = vec![
                ReplayRootSlot {
                    name: "B".to_owned(),
                    value: Some(ValueStorageId(1)),
                    ty: None,
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                },
                ReplayRootSlot {
                    name: "A".to_owned(),
                    value: None,
                    ty: Some(TypeGroupId(2)),
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                },
            ];
            if reverse {
                owners.reverse();
                sites.reverse();
                baselines.reverse();
                roots.reverse();
            }
            trace
                .finish(owners, roots, sites, Vec::new(), baselines, 0)
                .unwrap()
        };
        let forward = build(false);
        let reverse = build(true);
        assert_eq!(
            forward.canonical_manifest_bytes,
            reverse.canonical_manifest_bytes
        );
        assert_eq!(
            forward.canonical_manifest_sha256,
            reverse.canonical_manifest_sha256
        );
    }

    #[test]
    fn malformed_edge_rows_increment_the_real_validator_counter() {
        let owner = ReplayOwner::GlobalObject;
        let partition = BTreeSet::from([owner]);
        assert_eq!(
            validate_edge_rows(
                &partition,
                &[ReplayReverseEdge {
                    dependency: owner,
                    consumer: owner,
                }],
                &[],
            ),
            1
        );
    }

    fn finish_dependency_fixture(
        trace: ReplayDependencyTrace,
        owners: Vec<ReplayOwner>,
    ) -> Result<CollisionReplayIndex, ReplayIndexGenerationError> {
        trace.finish(
            owners.clone(),
            Vec::new(),
            owners.iter().copied().map(site).collect(),
            Vec::new(),
            baselines(&owners),
            0,
        )
    }

    #[test]
    fn typed_coverage_accepts_a_forward_transitive_class_dependency() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let intermediate = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(consumer);
        trace.demand(intermediate);
        trace.leave(consumer);
        trace.enter(intermediate);
        trace.demand(class);
        trace.leave(intermediate);
        trace.require_dependency(consumer, class);
        assert!(finish_dependency_fixture(trace, vec![consumer, intermediate, class]).is_ok());
    }

    #[test]
    fn typed_coverage_rejects_a_reverse_only_class_edge() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(class);
        trace.demand(consumer);
        trace.leave(class);
        trace.require_dependency(consumer, class);
        let error = finish_dependency_fixture(trace, vec![consumer, class]).unwrap_err();
        assert!(matches!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage { count: 1, .. }
        ));
    }

    #[test]
    fn typed_coverage_accepts_an_scc_with_an_outgoing_class_dependency() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let peer = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(consumer);
        trace.demand(peer);
        trace.leave(consumer);
        trace.enter(peer);
        trace.demand(consumer);
        trace.demand(class);
        trace.leave(peer);
        trace.require_dependency(consumer, class);
        assert!(finish_dependency_fixture(trace, vec![consumer, peer, class]).is_ok());
    }

    #[test]
    fn typed_coverage_does_not_reverse_an_unrelated_dependency_chain() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let unrelated = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(class);
        trace.demand(unrelated);
        trace.leave(class);
        trace.enter(unrelated);
        trace.demand(consumer);
        trace.leave(unrelated);
        trace.require_dependency(consumer, class);
        let error = finish_dependency_fixture(trace, vec![consumer, unrelated, class]).unwrap_err();
        assert!(matches!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage { count: 1, .. }
        ));
    }

    fn normalize_source(source: &str) -> String {
        source.split_whitespace().collect()
    }

    const DENIED_RAW_REPLAY_TOKENS: &[&str] = &[
        ".resolve_value_binding(",
        ".resolve_value(",
        ".resolve_type(",
        ".resolve_qualified_type_path(",
        ".published_class(",
        "type_decl_id(",
        "value_decl_id(",
        "decl_types.get(",
        "decl_types.set(",
        "class_parents.get(",
        "class_parents.iter(",
        "class_parents.local_iter(",
    ];

    struct RawAccessAllowance {
        path: &'static str,
        snippet: &'static str,
        count: usize,
        reason: &'static str,
    }

    const TEST_ONLY_REPLAY_SOURCE_PATHS: &[&str] = &[
        "src/check/checker/calls/contextual_rewalk_scaling_spec.rs",
        "src/check/checker/declaration_owner_scaling_spec.rs",
        "src/check/checker/declaration_surface_lazy_spec.rs",
        "src/check/checker/declaration_surface_measure.rs",
        "src/check/checker/decls/cycle_tainted_application_cache_spec.rs",
        "src/check/checker/decls/eager_application_cache_spec.rs",
        "src/check/checker/decls/interface_scc_pending_spec.rs",
        "src/check/checker/eval/deferred_keyof_cache_spec.rs",
        "src/check/checker/eval/tests.rs",
        "src/check/checker/library_snapshot_codec/spec.rs",
        "src/check/checker/lexical_events/owner_lookup_spec.rs",
        "src/check/query/deferred_indexed_lazy_spec.rs",
        "src/check/query/demand_identity_spec.rs",
        "src/check/query/dom_source_cold_spec.rs",
        "src/check/query/event_listener_union_scaling_spec.rs",
        "src/check/query/identity_memo_spec.rs",
        "src/check/query/instantiation_root_lazy_spec.rs",
        "src/check/query/relation_root_lazy_spec.rs",
        "src/check/query/tests.rs",
        "src/check/query/transaction_fork_scaling_spec.rs",
    ];

    const EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS: &[&str] = &[
        "src/check/checker/annotations/composites.rs",
        "src/check/checker/annotations/declared.rs",
        "src/check/checker/annotations/functions.rs",
        "src/check/checker/annotations/mod.rs",
        "src/check/checker/annotations/signatures.rs",
        "src/check/checker/annotations/type_operators.rs",
        "src/check/checker/assignment.rs",
        "src/check/checker/calls.rs",
        "src/check/checker/classes/application.rs",
        "src/check/checker/classes/body.rs",
        "src/check/checker/classes/construction.rs",
        "src/check/checker/classes/inheritance.rs",
        "src/check/checker/classes/initializer.rs",
        "src/check/checker/classes/members.rs",
        "src/check/checker/classes/mod.rs",
        "src/check/checker/classes/publication.rs",
        "src/check/checker/classes/retained.rs",
        "src/check/checker/classes/surface_types.rs",
        "src/check/checker/classes/type_syntax.rs",
        "src/check/checker/classes/visibility.rs",
        "src/check/checker/context.rs",
        "src/check/checker/decls/interface.rs",
        "src/check/checker/decls/mod.rs",
        "src/check/checker/decls/params.rs",
        "src/check/checker/decls/resolve.rs",
        "src/check/checker/eval/demand.rs",
        "src/check/checker/eval/extends.rs",
        "src/check/checker/eval/instantiation.rs",
        "src/check/checker/eval/keyof.rs",
        "src/check/checker/eval/mapped.rs",
        "src/check/checker/eval/mod.rs",
        "src/check/checker/eval/template.rs",
        "src/check/checker/events.rs",
        "src/check/checker/events_library.rs",
        "src/check/checker/expr.rs",
        "src/check/checker/flowgraph/exprs.rs",
        "src/check/checker/flowgraph/mod.rs",
        "src/check/checker/flowgraph/nodes.rs",
        "src/check/checker/function_groups.rs",
        "src/check/checker/indexed_access.rs",
        "src/check/checker/lexical_events.rs",
        "src/check/checker/lexical_events_library.rs",
        "src/check/checker/lexical_events_user.rs",
        "src/check/checker/library_compiler.rs",
        "src/check/checker/library_identities.rs",
        "src/check/checker/library_reporting.rs",
        "src/check/checker/library_snapshot_codec/mod.rs",
        "src/check/checker/library_snapshot_codec/profile.rs",
        "src/check/checker/library_snapshot_codec/runtime.rs",
        "src/check/checker/mod.rs",
        "src/check/checker/namespace_values.rs",
        "src/check/checker/narrowing.rs",
        "src/check/checker/replay_index.rs",
        "src/check/checker/reporting_record.rs",
        "src/check/checker/statements.rs",
        "src/check/checker/type_groups.rs",
        "src/check/query/mod.rs",
    ];

    fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
        let mut cursor = start;
        if bytes.get(cursor) == Some(&b'b') {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'r') {
            return None;
        }
        cursor += 1;
        let hashes = bytes[cursor..]
            .iter()
            .take_while(|byte| **byte == b'#')
            .count();
        cursor += hashes;
        if bytes.get(cursor) != Some(&b'"') {
            return None;
        }
        cursor += 1;
        while cursor < bytes.len() {
            if bytes[cursor] == b'"'
                && bytes
                    .get(cursor + 1..cursor + 1 + hashes)
                    .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
            {
                return Some(cursor + 1 + hashes);
            }
            cursor += 1;
        }
        Some(bytes.len())
    }

    fn quoted_end(bytes: &[u8], start: usize, quote: u8) -> usize {
        let mut cursor = start + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' => cursor = (cursor + 2).min(bytes.len()),
                byte if byte == quote => return cursor + 1,
                _ => cursor += 1,
            }
        }
        bytes.len()
    }

    fn skip_non_code(bytes: &[u8], cursor: usize) -> Option<usize> {
        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            return Some(
                bytes[cursor + 2..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |offset| cursor + 3 + offset),
            );
        }
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            let mut depth = 1usize;
            let mut end = cursor + 2;
            while end < bytes.len() && depth != 0 {
                if bytes.get(end..end + 2) == Some(b"/*") {
                    depth += 1;
                    end += 2;
                } else if bytes.get(end..end + 2) == Some(b"*/") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            return Some(end);
        }
        if let Some(end) = raw_string_end(bytes, cursor) {
            return Some(end);
        }
        if bytes[cursor] == b'"' {
            return Some(quoted_end(bytes, cursor, b'"'));
        }
        if bytes[cursor] == b'\'' {
            let end = quoted_end(bytes, cursor, b'\'');
            if end.saturating_sub(cursor) <= 4 {
                return Some(end);
            }
        }
        None
    }

    fn cfg_test_item_end(source: &str, item_start: usize) -> usize {
        let bytes = source.as_bytes();
        let mut cursor = item_start;
        let mut parentheses = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        while cursor < bytes.len() {
            if let Some(end) = skip_non_code(bytes, cursor) {
                cursor = end;
                continue;
            }
            match bytes[cursor] {
                b'(' => parentheses += 1,
                b')' => parentheses = parentheses.saturating_sub(1),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'{' => braces += 1,
                b'}' if braces != 0 => {
                    braces -= 1;
                    if braces == 0 && parentheses == 0 && brackets == 0 {
                        cursor += 1;
                        while cursor < bytes.len() {
                            if let Some(end) = skip_non_code(bytes, cursor) {
                                cursor = end;
                            } else if bytes[cursor].is_ascii_whitespace() {
                                cursor += 1;
                            } else {
                                break;
                            }
                        }
                        if matches!(bytes.get(cursor), Some(b',' | b';')) {
                            cursor += 1;
                        }
                        return cursor;
                    }
                }
                b';' if parentheses == 0 && brackets == 0 && braces == 0 => return cursor + 1,
                _ => {}
            }
            cursor += 1;
        }
        bytes.len()
    }

    fn strip_keyword<'a>(source: &'a str, keyword: &str) -> Option<&'a str> {
        let remainder = source.strip_prefix(keyword)?;
        if remainder
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return None;
        }
        Some(remainder)
    }

    fn cfg_test_starts_item(source: &str, item_start: usize) -> bool {
        let mut remainder = source[item_start..].trim_start();
        while remainder.starts_with("#[") {
            let bytes = remainder.as_bytes();
            let mut cursor = 2usize;
            let mut brackets = 1usize;
            while cursor < bytes.len() && brackets != 0 {
                if let Some(end) = skip_non_code(bytes, cursor) {
                    cursor = end;
                    continue;
                }
                match bytes[cursor] {
                    b'[' => brackets += 1,
                    b']' => brackets -= 1,
                    _ => {}
                }
                cursor += 1;
            }
            remainder = remainder[cursor..].trim_start();
        }

        if let Some(after_pub) = strip_keyword(remainder, "pub") {
            remainder = after_pub.trim_start();
            if remainder.starts_with('(') {
                let Some(end) = remainder.find(')') else {
                    return false;
                };
                remainder = remainder[end + 1..].trim_start();
            }
        }
        while let Some(after_wrapper) = ["async", "unsafe", "default"]
            .iter()
            .find_map(|wrapper| strip_keyword(remainder, wrapper))
        {
            remainder = after_wrapper.trim_start();
        }
        if let Some(after_extern) = strip_keyword(remainder, "extern") {
            remainder = after_extern.trim_start();
            if remainder.starts_with('"') {
                remainder = &remainder[quoted_end(remainder.as_bytes(), 0, b'"')..];
            }
            remainder = remainder.trim_start();
        }

        [
            "fn",
            "mod",
            "impl",
            "use",
            "const",
            "static",
            "struct",
            "enum",
            "type",
            "trait",
            "union",
            "macro",
            "macro_rules",
        ]
        .iter()
        .any(|keyword| strip_keyword(remainder, keyword).is_some())
    }

    fn next_cfg_test_attribute(source: &str, search_from: usize) -> Option<usize> {
        const CFG_TEST: &[u8] = b"#[cfg(test)]";
        let bytes = source.as_bytes();
        let mut cursor = search_from;
        while cursor < bytes.len() {
            if let Some(end) = skip_non_code(bytes, cursor) {
                cursor = end.max(cursor + 1);
                continue;
            }
            if bytes.get(cursor..cursor + CFG_TEST.len()) == Some(CFG_TEST) {
                return Some(cursor);
            }
            cursor += 1;
        }
        None
    }

    fn production_source(source: &str) -> String {
        const CFG_TEST_LEN: usize = b"#[cfg(test)]".len();
        let mut stripped = source.to_owned();
        let mut search_from = 0usize;
        while let Some(start) = next_cfg_test_attribute(&stripped, search_from) {
            let item_start = start + CFG_TEST_LEN;
            let end = if cfg_test_starts_item(&stripped, item_start) {
                cfg_test_item_end(&stripped, item_start)
            } else {
                item_start
            };
            stripped.replace_range(start..end, " ");
            search_from = start + 1;
        }
        stripped
    }

    fn discover_production_rust_sources(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<String, String> {
        fn visit(
            root: &std::path::Path,
            directory: &std::path::Path,
            sources: &mut std::collections::BTreeMap<String, String>,
        ) {
            let mut entries = std::fs::read_dir(directory)
                .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
                .collect::<Result<Vec<_>, _>>()
                .expect("read checker source entries");
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    if entry.file_name() != "tests" {
                        visit(root, &path, sources);
                    }
                    continue;
                }
                if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .expect("source stays below manifest root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if TEST_ONLY_REPLAY_SOURCE_PATHS.contains(&relative.as_str()) {
                    continue;
                }
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                assert!(sources.insert(relative, source).is_none());
            }
        }

        let mut sources = std::collections::BTreeMap::new();
        visit(root, &root.join("src/check/checker"), &mut sources);
        visit(root, &root.join("src/check/query"), &mut sources);
        sources
    }

    fn validate_production_source_set(
        sources: &std::collections::BTreeMap<String, String>,
        expected_paths: &[&str],
    ) -> Result<(), String> {
        let actual = sources
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let expected = expected_paths
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if actual == expected {
            return Ok(());
        }
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).copied().collect::<Vec<_>>();
        Err(format!(
            "production replay source manifest drift: missing={missing:?}, unexpected={unexpected:?}"
        ))
    }

    fn validate_raw_access_manifest(
        sources: &std::collections::BTreeMap<String, String>,
        expected_paths: &[&str],
        allowed: &[RawAccessAllowance],
    ) -> Result<(), String> {
        validate_production_source_set(sources, expected_paths)?;
        let mut production = sources
            .iter()
            .map(|(path, source)| (path.clone(), normalize_source(&production_source(source))))
            .collect::<std::collections::BTreeMap<_, _>>();
        for allowance in allowed {
            let Some(remainder) = production.get_mut(allowance.path) else {
                return Err(format!(
                    "raw replay allowance references undiscovered file {}: {}",
                    allowance.path, allowance.reason
                ));
            };
            let snippet = normalize_source(allowance.snippet);
            let actual = remainder.matches(&snippet).count();
            if actual != allowance.count {
                return Err(format!(
                    "raw replay allowance drift in {}: {} (expected {}, found {})",
                    allowance.path, allowance.reason, allowance.count, actual
                ));
            }
            *remainder = remainder.replacen(&snippet, "", allowance.count);
        }
        for (path, remainder) in &production {
            for token in DENIED_RAW_REPLAY_TOKENS {
                if remainder.contains(token) {
                    return Err(format!("unapproved raw replay access `{token}` in {path}"));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn semantic_replay_accesses_have_an_exact_source_allowlist() {
        let root = crate::test_repository_root();
        let sources = discover_production_rust_sources(&root);
        let allowed = [
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "parts.class_parents.iter()", count: 1, reason: "snapshot ordering" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "self.class_parents.iter()", count: 1, reason: "snapshot projection" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "type_decl_id(binder, binder.prelude_module, name)", count: 1, reason: "prelude identity selection" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "return self.binder.resolve_value_binding(scope, name);", count: 1, reason: "no-trace value binding fast path" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "return self.binder.resolve_value(scope, name);", count: 1, reason: "no-trace value fast path" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "return self.binder.resolve_type(scope, name);", count: 1, reason: "no-trace type fast path" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "let symbol = self.binder.resolve_type(scope, name)?;", count: 1, reason: "speculative planner read; dependency replayed on commit; fallback performs canonical traced lowering" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "return self.binder.resolve_qualified_type_path(scope, segments);", count: 1, reason: "no-trace qualified fast path" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: ".classes().published_class(class)", count: 1, reason: "instrumented class delegate" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "return self.decl_types.get(storage);", count: 1, reason: "no-trace decl fast path" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "self.decl_types.get(storage)", count: 1, reason: "instrumented decl delegate" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "decl_types.set(decl_id, error)", count: 1, reason: "module placeholder bootstrap" },
            RawAccessAllowance { path: "src/check/checker/mod.rs", snippet: "|pass| pass.decl_types.set(storage, ty)", count: 1, reason: "owned copied publication" },
            RawAccessAllowance { path: "src/check/checker/assignment.rs", snippet: "binder.resolve_value(scope, name)", count: 1, reason: "current binding identity" },
            RawAccessAllowance { path: "src/check/checker/calls.rs", snippet: ".binder.resolve_value(scope, &name)", count: 5, reason: "current callable parameter identity" },
            RawAccessAllowance { path: "src/check/checker/calls.rs", snippet: ".binder.resolve_value(scope, &n)", count: 1, reason: "current callable parameter identity" },
            RawAccessAllowance { path: "src/check/checker/statements.rs", snippet: "self.decl_types.get(decl_id)", count: 2, reason: "own-target cache invalidation" },
            RawAccessAllowance { path: "src/check/checker/statements.rs", snippet: "self.binder.resolve_value(scope, id.name.as_str())", count: 1, reason: "current declaration symbol" },
            RawAccessAllowance { path: "src/check/checker/statements.rs", snippet: ".binder.resolve_value(scope, name)", count: 2, reason: "current function group symbol" },
            RawAccessAllowance { path: "src/check/checker/statements.rs", snippet: "binder.resolve_value(declaration_scope, identifier.name.as_str())", count: 1, reason: "declaration owner lookup" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: "type_decl_id(self.binder, self.scope, name)", count: 1, reason: "no-trace surface resolver" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: "self.binder.resolve_qualified_type_path(self.scope, segments)", count: 1, reason: "no-trace qualified resolver" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: ".staged_published_classes.as_ref().expect(\"class publication is staged\").published_class(class)", count: 1, reason: "instrumented staged class delegate" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: "self.decl_types.set(value_decl, static_side)", count: 1, reason: "class-owned construction staging" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: "self.decl_types.set(value_decl, surface.static_template())", count: 1, reason: "value-owned final class publication" },
            RawAccessAllowance { path: "src/check/checker/classes/publication.rs", snippet: "self.class_parents.get(&class_id)", count: 1, reason: "child-owned parent metadata" },
            RawAccessAllowance { path: "src/check/checker/classes/inheritance.rs", snippet: "self.class_parents.get(&current)", count: 1, reason: "child-owned parent chain" },
            RawAccessAllowance { path: "src/check/checker/decls/interface.rs", snippet: ".expect(\"class registry is frozen before interface heritage construction\").published_class(application.class)", count: 1, reason: "instrumented interface class projection" },
            RawAccessAllowance { path: "src/check/checker/namespace_values.rs", snippet: ".expect(\"class publication precedes namespace finalization\").published_class(*class)", count: 1, reason: "instrumented staged class delegate" },
            RawAccessAllowance { path: "src/check/checker/namespace_values.rs", snippet: ".binder.resolve_value(self.scope, identifier.name.as_str())", count: 1, reason: "root self-storage census" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "type_decl_id(binder, scope, \"Array\")", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "type_decl_id(binder, scope, name)", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "binder.resolve_type(scope, name)", count: 2, reason: "topology ambiguity check and helper sink" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "binder.resolve_value(scope, name)", count: 1, reason: "topology ambiguity check" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "binder.resolve_qualified_type_path(scope, segments)", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "src/check/checker/decls/mod.rs", snippet: "fn type_decl_id(", count: 1, reason: "raw helper definition" },
            RawAccessAllowance { path: "src/check/checker/library_compiler.rs", snippet: "parts.runtime.class_parents.iter()", count: 2, reason: "snapshot structural validation" },
            RawAccessAllowance { path: "src/check/checker/replay_index.rs", snippet: "self.published.published_class(class)", count: 1, reason: "instrumented replay class delegate" },
            RawAccessAllowance { path: "src/check/query/mod.rs", snippet: "self.published.published_class(source.class)", count: 1, reason: "relation query class projection" },
            RawAccessAllowance { path: "src/check/query/mod.rs", snippet: "self.published.published_class(instance.class)", count: 1, reason: "query application class projection" },
            RawAccessAllowance { path: "src/check/query/mod.rs", snippet: "published.published_class(instance.class)", count: 1, reason: "query publication frontier" },
        ];
        validate_raw_access_manifest(&sources, EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS, &allowed)
            .unwrap();
    }

    #[test]
    fn semantic_replay_access_guard_rejects_an_injected_bypass() {
        let sources = std::collections::BTreeMap::from([(
            "src/check/checker/injected.rs".to_owned(),
            "fn bypass() { binder.resolve_type(scope, name); }".to_owned(),
        )]);
        let error = validate_raw_access_manifest(&sources, &["src/check/checker/injected.rs"], &[])
            .unwrap_err();
        assert!(error.contains("unapproved raw replay access"));
    }

    #[test]
    fn semantic_replay_access_guard_preserves_code_after_a_cfg_test_field() {
        let sources = std::collections::BTreeMap::from([(
            "src/check/checker/injected.rs".to_owned(),
            "
                struct Probe {
                    #[cfg(test)]
                    test_only: (u8, [u8; 2]),
                    production: u8,
                }
                fn bypass() { binder.resolve_type(scope, name); }
            "
            .to_owned(),
        )]);
        let error = validate_raw_access_manifest(&sources, &["src/check/checker/injected.rs"], &[])
            .unwrap_err();
        assert!(error.contains("unapproved raw replay access"));
    }

    #[test]
    fn semantic_replay_access_guard_ignores_cfg_markers_in_non_code() {
        let fixtures = [
            "// #[cfg(test)]\nfn bypass() { binder.resolve_type(scope, name); }",
            "const MARKER: &str = \"#[cfg(test)]\";\nfn bypass() { binder.resolve_type(scope, name); }",
            "const MARKER: &str = r###\"#[cfg(test)]\"###;\nfn bypass() { binder.resolve_type(scope, name); }",
        ];
        for source in fixtures {
            let sources = std::collections::BTreeMap::from([(
                "src/check/checker/injected.rs".to_owned(),
                source.to_owned(),
            )]);
            let error =
                validate_raw_access_manifest(&sources, &["src/check/checker/injected.rs"], &[])
                    .unwrap_err();
            assert!(error.contains("unapproved raw replay access"));
        }
    }

    #[test]
    fn cfg_test_stripping_is_conservative_for_variants_and_initializers() {
        let source = "
            enum Probe {
                #[cfg(test)]
                TestOnly(u8, [u8; 2]),
                Production,
            }
            const PROBE: Config = Config {
                #[cfg(test)]
                test_only: make(1 < 2, (1, [2, 3]), Config { value: 4 }),
                production: 5,
            };
            fn bypass() { binder.resolve_type(scope, name); }
        ";
        let production = production_source(source);
        assert!(production.contains("TestOnly"));
        assert!(production.contains("test_only"));
        assert!(production.contains("Production"));
        assert!(production.contains("production: 5"));
        assert!(production.contains("binder.resolve_type(scope, name)"));
    }

    #[test]
    fn cfg_test_stripping_removes_a_generic_item_without_hiding_following_code() {
        let source = "
            #[cfg(test)]
            fn helper<'a, T, U>() {
                binder.resolve_type(scope, name);
            }
            fn bypass() { binder.resolve_value(scope, name); }
        ";
        let production = production_source(source);
        assert!(!production.contains("helper"));
        assert!(!production.contains("binder.resolve_type(scope, name)"));
        assert!(production.contains("binder.resolve_value(scope, name)"));
    }

    #[test]
    fn production_source_discovery_recurses_into_query_submodules() {
        let unique = format!(
            "typokat-replay-source-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        std::fs::create_dir_all(root.join("src/check/checker")).expect("create checker fixture");
        std::fs::create_dir_all(root.join("src/check/query/nested")).expect("create query fixture");
        std::fs::write(root.join("src/check/checker/known.rs"), "fn known() {}")
            .expect("write checker fixture");
        std::fs::write(root.join("src/check/query/mod.rs"), "mod nested;")
            .expect("write query root fixture");
        std::fs::write(
            root.join("src/check/query/nested/new.rs"),
            "fn discovered() {}",
        )
        .expect("write nested query fixture");
        std::fs::write(root.join("src/check/query/tests.rs"), "fn test_only() {}")
            .expect("write excluded query fixture");

        let sources = discover_production_rust_sources(&root);
        assert!(sources.contains_key("src/check/query/nested/new.rs"));
        assert!(!sources.contains_key("src/check/query/tests.rs"));
        let error = validate_production_source_set(
            &sources,
            &["src/check/checker/known.rs", "src/check/query/mod.rs"],
        )
        .unwrap_err();
        assert!(error.contains("unexpected=[\"src/check/query/nested/new.rs\"]"));
    }

    #[test]
    fn semantic_replay_access_guard_rejects_an_unmanifested_production_file() {
        let sources = std::collections::BTreeMap::from([
            (
                "src/check/checker/known.rs".to_owned(),
                "fn safe() {}".to_owned(),
            ),
            (
                "src/check/checker/new_module.rs".to_owned(),
                "fn also_safe() {}".to_owned(),
            ),
        ]);
        let error = validate_raw_access_manifest(&sources, &["src/check/checker/known.rs"], &[])
            .unwrap_err();
        assert!(error.contains("unexpected=[\"src/check/checker/new_module.rs\"]"));
    }

    #[test]
    fn semantic_replay_access_guard_rejects_a_missing_production_file() {
        let sources = std::collections::BTreeMap::from([(
            "src/check/checker/known.rs".to_owned(),
            "fn safe() {}".to_owned(),
        )]);
        let error = validate_raw_access_manifest(
            &sources,
            &["src/check/checker/known.rs", "src/check/checker/missing.rs"],
            &[],
        )
        .unwrap_err();
        assert!(error.contains("missing=[\"src/check/checker/missing.rs\"]"));
    }
}
