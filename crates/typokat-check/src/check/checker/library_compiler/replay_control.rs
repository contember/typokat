use super::*;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PRIVATE_REPLAY_SCHEDULER_WORK: std::cell::RefCell<Option<BTreeMap<&'static str, u64>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(any(test, feature = "test-utils"))]
const PRIVATE_REPLAY_SCHEDULER_FAMILIES: [&str; 10] = [
    "seed-pushes",
    "seed-pops",
    "scc-queue-pushes",
    "scc-queue-pops",
    "edge-probes",
    "owner-set-probes",
    "owner-set-inserts",
    "sort-items",
    "dedup-items",
    "replay-allocations",
];

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_private_replay_scheduler_work_for_test(family: &'static str, count: usize) {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    PRIVATE_REPLAY_SCHEDULER_WORK.with_borrow_mut(|active| {
        let Some(work) = active.as_mut() else {
            return;
        };
        let counter = work.entry(family).or_default();
        *counter = counter.saturating_add(count);
    });
}

#[cfg(not(any(test, feature = "test-utils")))]
pub(super) fn record_private_replay_scheduler_work_for_test(_family: &'static str, _count: usize) {}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateReplaySchedulerWorkScopeForTest;

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplaySchedulerWorkScopeForTest {
    pub fn start() -> Self {
        PRIVATE_REPLAY_SCHEDULER_WORK.with_borrow_mut(|active| {
            assert!(
                active.is_none(),
                "private replay scheduler scopes do not nest"
            );
            *active = Some(
                PRIVATE_REPLAY_SCHEDULER_FAMILIES
                    .into_iter()
                    .map(|family| (family, 0))
                    .collect(),
            );
        });
        Self
    }

    pub fn finish(self) -> BTreeMap<&'static str, u64> {
        PRIVATE_REPLAY_SCHEDULER_WORK
            .with_borrow_mut(Option::take)
            .expect("private replay scheduler scope is active")
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn calibrate_private_replay_scheduler_work_for_test() -> BTreeMap<&'static str, u64> {
    let scope = PrivateReplaySchedulerWorkScopeForTest::start();
    let mut seed = Vec::new();
    seed.push(ReplayOwner::GlobalObject);
    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
    let _ = seed.pop();
    record_private_replay_scheduler_work_for_test("seed-pops", 1);
    let mut queue = Vec::new();
    queue.push(ReplayOwner::GlobalObject);
    record_private_replay_scheduler_work_for_test("scc-queue-pushes", 1);
    let _ = queue.pop();
    record_private_replay_scheduler_work_for_test("scc-queue-pops", 1);
    let edge = [(ReplayOwner::GlobalObject, ReplayOwner::GlobalObject)];
    let _ = edge.first();
    record_private_replay_scheduler_work_for_test("edge-probes", 1);
    let mut owners = BTreeSet::new();
    let _ = owners.contains(&ReplayOwner::GlobalObject);
    record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
    let _ = owners.insert(ReplayOwner::GlobalObject);
    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
    let mut sortable = [1_u8];
    sortable.sort_unstable();
    record_private_replay_scheduler_work_for_test("sort-items", sortable.len());
    let mut deduplicated = vec![1_u8];
    deduplicated.dedup();
    record_private_replay_scheduler_work_for_test("dedup-items", deduplicated.len());
    let allocation = std::iter::once(1_u8).collect::<Vec<_>>();
    record_private_replay_scheduler_work_for_test("replay-allocations", allocation.len());
    scope.finish()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CollisionPlanForbiddenWork {
    pub(super) library_source_compiles: u64,
    pub(super) second_source_censuses: u64,
    pub(super) canonical_manifest_bytes: u64,
    pub(super) rendered_record_digest_bytes: u64,
    pub(super) transitive_terminal_owner_entries: u64,
    pub(super) eager_all_owner_scc_memberships: u64,
    pub(super) namespace_snapshot_rows: u64,
    pub(super) runtime_snapshot_rows: u64,
    pub(super) canonical_terminal_rows: u64,
    pub(super) full_semantic_projection_rows: u64,
}

thread_local! {
    static COLLISION_PLAN_FORBIDDEN_WORK: Cell<CollisionPlanForbiddenWork> =
        Cell::new(CollisionPlanForbiddenWork::default());
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) static FORCED_COLLISION_PLAN_FAILURE: Cell<Option<ForcedCollisionPlanFailure>> =
        const { Cell::new(None) };
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) static FULL_COLLISION_PLAN_ORACLE: RefCell<Option<FullCollisionPlanOracleForTest>> =
        const { RefCell::new(None) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullCollisionPlanOracleForTest {
    pub owner_sites: Vec<CollisionReplayOwnerSite>,
    pub baseline_records: Vec<super::replay_index::ReplayBaselineRecord>,
    pub reverse_edges: Vec<super::replay_index::ReplayReverseEdge>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy)]
struct IndependentEventMeta {
    file_ordinal: LibraryFileOrdinal,
    source_start: u32,
    event_ordinal: usize,
    next_record_ordinal: usize,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct IndependentEventOwnerSiteOracle {
    file_ordinal: LibraryFileOrdinal,
    events: Vec<IndependentEventMeta>,
    next_event_ordinal: BTreeMap<LibraryFileOrdinal, usize>,
    ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>,
    owner_sites: Vec<CollisionReplayOwnerSite>,
    invalid_ticket: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl IndependentEventOwnerSiteOracle {
    pub(super) fn new() -> Self {
        Self {
            file_ordinal: LibraryFileOrdinal::new(0),
            events: Vec::new(),
            next_event_ordinal: BTreeMap::new(),
            ticket_owners: BTreeMap::new(),
            owner_sites: Vec::new(),
            invalid_ticket: false,
        }
    }

    pub(super) fn select_file(&mut self, file_ordinal: LibraryFileOrdinal) {
        self.file_ordinal = file_ordinal;
    }

    pub(super) fn reserve_exact_site(&mut self, span: Span, phase: CollisionReplayEventPhase) {
        let (_, ticket) = self.reserve_event(span.start);
        self.record_owner_site(ticket, span, phase);
    }

    pub(super) fn finish(mut self) -> Result<Vec<CollisionReplayOwnerSite>, InjectedProfileError> {
        if self.invalid_ticket {
            return Err(InjectedProfileError::CanonicalProjection(
                "independent lexical event oracle lost a reservation ticket".to_owned(),
            ));
        }
        canonicalize_collision_replay_owner_sites(&mut self.owner_sites);
        Ok(self.owner_sites)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl LexicalReservationAllocator for IndependentEventOwnerSiteOracle {
    type Event = usize;
    type Ticket = (usize, usize);
    type Error = &'static str;

    fn source_unit(&self) -> SourceUnit {
        SourceUnit::Library {
            file_ordinal: self.file_ordinal,
        }
    }

    fn reserve_event(&mut self, source_start: u32) -> (Self::Event, Self::Ticket) {
        let event_ordinal = self
            .next_event_ordinal
            .entry(self.file_ordinal)
            .or_insert(0);
        let event = self.events.len();
        let key = LibraryEventKey {
            file_ordinal: self.file_ordinal,
            source_start,
            event_ordinal: *event_ordinal,
            record_ordinal: 0,
        };
        *event_ordinal = event_ordinal.saturating_add(1);
        self.events.push(IndependentEventMeta {
            file_ordinal: self.file_ordinal,
            source_start,
            event_ordinal: key.event_ordinal,
            next_record_ordinal: 1,
        });
        self.ticket_owners.insert((event, 0), key);
        (event, (event, 0))
    }

    fn reserve_record(&mut self, event: Self::Event) -> Result<Self::Ticket, Self::Error> {
        let Some(meta) = self.events.get_mut(event) else {
            return Err("independent lexical event oracle saw an unknown event");
        };
        let record = meta.next_record_ordinal;
        meta.next_record_ordinal = meta.next_record_ordinal.saturating_add(1);
        self.ticket_owners.insert(
            (event, record),
            LibraryEventKey {
                file_ordinal: meta.file_ordinal,
                source_start: meta.source_start,
                event_ordinal: meta.event_ordinal,
                record_ordinal: record,
            },
        );
        Ok((event, record))
    }

    fn record_owner_site(
        &mut self,
        ticket: Self::Ticket,
        span: Span,
        phase: CollisionReplayEventPhase,
    ) {
        let Some(key) = self.ticket_owners.get(&ticket).copied() else {
            self.invalid_ticket = true;
            return;
        };
        self.owner_sites.push(CollisionReplayOwnerSite {
            owner: ReplayOwner::Statement(key),
            file_ordinal: key.file_ordinal,
            span,
            provenance: CollisionReplaySiteProvenance::Event { phase },
        });
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn take_full_collision_plan_oracle_for_test() -> Option<FullCollisionPlanOracleForTest> {
    FULL_COLLISION_PLAN_ORACLE.with(|oracle| oracle.borrow_mut().take())
}

#[cfg(any(test, feature = "test-utils"))]
pub fn forbidden_projection_callsite_audit_for_test() -> bool {
    let compiler = concat!(
        include_str!("../library_compiler.rs"),
        include_str!("replay_control.rs"),
        include_str!("runtime_model.rs"),
        include_str!("runtime_state.rs"),
        include_str!("runtime_witness.rs"),
        include_str!("profile.rs"),
        include_str!("canonical_bytes.rs"),
        include_str!("terminal_validation.rs"),
        include_str!("collision_plan.rs"),
        include_str!("complete_source.rs"),
        include_str!("compile_api.rs"),
        include_str!("compile_engine.rs"),
        include_str!("continuation.rs"),
        include_str!("probes.rs"),
        include_str!("tests.rs"),
    );
    let replay = include_str!("../replay_index.rs");
    [
        (compiler, "second_source_censuses =", 2),
        (compiler, "canonical_record_bytes(", 4),
        (compiler, "validate_terminal_class_dependencies(", 4),
        (compiler, "snapshot_namespace_terminals_for_replay(", 4),
        (compiler, ".snapshot_parts()", 6),
        (compiler, "canonical_terminals()", 3),
        (compiler, "full_semantic_projection_rows =", 2),
        (replay, "record_collision_manifest_bytes(", 1),
    ]
    .into_iter()
    .all(|(source, token, expected)| source.matches(token).count() == expected)
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForcedCollisionPlanFailure {
    UnownedTypedReference,
    RawSemanticAccess,
    ForbiddenProjection,
    EventCaptureCorruption,
    LateOwnerReservation,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct ForcedCollisionPlanFailureScope;

#[cfg(any(test, feature = "test-utils"))]
impl Drop for ForcedCollisionPlanFailureScope {
    fn drop(&mut self) {
        FORCED_COLLISION_PLAN_FAILURE.set(None);
    }
}

pub(super) struct CollisionPlanForbiddenWorkScope(CollisionPlanForbiddenWork);

impl CollisionPlanForbiddenWorkScope {
    pub(super) fn start() -> Self {
        Self(COLLISION_PLAN_FORBIDDEN_WORK.get())
    }

    pub(super) fn finish(self) -> CollisionPlanForbiddenWork {
        let after = COLLISION_PLAN_FORBIDDEN_WORK.get();
        CollisionPlanForbiddenWork {
            library_source_compiles: after
                .library_source_compiles
                .saturating_sub(self.0.library_source_compiles),
            second_source_censuses: after
                .second_source_censuses
                .saturating_sub(self.0.second_source_censuses),
            canonical_manifest_bytes: after
                .canonical_manifest_bytes
                .saturating_sub(self.0.canonical_manifest_bytes),
            rendered_record_digest_bytes: after
                .rendered_record_digest_bytes
                .saturating_sub(self.0.rendered_record_digest_bytes),
            transitive_terminal_owner_entries: after
                .transitive_terminal_owner_entries
                .saturating_sub(self.0.transitive_terminal_owner_entries),
            eager_all_owner_scc_memberships: after
                .eager_all_owner_scc_memberships
                .saturating_sub(self.0.eager_all_owner_scc_memberships),
            namespace_snapshot_rows: after
                .namespace_snapshot_rows
                .saturating_sub(self.0.namespace_snapshot_rows),
            runtime_snapshot_rows: after
                .runtime_snapshot_rows
                .saturating_sub(self.0.runtime_snapshot_rows),
            canonical_terminal_rows: after
                .canonical_terminal_rows
                .saturating_sub(self.0.canonical_terminal_rows),
            full_semantic_projection_rows: after
                .full_semantic_projection_rows
                .saturating_sub(self.0.full_semantic_projection_rows),
        }
    }
}

#[rustfmt::skip]
pub(super) fn record_collision_plan_forbidden_work(update: impl FnOnce(&mut CollisionPlanForbiddenWork)) {
    COLLISION_PLAN_FORBIDDEN_WORK.set({
        let mut work = COLLISION_PLAN_FORBIDDEN_WORK.get();
        update(&mut work);
        work
    });
}

pub(in crate::check::checker) fn record_collision_manifest_bytes(bytes: usize) {
    record_collision_plan_forbidden_work(|work| {
        work.canonical_manifest_bytes = work
            .canonical_manifest_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
    });
}

pub(super) fn snapshot_namespace_terminals_for_replay(
    terminals: &FrozenNamespaceValueTerminals,
) -> Result<Vec<FrozenNamespaceValueTerminalSnapshotRow>, InjectedProfileError> {
    let rows = terminals
        .snapshot_parts()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    record_collision_plan_forbidden_work(|work| {
        work.namespace_snapshot_rows = work
            .namespace_snapshot_rows
            .saturating_add(u64::try_from(rows.len()).unwrap_or(u64::MAX));
    });
    Ok(rows)
}

/// Sealed authority for one collision-preflighted frozen-prefix fork.
pub struct CollisionFreeUserDeltaCapability(());

impl CollisionFreeUserDeltaCapability {
    pub fn issue() -> Self {
        Self(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCollisionReplaySeed {
    pub name: String,
    pub value: bool,
    pub ty: bool,
    pub namespace: bool,
    pub global_object: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCollisionReplaySource {
    pub file_ordinal: LibraryFileOrdinal,
    pub name: String,
    pub source: String,
}

#[derive(Clone, Default)]
pub struct PrivateCollisionReplaySourceRegistry {
    sources: std::sync::Arc<[PrivateCollisionReplaySource]>,
}

impl std::fmt::Debug for PrivateCollisionReplaySourceRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivateCollisionReplaySourceRegistry")
            .field("source_count", &self.sources.len())
            .finish()
    }
}

impl PrivateCollisionReplaySourceRegistry {
    pub fn from_sources(sources: Vec<PrivateCollisionReplaySource>) -> Self {
        Self {
            sources: sources.into(),
        }
    }

    pub fn get(
        &self,
        ordinal: LibraryFileOrdinal,
    ) -> Result<PrivateCollisionReplaySource, &'static str> {
        #[cfg(any(test, feature = "test-utils"))]
        if inject_checker_full_source_registry_scan_for_test() {
            self.measured_full_scan_for_test();
        }
        self.sources
            .get(ordinal.index())
            .filter(|source| source.file_ordinal == ordinal)
            .cloned()
            .ok_or("private replay source registry is not canonical")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn measured_full_scan_for_test(&self) -> usize {
        let rows = self.sources.iter().count();
        record_private_replay_full_base_scan_for_test(rows);
        rows
    }
}

#[derive(Debug)]
pub(in crate::check::checker) struct PrivateCollisionEpoch {
    pub(in crate::check::checker) affected_owners: BTreeSet<ReplayOwner>,
    pub(in crate::check::checker) mutation_owners: BTreeSet<ReplayOwner>,
    pub(in crate::check::checker) value_winners: BTreeMap<String, ValueStorageId>,
    pub(in crate::check::checker) owner_sites: Vec<super::replay_index::CollisionReplayOwnerSite>,
    pub(in crate::check::checker) library_record_baselines:
        Vec<super::replay_index::ReplayBaselineRecord>,
    pub(in crate::check::checker) sources: Vec<PrivateCollisionReplaySource>,
    pub(in crate::check::checker) complete_source_replay: bool,
    pub(in crate::check::checker) plan: std::sync::Arc<CollisionReplayPlan>,
    pub(super) _permit: PrivateCollisionReplayPermitToken,
    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) state_drop_witness:
        Option<PrivateCollisionStateDropWitnessForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub(in crate::check::checker) struct PrivateCollisionStateDropWitnessForTest;

#[cfg(any(test, feature = "test-utils"))]
impl Drop for PrivateCollisionStateDropWitnessForTest {
    fn drop(&mut self) {
        record_private_replay_scale_hook_for_test(|trace, event| {
            trace.private_state_dropped = event;
        });
    }
}

impl PrivateCollisionEpoch {
    fn mutation_owner_requires_authentication(
        plan: &CollisionReplayPlan,
        owner: ReplayOwner,
    ) -> bool {
        let (boundary, id) = match owner {
            ReplayOwner::TypeGroup(id) => (6, id.0),
            ReplayOwner::Namespace(id) => (7, id.0),
            ReplayOwner::Value(id) => (8, id.0),
            ReplayOwner::Class(_) | ReplayOwner::GlobalObject | ReplayOwner::Statement(_) => {
                return true;
            }
        };
        usize::try_from(id).is_ok_and(|id| {
            plan.prefix_cardinality(boundary)
                .is_some_and(|boundary| id < boundary)
        })
    }

    fn contains_mutation_owner(plan: &CollisionReplayPlan, owner: ReplayOwner) -> bool {
        plan.contains_mutation_owner(owner)
    }

    fn mutation_owner_exists(owner: ReplayOwner, owner_ends: [usize; 3]) -> bool {
        let (end, id) = match owner {
            ReplayOwner::TypeGroup(id) => (owner_ends[0], id.0),
            ReplayOwner::Value(id) => (owner_ends[1], id.0),
            ReplayOwner::Namespace(id) => (owner_ends[2], id.0),
            ReplayOwner::Class(_) | ReplayOwner::GlobalObject | ReplayOwner::Statement(_) => {
                return false;
            }
        };
        usize::try_from(id).is_ok_and(|id| id < end)
    }

    fn close_owners(plan: &CollisionReplayPlan, owners: &mut BTreeSet<ReplayOwner>) {
        let mut pending = owners.iter().copied().collect::<Vec<_>>();
        let mut cursor = 0;
        while let Some(owner) = pending.get(cursor).copied() {
            cursor += 1;
            for edge in plan.reverse_consumers(owner) {
                if owners.insert(edge.consumer) {
                    pending.push(edge.consumer);
                }
            }
        }
    }

    pub(super) fn owner_sites_for(
        plan: &CollisionReplayPlan,
        owners: &BTreeSet<ReplayOwner>,
    ) -> Vec<super::replay_index::CollisionReplayOwnerSite> {
        let mut sites = Vec::new();
        for owner in owners {
            sites.extend_from_slice(plan.owner_sites_for(*owner));
        }
        sites
    }

    pub(super) fn baselines_for(
        plan: &CollisionReplayPlan,
        owners: &BTreeSet<ReplayOwner>,
    ) -> Vec<super::replay_index::ReplayBaselineRecord> {
        let mut baselines = Vec::new();
        for owner in owners {
            baselines.extend_from_slice(plan.baselines_for(*owner));
        }
        baselines
    }

    pub(in crate::check::checker) fn reclose_after_binding(
        &mut self,
        mutation_owners: BTreeSet<ReplayOwner>,
        owner_ends: [usize; 3],
        loaded_sources: &BTreeSet<LibraryFileOrdinal>,
    ) -> Result<(), PrivateReplayScheduleFailure> {
        self.mutation_owners = mutation_owners;
        if let Some(owner) = self.mutation_owners.iter().find(|owner| {
            !Self::mutation_owner_exists(**owner, owner_ends)
                || (Self::mutation_owner_requires_authentication(&self.plan, **owner)
                    && !Self::contains_mutation_owner(&self.plan, **owner))
        }) {
            return Err(PrivateReplayScheduleFailure::MutationOwnerOutsidePlan(
                *owner,
            ));
        }

        let mut affected = self.affected_owners.clone();
        affected.extend(self.mutation_owners.iter().copied());
        Self::close_owners(&self.plan, &mut affected);
        let owner_sites = Self::owner_sites_for(&self.plan, &affected);
        let required_sources = owner_sites
            .iter()
            .filter(|site| !matches!(site.owner, ReplayOwner::GlobalObject))
            .map(|site| site.file_ordinal)
            .collect::<BTreeSet<_>>();
        if let Some(source) = required_sources
            .iter()
            .find(|source| !loaded_sources.contains(source))
        {
            return Err(PrivateReplayScheduleFailure::RequiredSourceNotLoaded(
                *source,
            ));
        }

        self.value_winners
            .retain(|_, value| affected.contains(&ReplayOwner::Value(*value)));
        self.library_record_baselines = Self::baselines_for(&self.plan, &affected);
        self.owner_sites = owner_sites;
        self.affected_owners = affected;
        #[cfg(test)]
        if let Some(selected) = explicit_sparse_selection_for_test() {
            self.value_winners
                .retain(|_, value| selected.contains(&ReplayOwner::Value(*value)));
            self.library_record_baselines = Self::baselines_for(&self.plan, &selected);
            self.owner_sites = Self::owner_sites_for(&self.plan, &selected);
            self.affected_owners = selected;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PrivateReplayScheduleFailure {
    MutationOwnerOutsidePlan(ReplayOwner),
    RequiredSourceNotLoaded(LibraryFileOrdinal),
}

#[derive(Debug)]
struct PrivateCollisionReplayPermit {
    occupied: Mutex<bool>,
    available: Condvar,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
struct PrivateReplayScaleSharedForTest {
    contenders: std::sync::atomic::AtomicUsize,
    max_contenders: std::sync::atomic::AtomicUsize,
    active: std::sync::atomic::AtomicUsize,
    peak_active: std::sync::atomic::AtomicUsize,
    acquisitions: std::sync::atomic::AtomicUsize,
    hook_invocations: std::sync::atomic::AtomicU64,
}

#[cfg(any(test, feature = "test-utils"))]
fn update_scale_max_for_test(target: &std::sync::atomic::AtomicUsize, observed: usize) {
    use std::sync::atomic::Ordering;

    let mut current = target.load(Ordering::SeqCst);
    while observed > current {
        match target.compare_exchange(current, observed, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrivateReplayScaleRouteTraceForTest {
    pub production_route_invocations: u64,
    pub production_work_hook_invocations: u64,
    pub sparse_replay_invocations: u64,
    pub full_source_fallback_invocations: u64,
    pub sparse_library_source_units: u64,
    pub full_base_scan_units: u64,
    pub production_visibility_query_invocations: u64,
    pub epoch_id: Option<usize>,
    pub permit_acquired: u64,
    pub private_work_started: u64,
    pub private_state_dropped: u64,
    pub permit_released: u64,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct PrivateReplayScaleControlForTest {
    shared: std::sync::Arc<PrivateReplayScaleSharedForTest>,
    suppress_permit_instrumentation: bool,
    inject_checker_full_base_scan: bool,
    inject_checker_full_plan_scan: bool,
    inject_checker_full_source_registry_scan: bool,
    pub(super) trace: PrivateReplayScaleRouteTraceForTest,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PRIVATE_REPLAY_SCALE_CONTROL: std::cell::RefCell<Option<PrivateReplayScaleControlForTest>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static EXPLICIT_SPARSE_SELECTION_FOR_TEST:
        std::cell::RefCell<Option<BTreeSet<ReplayOwner>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn explicit_sparse_selection_for_test() -> Option<BTreeSet<ReplayOwner>> {
    EXPLICIT_SPARSE_SELECTION_FOR_TEST.with_borrow(Clone::clone)
}

#[cfg(test)]
pub(super) struct ExplicitSparseSelectionScopeForTest;

#[cfg(test)]
impl ExplicitSparseSelectionScopeForTest {
    pub(super) fn start(selected: BTreeSet<ReplayOwner>) -> Self {
        EXPLICIT_SPARSE_SELECTION_FOR_TEST.with_borrow_mut(|active| {
            assert!(
                active.is_none(),
                "explicit sparse selection scopes do not nest"
            );
            *active = Some(selected);
        });
        Self
    }
}

#[cfg(test)]
impl Drop for ExplicitSparseSelectionScopeForTest {
    fn drop(&mut self) {
        EXPLICIT_SPARSE_SELECTION_FOR_TEST.with_borrow_mut(|active| {
            active.take();
        });
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug)]
pub struct PrivateReplayScaleRunForTest {
    shared: std::sync::Arc<PrivateReplayScaleSharedForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplayScaleRunForTest {
    pub fn start() -> Self {
        Self {
            shared: std::sync::Arc::new(PrivateReplayScaleSharedForTest::default()),
        }
    }

    pub fn acquisitions(&self) -> usize {
        self.shared
            .acquisitions
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn hook_invocations(&self) -> u64 {
        self.shared
            .hook_invocations
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn max_contenders(&self) -> usize {
        self.shared
            .max_contenders
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn peak_active(&self) -> usize {
        self.shared
            .peak_active
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateReplayScaleRouteScopeForTest;

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplayScaleRouteScopeForTest {
    pub fn start(
        run: &PrivateReplayScaleRunForTest,
        suppress_permit_instrumentation: bool,
        inject_checker_full_base_scan: bool,
        inject_checker_full_plan_scan: bool,
        inject_checker_full_source_registry_scan: bool,
    ) -> Result<Self, &'static str> {
        PRIVATE_REPLAY_SCALE_CONTROL.with_borrow_mut(|active| {
            if active.is_some() {
                return Err("private replay scale route scopes do not nest");
            }
            *active = Some(PrivateReplayScaleControlForTest {
                shared: std::sync::Arc::clone(&run.shared),
                suppress_permit_instrumentation,
                inject_checker_full_base_scan,
                inject_checker_full_plan_scan,
                inject_checker_full_source_registry_scan,
                trace: PrivateReplayScaleRouteTraceForTest::default(),
            });
            Ok(Self)
        })
    }

    pub fn finish(self) -> Result<PrivateReplayScaleRouteTraceForTest, &'static str> {
        PRIVATE_REPLAY_SCALE_CONTROL.with_borrow_mut(|active| {
            active
                .take()
                .map(|control| control.trace)
                .ok_or("private replay scale route scope is not active")
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn inject_checker_full_base_scan_for_test() -> bool {
    PRIVATE_REPLAY_SCALE_CONTROL.with_borrow(|active| {
        active
            .as_ref()
            .is_some_and(|active| active.inject_checker_full_base_scan)
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn inject_checker_full_plan_scan_for_test() -> bool {
    PRIVATE_REPLAY_SCALE_CONTROL.with_borrow(|active| {
        active
            .as_ref()
            .is_some_and(|active| active.inject_checker_full_plan_scan)
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn inject_checker_full_source_registry_scan_for_test() -> bool {
    PRIVATE_REPLAY_SCALE_CONTROL.with_borrow(|active| {
        active
            .as_ref()
            .is_some_and(|active| active.inject_checker_full_source_registry_scan)
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn with_private_replay_scale_control_for_test(
    update: impl FnOnce(&mut PrivateReplayScaleControlForTest),
) {
    PRIVATE_REPLAY_SCALE_CONTROL.with_borrow_mut(|active| {
        if let Some(active) = active.as_mut() {
            update(active);
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_private_replay_scale_hook_for_test(
    update: impl FnOnce(&mut PrivateReplayScaleRouteTraceForTest, u64),
) {
    with_private_replay_scale_control_for_test(|active| {
        if active.suppress_permit_instrumentation {
            return;
        }
        let event = next_private_replay_lifecycle_event_for_test();
        active.trace.production_work_hook_invocations = active
            .trace
            .production_work_hook_invocations
            .saturating_add(1);
        update(&mut active.trace, event);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_private_replay_route_invocation_for_test() {
    with_private_replay_scale_control_for_test(|active| {
        active.trace.production_route_invocations =
            active.trace.production_route_invocations.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_private_replay_fallback_invocation_for_test() {
    with_private_replay_scale_control_for_test(|active| {
        active.trace.full_source_fallback_invocations = active
            .trace
            .full_source_fallback_invocations
            .saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_private_replay_visibility_query_for_test() {
    with_private_replay_scale_control_for_test(|active| {
        active.trace.production_visibility_query_invocations = active
            .trace
            .production_visibility_query_invocations
            .saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_private_replay_full_base_scan_for_test(units: usize) {
    with_private_replay_scale_control_for_test(|active| {
        active.trace.full_base_scan_units = active
            .trace
            .full_base_scan_units
            .saturating_add(u64::try_from(units).unwrap_or(u64::MAX));
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn private_replay_scale_contender_entered_for_test() {
    with_private_replay_scale_control_for_test(|active| {
        let contenders = active
            .shared
            .contenders
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .saturating_add(1);
        update_scale_max_for_test(&active.shared.max_contenders, contenders);
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn private_replay_scale_permit_acquired_for_test(epoch_id: usize, acquired_event: u64) {
    with_private_replay_scale_control_for_test(|active| {
        active
            .shared
            .contenders
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        if active.suppress_permit_instrumentation {
            return;
        }
        active
            .shared
            .acquisitions
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let active_count = active
            .shared
            .active
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .saturating_add(1);
        update_scale_max_for_test(&active.shared.peak_active, active_count);
        active
            .shared
            .hook_invocations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        active.trace.production_work_hook_invocations = active
            .trace
            .production_work_hook_invocations
            .saturating_add(1);
        active.trace.epoch_id = Some(epoch_id);
        active.trace.permit_acquired = acquired_event;
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn private_replay_scale_permit_released_for_test(released_event: u64) {
    with_private_replay_scale_control_for_test(|active| {
        if active.suppress_permit_instrumentation {
            return;
        }
        active
            .shared
            .active
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        active
            .shared
            .hook_invocations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        active.trace.production_work_hook_invocations = active
            .trace
            .production_work_hook_invocations
            .saturating_add(1);
        active.trace.permit_released = released_event;
    });
}

impl PrivateCollisionReplayPermit {
    const fn new() -> Self {
        Self {
            occupied: Mutex::new(false),
            available: Condvar::new(),
        }
    }

    fn acquire(&'static self) -> Result<PrivateCollisionReplayPermitToken, &'static str> {
        #[cfg(any(test, feature = "test-utils"))]
        private_replay_scale_contender_entered_for_test();
        let mut occupied = self
            .occupied
            .lock()
            .map_err(|_| "private collision replay permit is poisoned")?;
        while *occupied {
            occupied = self
                .available
                .wait(occupied)
                .map_err(|_| "private collision replay permit is poisoned")?;
        }
        *occupied = true;
        #[cfg(any(test, feature = "test-utils"))]
        PRIVATE_COLLISION_REPLAY_THREAD_ACQUISITIONS.set(
            PRIVATE_COLLISION_REPLAY_THREAD_ACQUISITIONS
                .get()
                .saturating_add(1),
        );
        #[cfg(any(test, feature = "test-utils"))]
        let epoch_id = PRIVATE_COLLISION_REPLAY_EPOCHS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1);
        #[cfg(any(test, feature = "test-utils"))]
        let acquired_event = next_private_replay_lifecycle_event_for_test();
        #[cfg(any(test, feature = "test-utils"))]
        PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(any(test, feature = "test-utils"))]
        private_replay_scale_permit_acquired_for_test(epoch_id, acquired_event);
        Ok(PrivateCollisionReplayPermitToken {
            _lease: std::sync::Arc::new(PrivateCollisionReplayPermitLease {
                permit: self,
                #[cfg(any(test, feature = "test-utils"))]
                acquired_event,
            }),
        })
    }
}

#[derive(Clone, Debug)]
pub struct PrivateCollisionReplayPermitToken {
    _lease: std::sync::Arc<PrivateCollisionReplayPermitLease>,
}

#[derive(Debug)]
struct PrivateCollisionReplayPermitLease {
    permit: &'static PrivateCollisionReplayPermit,
    #[cfg(any(test, feature = "test-utils"))]
    acquired_event: u64,
}

impl Drop for PrivateCollisionReplayPermitLease {
    fn drop(&mut self) {
        #[cfg(any(test, feature = "test-utils"))]
        {
            let event = next_private_replay_lifecycle_event_for_test();
            PRIVATE_COLLISION_REPLAY_LAST_RELEASE_EVENT
                .store(event, std::sync::atomic::Ordering::Relaxed);
            PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            private_replay_scale_permit_released_for_test(event);
        }
        if let Ok(mut occupied) = self.permit.occupied.lock() {
            *occupied = false;
            self.permit.available.notify_one();
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
static PRIVATE_COLLISION_REPLAY_EPOCHS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(any(test, feature = "test-utils"))]
static PRIVATE_COLLISION_REPLAY_LIFECYCLE_EVENTS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(any(test, feature = "test-utils"))]
static PRIVATE_COLLISION_REPLAY_LAST_RELEASE_EVENT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(any(test, feature = "test-utils"))]
static PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PRIVATE_COLLISION_REPLAY_THREAD_ACQUISITIONS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub struct PrivateReplayPermitAcquisitionScopeForTest(u64);

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplayPermitAcquisitionScopeForTest {
    pub fn start() -> Self {
        Self(PRIVATE_COLLISION_REPLAY_THREAD_ACQUISITIONS.get())
    }

    pub fn finish(self) -> u64 {
        PRIVATE_COLLISION_REPLAY_THREAD_ACQUISITIONS
            .get()
            .saturating_sub(self.0)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl PrivateCollisionReplayPermitToken {
    pub fn inner_allocation_identity_for_test(&self) -> usize {
        std::sync::Arc::as_ptr(&self._lease).addr()
    }

    pub fn acquired_event_for_test(&self) -> u64 {
        self._lease.acquired_event
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn next_private_replay_lifecycle_event_for_test() -> u64 {
    PRIVATE_COLLISION_REPLAY_LIFECYCLE_EVENTS
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .saturating_add(1)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn private_replay_last_release_event_for_test() -> u64 {
    PRIVATE_COLLISION_REPLAY_LAST_RELEASE_EVENT.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn private_replay_hook_invocations_for_test() -> u64 {
    PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn private_replay_epoch_count_for_test() -> usize {
    PRIVATE_COLLISION_REPLAY_EPOCHS.load(std::sync::atomic::Ordering::Relaxed)
}

static PRIVATE_COLLISION_REPLAY_PERMIT: PrivateCollisionReplayPermit =
    PrivateCollisionReplayPermit::new();

pub fn acquire_private_collision_replay_permit(
) -> Result<PrivateCollisionReplayPermitToken, &'static str> {
    PRIVATE_COLLISION_REPLAY_PERMIT.acquire()
}
