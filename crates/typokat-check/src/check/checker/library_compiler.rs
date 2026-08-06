//! Measurement-only compiler for injected declaration-library profiles.

use super::classes::application::ClassTypeParameterDefault;
use super::context::{CertifiedLibraryValues, CheckerRecordBatch, DeclTypes, TypeDecl};
use super::events::{CandidateEffects, EventStore, UserRecordTicket};
use super::events_library::{
    library_record_ticket_key, LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError,
    LibraryRecordTicket, LibrarySemanticReportingAdapter,
};
#[cfg(any(test, feature = "test-utils"))]
use super::lexical_events::LexicalReservationAllocator;
use super::lexical_events::LexicalReservations;
use super::lexical_events_library::library_unit;
#[cfg(any(test, feature = "test-utils"))]
use super::lexical_events_library::ExactUnit;
use super::library_identities::LibraryIdentityTerminal;
use super::library_identities::{LibrarySemanticIdentities, NativeArrayGroups};
#[cfg(any(test, feature = "test-utils"))]
use super::library_reporting::independent_library_reporting_site_descriptors;
use super::library_reporting::LibraryReportingConsumer;
#[cfg(test)]
use super::library_reporting::LibraryReportingFamily;
#[cfg(any(test, feature = "test-utils"))]
use super::library_reporting::LibraryReportingReceipt;
#[cfg(any(test, feature = "test-utils"))]
use super::namespace_values::NamespaceValueRegistry;
use super::namespace_values::{
    FrozenNamespaceValueTerminalSnapshot, FrozenNamespaceValueTerminalSnapshotRow,
    FrozenNamespaceValueTerminals,
};
#[cfg(test)]
use super::replay_index::ReplayReverseEdge;
use super::replay_index::{
    admit_generated_collision_replay_index, baseline_record, AdmittedCollisionReplayIndex,
    CollisionReplayConstructionEvidence, CollisionReplayIndex, CollisionReplayOwnerSite,
    CollisionReplayPlan, OwnerSiteStorageMode, ReplayDependencyTrace, ReplayIndexAdmissionError,
    ReplayIndexAdmissionLimits, ReplayIndexGenerationError, ReplayOwner, ReplayOwnerSite,
    ReplayRootSlot,
};
#[cfg(any(test, feature = "test-utils"))]
use super::replay_index::{
    canonicalize_collision_replay_owner_sites, CollisionReplayEventPhase,
    CollisionReplaySiteProvenance,
};
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
    PublishedTypeParameterDefault,
};
use super::{
    attach_class_bindings, attach_type_decl_owners, build_pass_with_tickets,
    enqueue_ambient_context_diagnostics, enqueue_local_ambient_export_alias_diagnostics,
    enqueue_namespace_placement_diagnostics, finish_semantic_effects,
    private_combined_record_ticket_key, reserve_type_decls,
    reserve_type_decls_for_combined_library, reserve_type_decls_for_combined_user,
    FrozenCheckerRuntimeMetadata, FrozenCheckerRuntimeSnapshotParts, PassReporting,
    PassReportingPlan, PrivateCombinedRecordTicket,
};
#[cfg(any(test, feature = "test-utils"))]
use super::{check_bound_user_program_with_final_identity_inspector, BoundUserBase};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::bind::LibraryBinderCheckpointEnds;
use crate::binder::bind::{
    ImportPlaceholder, LibraryBinderCheckpoint, LibraryBinderUnit, ProjectBinderBuilder,
};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::declaration::DeclId;
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::declaration::TypeFragmentKind;
use crate::binder::declaration::{
    source_global_binding_census_with_provenance, SourceBindingSlot, SourceGlobalContributorKind,
    TypeGroupId, ValueStorageId,
};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::namespace::MergeDeclarationKind;
use crate::binder::namespace::{
    exact_key, source_file_kind, CompilationUnit, ExactKey, ExportContextKind,
    ExportSyntaxDisposition, ModuleBindingContext, NamespaceId, SourceFileKind,
};
use crate::binder::roots::{collect_root_rows, LibraryRootProjection, RootNameRow};
use crate::binder::scope::ScopeId;
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::class_semantics::CanonicalPublishedClassTerminal;
#[cfg(any(test, feature = "test-utils"))]
use crate::class_semantics::DemandOutcome;
use crate::class_semantics::OwnedPublishedClassTerminal;
#[cfg(any(test, feature = "test-utils"))]
use crate::diagnostics::render_type;
use crate::diagnostics::{render_to_writer_with_format, DiagnosticFormat};
use crate::source::{
    CompilationOrigin, LibraryFileOrdinal, ModuleOrdinal, SourceOrdinal, SourceUnit, UnitSlot,
};
use crate::span::Span;
use crate::types::repr::ClassId;
use crate::types::repr::TypeParamId;
#[cfg(any(test, feature = "test-utils"))]
use crate::types::repr::{
    IntrinsicKind, LiteralValue, ModifierOp, PropertyKey, TypeTag, Visibility, WellKnownSymbol,
};
use crate::types::repr::{ObjectType, PropertyType};
#[cfg(any(test, feature = "test-utils"))]
use crate::types::store::Store;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;
#[cfg(any(test, feature = "test-utils"))]
use sha2::{Digest, Sha256};
use std::cell::Cell;
#[cfg(any(test, feature = "test-utils"))]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Condvar, Mutex};
#[cfg(any(test, feature = "test-utils"))]
use std::time::{Duration, Instant};

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
fn record_private_replay_scheduler_work_for_test(family: &'static str, count: usize) {
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
fn record_private_replay_scheduler_work_for_test(_family: &'static str, _count: usize) {}

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
struct CollisionPlanForbiddenWork {
    library_source_compiles: u64,
    second_source_censuses: u64,
    canonical_manifest_bytes: u64,
    rendered_record_digest_bytes: u64,
    transitive_terminal_owner_entries: u64,
    eager_all_owner_scc_memberships: u64,
    namespace_snapshot_rows: u64,
    runtime_snapshot_rows: u64,
    canonical_terminal_rows: u64,
    full_semantic_projection_rows: u64,
}

thread_local! {
    static COLLISION_PLAN_FORBIDDEN_WORK: Cell<CollisionPlanForbiddenWork> =
        Cell::new(CollisionPlanForbiddenWork::default());
    #[cfg(any(test, feature = "test-utils"))]
    static FORCED_COLLISION_PLAN_FAILURE: Cell<Option<ForcedCollisionPlanFailure>> =
        const { Cell::new(None) };
    #[cfg(any(test, feature = "test-utils"))]
    static FULL_COLLISION_PLAN_ORACLE: RefCell<Option<FullCollisionPlanOracleForTest>> =
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
struct IndependentEventOwnerSiteOracle {
    file_ordinal: LibraryFileOrdinal,
    events: Vec<IndependentEventMeta>,
    next_event_ordinal: BTreeMap<LibraryFileOrdinal, usize>,
    ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>,
    owner_sites: Vec<CollisionReplayOwnerSite>,
    invalid_ticket: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl IndependentEventOwnerSiteOracle {
    fn new() -> Self {
        Self {
            file_ordinal: LibraryFileOrdinal::new(0),
            events: Vec::new(),
            next_event_ordinal: BTreeMap::new(),
            ticket_owners: BTreeMap::new(),
            owner_sites: Vec::new(),
            invalid_ticket: false,
        }
    }

    fn select_file(&mut self, file_ordinal: LibraryFileOrdinal) {
        self.file_ordinal = file_ordinal;
    }

    fn reserve_exact_site(&mut self, span: Span, phase: CollisionReplayEventPhase) {
        let (_, ticket) = self.reserve_event(span.start);
        self.record_owner_site(ticket, span, phase);
    }

    fn finish(mut self) -> Result<Vec<CollisionReplayOwnerSite>, InjectedProfileError> {
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
    let compiler = include_str!("library_compiler.rs");
    let replay = include_str!("replay_index.rs");
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
struct ForcedCollisionPlanFailureScope;

#[cfg(any(test, feature = "test-utils"))]
impl Drop for ForcedCollisionPlanFailureScope {
    fn drop(&mut self) {
        FORCED_COLLISION_PLAN_FAILURE.set(None);
    }
}

struct CollisionPlanForbiddenWorkScope(CollisionPlanForbiddenWork);

impl CollisionPlanForbiddenWorkScope {
    fn start() -> Self {
        Self(COLLISION_PLAN_FORBIDDEN_WORK.get())
    }

    fn finish(self) -> CollisionPlanForbiddenWork {
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

fn record_collision_plan_forbidden_work(update: impl FnOnce(&mut CollisionPlanForbiddenWork)) {
    COLLISION_PLAN_FORBIDDEN_WORK.set({
        let mut work = COLLISION_PLAN_FORBIDDEN_WORK.get();
        update(&mut work);
        work
    });
}

pub(super) fn record_collision_manifest_bytes(bytes: usize) {
    record_collision_plan_forbidden_work(|work| {
        work.canonical_manifest_bytes = work
            .canonical_manifest_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
    });
}

fn snapshot_namespace_terminals_for_replay(
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

pub struct OwnedLibraryRuntimeState {
    interner: Interner,
    binder: Binder,
    published_types: PublishedTypeEnvironment,
    decl_types: DeclTypes,
    semantic_identities: Option<LibrarySemanticIdentities>,
    runtime: FrozenCheckerRuntimeMetadata,
    next_type_param: u32,
    next_class_id: u32,
    source_file_count: u32,
    library_modules: std::sync::Arc<[ScopeId]>,
    replay_index: Option<Box<AdmittedCollisionReplayIndex>>,
    private_collision_epoch: Option<PrivateCollisionEpoch>,
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
    _permit: PrivateCollisionReplayPermitToken,
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

    fn owner_sites_for(
        plan: &CollisionReplayPlan,
        owners: &BTreeSet<ReplayOwner>,
    ) -> Vec<super::replay_index::CollisionReplayOwnerSite> {
        let mut sites = Vec::new();
        for owner in owners {
            sites.extend_from_slice(plan.owner_sites_for(*owner));
        }
        sites
    }

    fn baselines_for(
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
struct PrivateReplayScaleControlForTest {
    shared: std::sync::Arc<PrivateReplayScaleSharedForTest>,
    suppress_permit_instrumentation: bool,
    inject_checker_full_base_scan: bool,
    inject_checker_full_plan_scan: bool,
    inject_checker_full_source_registry_scan: bool,
    trace: PrivateReplayScaleRouteTraceForTest,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PRIVATE_REPLAY_SCALE_CONTROL: std::cell::RefCell<Option<PrivateReplayScaleControlForTest>> =
        const { std::cell::RefCell::new(None) };
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
fn with_private_replay_scale_control_for_test(
    update: impl FnOnce(&mut PrivateReplayScaleControlForTest),
) {
    PRIVATE_REPLAY_SCALE_CONTROL.with_borrow_mut(|active| {
        if let Some(active) = active.as_mut() {
            update(active);
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn record_private_replay_scale_hook_for_test(
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

pub(in crate::check::checker) struct OwnedLibraryRuntimeProductParts {
    pub(in crate::check::checker) interner: Interner,
    pub(in crate::check::checker) binder: Binder,
    pub(in crate::check::checker) published_types:
        super::type_groups::PublishedTypeEnvironmentProductParts,
    pub(in crate::check::checker) decl_types: Vec<Option<TypeId>>,
    pub(in crate::check::checker) semantic_identities:
        Option<super::library_identities::LibrarySemanticIdentitiesProductParts>,
    pub(in crate::check::checker) runtime: super::FrozenCheckerRuntimeSnapshotParts,
    pub(in crate::check::checker) next_type_param: u32,
    pub(in crate::check::checker) next_class_id: u32,
    pub(in crate::check::checker) source_file_count: u32,
}

pub struct CompiledLibraryRuntimeProduct {
    pub(in crate::check::checker) _parts: OwnedLibraryRuntimeProductParts,
    pub _replay_index: AdmittedCollisionReplayIndex,
}

pub fn freeze_library_runtime_product(
    mut state: OwnedLibraryRuntimeState,
) -> Result<CompiledLibraryRuntimeProduct, &'static str> {
    let replay_index = state
        .replay_index
        .take()
        .ok_or("source library compiler did not produce a replay index")?;
    state
        .into_product_parts()
        .map(|parts| CompiledLibraryRuntimeProduct {
            _parts: parts,
            _replay_index: *replay_index,
        })
}

fn validate_library_source_prefix(
    binder: &Binder,
    source_file_count: u32,
) -> Result<(), &'static str> {
    if source_file_count == 0 {
        return Err("product library state has no source files");
    }
    let mut source_keys = binder
        .module_sources()
        .values()
        .map(|source| source.0)
        .collect::<Vec<_>>();
    if source_keys.is_empty() {
        return Err("product binder has no retained source ownership");
    }
    source_keys.sort_unstable();
    if source_keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("product binder repeats a retained source key");
    }
    let expected_len = source_file_count
        .checked_add(1)
        .ok_or("product source count overflows the prelude prefix")?;
    if u32::try_from(source_keys.len()).map_err(|_| "product source-key count does not fit u32")?
        != expected_len
        || source_keys
            .iter()
            .enumerate()
            .any(|(index, source)| *source != u32::try_from(index).unwrap_or(u32::MAX))
    {
        return Err("product binder source keys are not the contiguous prelude/library prefix");
    }
    if binder
        .module_sources()
        .keys()
        .any(|scope| binder.graph.get(*scope).is_none())
    {
        return Err("product binder source ownership refers to an unknown scope");
    }
    Ok(())
}

fn validate_owned_library_product_parts(
    parts: &OwnedLibraryRuntimeProductParts,
) -> Result<(), &'static str> {
    fn type_in_range(id: TypeId, store_len: usize) -> bool {
        usize::try_from(id.0).is_ok_and(|index| index < store_len)
    }

    fn type_param_precedes_counter(id: TypeParamId, next_type_param: u32) -> bool {
        id.0 < next_type_param
    }

    validate_library_source_prefix(&parts.binder, parts.source_file_count)?;
    let authenticated_symbol = collect_root_rows(&parts.binder)
        .map_err(|_| "product binder root projection is invalid")?
        .into_iter()
        .find(|row| row.name == "Symbol")
        .and_then(|row| row.value);
    if parts
        .runtime
        .certified_library_values
        .symbol
        .is_some_and(|storage| storage.0 >= parts.binder.decl_count)
    {
        return Err("product certified Symbol value is out of range");
    }
    if parts.runtime.certified_library_values.symbol != authenticated_symbol {
        return Err("product certified Symbol value does not match the authenticated root");
    }
    if parts.published_types.groups.len() != parts.binder.type_groups.len() {
        return Err("product published type-group count does not match the binder");
    }
    if parts.decl_types.len()
        != usize::try_from(parts.binder.decl_count)
            .map_err(|_| "product binder storage count does not fit usize")?
    {
        return Err("product declaration types do not cover the binder storage prefix");
    }

    let store = parts.interner.store();
    let store_len = store.len();
    if parts
        .decl_types
        .iter()
        .flatten()
        .any(|ty| !type_in_range(*ty, store_len))
    {
        return Err("product declaration type is out of range");
    }

    let mut classes = BTreeSet::new();
    for (class, terminal) in &parts.published_types.classes {
        if !classes.insert(*class) {
            return Err("product class publication repeats a class id");
        }
        if class.0 >= parts.next_class_id {
            return Err("product class id collides with the next class counter");
        }
        let OwnedPublishedClassTerminal::Ready(surface) = terminal else {
            continue;
        };
        if surface
            .type_params()
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
        {
            return Err("product type parameter id collides with the next parameter counter");
        }
        if surface.class() != *class
            || !type_in_range(surface.instance_template(), store_len)
            || !type_in_range(surface.static_template(), store_len)
            || surface
                .constructor_template()
                .is_some_and(|ty| !type_in_range(ty, store_len))
        {
            return Err("product published class surface has an invalid reference");
        }
    }

    let mut class_group_names = BTreeMap::new();
    for (index, terminal) in parts.published_types.groups.iter().enumerate() {
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        let group_id = TypeGroupId(
            u32::try_from(index).map_err(|_| "product type-group index does not fit u32")?,
        );
        if parts
            .binder
            .type_groups
            .get(group_id)
            .is_none_or(|binding| binding.name != group.name)
        {
            return Err("product published type-group name does not match the binder");
        }
        if group
            .parameters
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
            || group.parameter_defaults.iter().any(|default| {
                matches!(default, PublishedTypeParameterDefault::Ready(ty) if !type_in_range(*ty, store_len))
            })
            || group
                .conflict_alternatives
                .iter()
                .flat_map(|alternative| &alternative.types)
                .any(|ty| !type_in_range(*ty, store_len))
        {
            return Err("product published type group has an invalid type reference");
        }
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) if !type_in_range(ty, store_len) => {
                return Err("product published type-group template is out of range")
            }
            PublishedTypeGroupSurface::Class(class) => {
                if !classes.contains(&class) {
                    return Err("product published type group refers to an unknown class");
                }
                if class_group_names
                    .insert(class, group.name.as_str())
                    .is_some()
                {
                    return Err("product class identity is published by multiple type groups");
                }
            }
            PublishedTypeGroupSurface::Template(_) => {}
        }
    }
    if class_group_names.keys().copied().collect::<BTreeSet<_>>() != classes {
        return Err("product published classes do not have exact type-group ownership");
    }

    for index in 0..store_len {
        let ty = TypeId(
            u32::try_from(index).map_err(|_| "product type-store length does not fit TypeId")?,
        );
        if store
            .type_param(ty)
            .is_some_and(|parameter| parameter.id.0 >= parts.next_type_param)
            || store.function_type(ty).is_some_and(|function| {
                function
                    .type_params
                    .iter()
                    .any(|parameter| parameter.id.0 >= parts.next_type_param)
            })
            || store.instantiation_type(ty).is_some_and(|instantiation| {
                instantiation
                    .args
                    .iter()
                    .any(|(parameter, _)| parameter.0 >= parts.next_type_param)
            })
        {
            return Err("product type parameter id collides with the next parameter counter");
        }
        if store.class_instance_type(ty).is_some_and(|instance| {
            instance.class.0 >= parts.next_class_id || !classes.contains(&instance.class)
        }) {
            return Err("product class instance refers to an unknown class identity");
        }
        if store.object_type(ty).is_some_and(|object| {
            object.properties.iter().any(|property| {
                property.declaring_class.is_some_and(|class| {
                    class.0 >= parts.next_class_id || !classes.contains(&class)
                })
            })
        }) {
            return Err("product object property refers to an unknown class identity");
        }
    }

    if let Some(identities) = &parts.semantic_identities {
        let expected_names = [
            "Array",
            "ReadonlyArray",
            "String",
            "Number",
            "Boolean",
            "RegExp",
            "Object",
            "CallableFunction",
        ];
        for (terminal, expected_name) in identities.iter().zip(expected_names) {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                return Err("product installed semantic identities are not all ready");
            };
            if !type_in_range(identity.template, store_len)
                || identity
                    .parameters
                    .iter()
                    .any(|parameter| parameter.0 >= parts.next_type_param)
            {
                return Err("product semantic identity has an invalid type reference");
            }
            let Some(PublishedTypeGroupTerminal::Ready(group)) =
                parts.published_types.groups.get(identity.group.index())
            else {
                return Err("product semantic identity refers to an unpublished type group");
            };
            if group.name != expected_name
                || group.parameters != identity.parameters
                || group.surface != PublishedTypeGroupSurface::Template(identity.template)
            {
                return Err("product semantic identity does not match its published type group");
            }
        }
    }

    let valid_class = |class: ClassId| class.0 < parts.next_class_id && classes.contains(&class);
    let valid_storage = |storage: ValueStorageId| storage.0 < parts.binder.decl_count;
    let application_classes = parts
        .runtime
        .class_application_parameters
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if application_classes != classes {
        return Err("product class application metadata does not exactly cover published classes");
    }
    let new_metadata_classes = parts
        .runtime
        .class_new_metadata
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if new_metadata_classes != classes {
        return Err("product new metadata does not exactly cover published classes");
    }
    let class_name_rows = parts
        .runtime
        .class_names
        .iter()
        .map(|(class, name)| (*class, name.as_str()))
        .collect::<BTreeMap<_, _>>();
    if class_name_rows.len() != parts.runtime.class_names.len()
        || class_name_rows.keys().copied().collect::<BTreeSet<_>>() != classes
        || class_name_rows.iter().any(|(class, name)| {
            class_group_names
                .get(class)
                .is_none_or(|group_name| group_name != name)
        })
    {
        return Err("product class names do not exactly match published class groups");
    }
    let mut bound_classes = BTreeSet::new();
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(_, binding)| !bound_classes.insert(binding.class_id))
        || bound_classes != classes
    {
        return Err("product class value bindings do not exactly cover published classes");
    }
    for (class, parameters) in &parts.runtime.class_application_parameters {
        if !valid_class(*class) {
            return Err("product class application metadata refers to an unknown class");
        }
        for parameter in parameters {
            if parameter.id.0 >= parts.next_type_param
                || parameter
                    .constraint
                    .is_some_and(|ty| !type_in_range(ty, store_len))
                || matches!(parameter.default, ClassTypeParameterDefault::Ready(ty) if !type_in_range(ty, store_len))
            {
                return Err("product class application metadata has an invalid type reference");
            }
        }
        if let Some((_, OwnedPublishedClassTerminal::Ready(surface))) = parts
            .published_types
            .classes
            .iter()
            .find(|(published, _)| published == class)
        {
            let parameter_ids = parameters
                .iter()
                .map(|parameter| parameter.id)
                .collect::<Vec<_>>();
            if parameter_ids != surface.type_params() {
                return Err(
                    "product class application parameters do not match the published class",
                );
            }
        }
    }
    for (class, metadata) in &parts.runtime.class_new_metadata {
        if !valid_class(*class) || !valid_class(metadata.ctor_declaring_class) {
            return Err("product runtime class metadata refers to an unknown class");
        }
        let mut current = *class;
        let mut visited = BTreeSet::new();
        while current != metadata.ctor_declaring_class && visited.insert(current) {
            let Some(parent) = parts
                .runtime
                .class_parents
                .iter()
                .find_map(|(child, parent)| (*child == current).then_some(*parent))
            else {
                break;
            };
            current = parent;
        }
        if current != metadata.ctor_declaring_class {
            return Err("product constructor owner is not on the class parent chain");
        }
    }
    if parts
        .runtime
        .class_parents
        .iter()
        .any(|(class, parent)| !valid_class(*class) || !valid_class(*parent))
        || parts
            .runtime
            .class_names
            .iter()
            .any(|(class, _)| !valid_class(*class))
    {
        return Err("product runtime class metadata refers to an unknown class");
    }
    if parts
        .runtime
        .class_value_aliases
        .iter()
        .chain(&parts.runtime.standalone_namespace_value_aliases)
        .any(|(alias, target)| !valid_storage(*alias) || !valid_storage(*target))
    {
        return Err("product runtime value alias is out of range");
    }
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(storage, binding)| !valid_storage(*storage) || !valid_class(binding.class_id))
    {
        return Err("product class value binding has an invalid reference");
    }
    for (_, binding) in &parts.runtime.class_value_bindings {
        let Some((_, parameters)) = parts
            .runtime
            .class_application_parameters
            .iter()
            .find(|(class, _)| *class == binding.class_id)
        else {
            return Err("product class value binding has no application metadata");
        };
        let expected_generic = !parameters.is_empty();
        if binding.has_header_type_params != expected_generic {
            return Err(
                "product class value binding generic bit does not match application metadata",
            );
        }
    }
    if parts.runtime.class_value_aliases.iter().any(|(_, target)| {
        !parts
            .runtime
            .class_value_bindings
            .iter()
            .any(|(storage, _)| storage == target)
    }) {
        return Err("product class value alias does not target a class binding");
    }

    let ready_namespace_storages = parts
        .runtime
        .namespace_terminals
        .iter()
        .filter_map(|row| match row.terminal {
            FrozenNamespaceValueTerminalSnapshot::Ready { storage, .. } => Some(storage),
            FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => None,
        })
        .collect::<Vec<_>>();
    for row in &parts.runtime.namespace_terminals {
        if parts.binder.namespaces.get(row.namespace).is_none() {
            return Err("product namespace terminal refers to an unknown namespace");
        }
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            if !valid_storage(storage) || !type_in_range(ty, store_len) {
                return Err("product namespace terminal has an invalid ready reference");
            }
        }
    }
    if parts
        .runtime
        .standalone_namespace_value_aliases
        .iter()
        .any(|(_, target)| !ready_namespace_storages.contains(target))
    {
        return Err("product namespace alias does not target a ready namespace root");
    }
    if parts.runtime.named_function_symbols.iter().any(|symbol| {
        parts
            .binder
            .symbols
            .get(*symbol)
            .is_none_or(|binding| binding.function_values.is_empty())
    }) {
        return Err("product named-function metadata refers to a non-function symbol");
    }
    Ok(())
}

impl OwnedLibraryRuntimeState {
    pub fn complete_replay_binder_checkpoint(
        &self,
    ) -> Result<LibraryBinderCheckpoint, &'static str> {
        let binder = self.binder.fork_sparse_collision_delta()?;
        let library_units = self
            .library_modules
            .iter()
            .copied()
            .enumerate()
            .map(|(ordinal, module)| {
                let source = binder
                    .source_for_module(module)
                    .ok_or("complete replay module has no retained source key")?;
                Ok(LibraryBinderUnit {
                    ordinal: LibraryFileOrdinal::new(ordinal),
                    source,
                    module,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(build_library_binder_checkpoint(binder, library_units))
    }

    #[cfg(test)]
    fn collect_library_modules(
        binder: &Binder,
        source_file_count: u32,
    ) -> Result<std::sync::Arc<[ScopeId]>, &'static str> {
        let count =
            usize::try_from(source_file_count).map_err(|_| "library source count overflows")?;
        let mut modules = vec![None; count];
        for (scope, source) in binder.module_sources().iter() {
            let Some(index) = source
                .0
                .checked_sub(1)
                .and_then(|source| usize::try_from(source).ok())
            else {
                continue;
            };
            if let Some(module) = modules.get_mut(index) {
                *module = Some(*scope);
            }
        }
        modules
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .map(Into::into)
            .ok_or("library runtime is missing a source module")
    }

    pub(in crate::check::checker) fn into_user_project_base(
        self,
    ) -> (Interner, Binder, super::BoundUserBase) {
        let Self {
            interner,
            binder,
            published_types,
            decl_types,
            semantic_identities,
            runtime,
            next_type_param,
            next_class_id,
            source_file_count: _,
            library_modules,
            replay_index: _,
            private_collision_epoch,
        } = self;
        (
            interner,
            binder,
            super::BoundUserBase {
                published_types,
                library_semantic_identities: semantic_identities,
                lexical_array_alias: None,
                decl_types,
                next_type_param,
                next_class_id,
                runtime,
                private_collision_epoch,
                library_modules,
            },
        )
    }

    pub fn freeze_as_library_base(&mut self) -> Result<(), &'static str> {
        self.interner.freeze_as_base()?;
        self.binder.freeze_as_base()?;
        self.published_types.freeze_as_base()?;
        self.decl_types.freeze_as_base()?;
        self.runtime.freeze_as_base()?;
        #[cfg(any(test, feature = "test-utils"))]
        record_complete_source_route_work_for_test(|work| {
            work.frozen_base_seals = work.frozen_base_seals.saturating_add(1);
        });
        Ok(())
    }

    pub fn fork_collision_free_user_delta(
        &self,
        _capability: CollisionFreeUserDeltaCapability,
    ) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    fn fork_user_delta(&self) -> Result<Self, &'static str> {
        #[cfg(any(test, feature = "test-utils"))]
        USER_DELTA_FORKS.set(USER_DELTA_FORKS.get().saturating_add(1));
        Ok(Self {
            interner: self.interner.fork_delta()?,
            binder: self.binder.fork_delta()?,
            published_types: self.published_types.fork_delta()?,
            decl_types: self.decl_types.fork_delta()?,
            semantic_identities: self.semantic_identities.clone(),
            runtime: self.runtime.fork_delta()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
            library_modules: std::sync::Arc::clone(&self.library_modules),
            replay_index: None,
            private_collision_epoch: None,
        })
    }

    pub fn fork_sparse_collision_epoch(&self) -> Result<Self, &'static str> {
        Ok(Self {
            interner: self.interner.fork_delta()?,
            binder: self.binder.fork_sparse_collision_delta()?,
            published_types: self.published_types.fork_sparse_delta()?,
            decl_types: self.decl_types.fork_sparse_delta()?,
            semantic_identities: self.semantic_identities.clone(),
            runtime: self.runtime.fork_sparse_delta()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
            library_modules: std::sync::Arc::clone(&self.library_modules),
            replay_index: None,
            private_collision_epoch: None,
        })
    }

    pub fn install_private_collision_replay(
        self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: Vec<PrivateCollisionReplaySeed>,
    ) -> Result<Self, &'static str> {
        let permit = acquire_private_collision_replay_permit()?;
        self.install_private_collision_replay_with_permit(plan, seeds, permit)
    }

    pub fn install_private_collision_replay_with_permit(
        mut self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: Vec<PrivateCollisionReplaySeed>,
        permit: PrivateCollisionReplayPermitToken,
    ) -> Result<Self, &'static str> {
        if self.private_collision_epoch.is_some() || seeds.is_empty() {
            return Err("private collision replay installs exactly once with nonempty seeds");
        }
        let mut affected_owners = BTreeSet::new();
        let mut value_winners = BTreeMap::new();
        for seed in &seeds {
            record_private_replay_scheduler_work_for_test("seed-pops", 1);
            if seed.global_object {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(ReplayOwner::GlobalObject) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
            let Some(root) = plan.root_slot(&seed.name) else {
                continue;
            };
            if seed.value {
                if let Some(value) = root.value {
                    value_winners.insert(root.name.clone(), value);
                }
            }
            // A same-name global declaration can affect merge disposition across spaces even
            // when its own syntax contributes only one slot. Seed every retained root meaning;
            // the compact reverse closure removes no-op consumers later.
            for owner in [
                root.value.map(ReplayOwner::Value),
                root.ty.map(ReplayOwner::TypeGroup),
                root.namespace.map(ReplayOwner::Namespace),
            ]
            .into_iter()
            .flatten()
            {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(owner) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
            // A user reopening a global namespace can mutate rows owned by that namespace
            // without naming the nested declaration in preflight. Seed every exact library owner
            // declared directly in the retained namespace; post-bind mutation validation below
            // still proves that the conservative expansion covered the actual changed rows.
            if seed.namespace {
                if let Some(namespace) = root.namespace {
                    for owner in plan.namespace_direct_owners(namespace) {
                        if affected_owners.insert(owner) {
                            record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                            record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                        }
                    }
                }
            }
            for consumer in plan.root_consumers(&seed.name) {
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(consumer.consumer) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    record_private_replay_scheduler_work_for_test("seed-pushes", 1);
                }
            }
        }
        let mut pending = affected_owners.iter().copied().collect::<Vec<_>>();
        record_private_replay_scheduler_work_for_test("replay-allocations", pending.len());
        record_private_replay_scheduler_work_for_test("scc-queue-pushes", pending.len());
        let mut cursor = 0;
        while let Some(owner) = pending.get(cursor).copied() {
            cursor += 1;
            record_private_replay_scheduler_work_for_test("scc-queue-pops", 1);
            for edge in plan.reverse_consumers(owner) {
                record_private_replay_scheduler_work_for_test("edge-probes", 1);
                record_private_replay_scheduler_work_for_test("owner-set-probes", 1);
                if affected_owners.insert(edge.consumer) {
                    record_private_replay_scheduler_work_for_test("owner-set-inserts", 1);
                    pending.push(edge.consumer);
                    record_private_replay_scheduler_work_for_test("scc-queue-pushes", 1);
                }
            }
        }
        let mut owner_sites = Vec::new();
        for owner in &affected_owners {
            owner_sites.extend_from_slice(plan.owner_sites_for(*owner));
        }
        record_private_replay_scheduler_work_for_test("replay-allocations", 1);
        let library_record_baselines =
            PrivateCollisionEpoch::baselines_for(&plan, &affected_owners);
        self.private_collision_epoch = Some(PrivateCollisionEpoch {
            affected_owners,
            mutation_owners: BTreeSet::new(),
            value_winners,
            owner_sites,
            library_record_baselines,
            sources: Vec::new(),
            complete_source_replay: false,
            plan,
            _permit: permit,
            #[cfg(any(test, feature = "test-utils"))]
            state_drop_witness: Some(PrivateCollisionStateDropWitnessForTest),
        });
        #[cfg(any(test, feature = "test-utils"))]
        record_private_replay_scale_hook_for_test(|trace, event| {
            trace.sparse_replay_invocations = trace.sparse_replay_invocations.saturating_add(1);
            trace.private_work_started = event;
        });
        Ok(self)
    }

    fn install_complete_source_replay_plan_with_permit(
        mut self,
        plan: std::sync::Arc<CollisionReplayPlan>,
        seeds: &[PrivateCollisionReplaySeed],
        permit: PrivateCollisionReplayPermitToken,
    ) -> Result<Self, &'static str> {
        if self.private_collision_epoch.is_some() {
            return Err("complete source replay installs exactly once");
        }
        let materialized = plan.complete_source_materialization();
        // A complete continuation can replace an actually colliding root with its user suffix.
        let mut affected_owners = materialized.owners;
        for seed in seeds {
            if seed.global_object {
                affected_owners.insert(ReplayOwner::GlobalObject);
            }
            if !seed.value {
                continue;
            }
            let Some(root) = plan.root_slot(&seed.name) else {
                continue;
            };
            affected_owners.extend(root.value.map(ReplayOwner::Value));
        }
        let value_winners = seeds
            .iter()
            .filter(|seed| seed.value)
            .filter_map(|seed| {
                plan.root_slot(&seed.name)
                    .and_then(|root| root.value)
                    .map(|value| (seed.name.clone(), value))
            })
            .collect();
        self.private_collision_epoch = Some(PrivateCollisionEpoch {
            affected_owners,
            mutation_owners: BTreeSet::new(),
            value_winners,
            owner_sites: materialized.owner_sites,
            library_record_baselines: materialized.baseline_records,
            sources: Vec::new(),
            complete_source_replay: true,
            plan,
            _permit: permit,
            #[cfg(any(test, feature = "test-utils"))]
            state_drop_witness: Some(PrivateCollisionStateDropWitnessForTest),
        });
        Ok(self)
    }

    pub fn private_collision_source_ordinals(
        &self,
    ) -> Result<Vec<LibraryFileOrdinal>, &'static str> {
        let epoch = self
            .private_collision_epoch
            .as_ref()
            .ok_or("private collision replay is not installed")?;
        if epoch.complete_source_replay {
            return Ok((0..self.source_file_count)
                .map(|ordinal| {
                    LibraryFileOrdinal::new(usize::try_from(ordinal).unwrap_or(usize::MAX))
                })
                .collect());
        }
        let mut ordinals = epoch
            .owner_sites
            .iter()
            .filter(|site| !matches!(site.owner, ReplayOwner::GlobalObject))
            .map(|site| site.file_ordinal)
            .collect::<Vec<_>>();
        if ordinals.is_empty() {
            if let Some(site) = epoch
                .owner_sites
                .iter()
                .find(|site| matches!(site.owner, ReplayOwner::GlobalObject))
            {
                ordinals.push(site.file_ordinal);
            }
        }
        ordinals.sort();
        ordinals.dedup();
        Ok(ordinals)
    }

    pub fn install_private_collision_sources(
        mut self,
        mut sources: Vec<PrivateCollisionReplaySource>,
    ) -> Result<Self, &'static str> {
        let expected = self.private_collision_source_ordinals()?;
        let epoch = self
            .private_collision_epoch
            .as_mut()
            .ok_or("private collision replay is not installed")?;
        if !epoch.sources.is_empty() {
            return Err("private collision sources install exactly once");
        }
        sources.sort_by_key(|source| source.file_ordinal);
        let observed = sources
            .iter()
            .map(|source| source.file_ordinal)
            .collect::<Vec<_>>();
        if observed != expected {
            return Err("private collision sources do not cover the scheduled ordinals");
        }
        #[cfg(any(test, feature = "test-utils"))]
        with_private_replay_scale_control_for_test(|active| {
            active.trace.sparse_library_source_units = active
                .trace
                .sparse_library_source_units
                .saturating_add(u64::try_from(sources.len()).unwrap_or(u64::MAX));
        });
        epoch.sources = sources;
        Ok(self)
    }

    pub fn take_private_collision_sources(
        &mut self,
    ) -> Result<Vec<PrivateCollisionReplaySource>, &'static str> {
        let epoch = self
            .private_collision_epoch
            .as_mut()
            .ok_or("private collision replay is not installed")?;
        Ok(std::mem::take(&mut epoch.sources))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn private_collision_affected_owners_for_test(
        &self,
    ) -> Result<BTreeSet<ReplayOwner>, &'static str> {
        self.private_collision_epoch
            .as_ref()
            .map(|epoch| epoch.affected_owners.clone())
            .ok_or("private collision replay is not installed")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn private_collision_owner_sites_for_test(
        &self,
    ) -> Result<Vec<super::replay_index::CollisionReplayOwnerSite>, &'static str> {
        self.private_collision_epoch
            .as_ref()
            .map(|epoch| epoch.owner_sites.clone())
            .ok_or("private collision replay is not installed")
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn fork_user_delta_for_test(&self) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    #[cfg(test)]
    fn shares_checker_base_with(&self, other: &Self) -> bool {
        self.published_types
            .shares_base_with(&other.published_types)
            && self.decl_types.shares_base_with(&other.decl_types)
            && self.runtime.shares_base_with(&other.runtime)
            && match (&self.semantic_identities, &other.semantic_identities) {
                (Some(left), Some(right)) => left.shares_storage_with(right),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
    }

    /// Reference-row counts of the frozen base: store, interner identity, binder.
    ///
    /// A user delta that leaked a row into the base would move one of these.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn reference_record_counts_for_test(&self) -> [usize; 3] {
        let (store, interner) = self.interner.reference_records_for_test();
        let binder = crate::binder::references::reference_records(&self.binder)
            .expect("frozen binder projects its reference rows")
            .len();
        [store.len(), interner.len(), binder]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn storage_identity_for_test(&self) -> [usize; 8] {
        [
            std::ptr::from_ref(self).addr(),
            std::ptr::from_ref(&self.interner).addr(),
            std::ptr::from_ref(self.interner.store()).addr(),
            std::ptr::from_ref(&self.binder).addr(),
            std::ptr::from_ref(&self.published_types).addr(),
            std::ptr::from_ref(&self.decl_types).addr(),
            std::ptr::from_ref(&self.runtime).addr(),
            self.semantic_identities
                .as_ref()
                .map_or(0, |identities| std::ptr::from_ref(identities).addr()),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn inner_base_allocation_identity_for_test(&self) -> usize {
        self.interner.store().base_allocation_identity_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn initial_visible_user_names_for_test(&self) -> BTreeSet<String> {
        self.binder.local_names_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn install_user_delta_drop_witness_for_test(
        &mut self,
        discarded: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.interner
            .install_user_delta_drop_witness_for_test(discarded);
    }

    /// Frozen id prefixes in `FrozenLibraryPrefixes` order: types, type params, classes,
    /// scopes, symbols, declarations, type groups, namespaces, value storages.
    pub fn library_prefixes(&self) -> Result<[usize; 9], &'static str> {
        Ok([
            self.interner.store().len(),
            usize::try_from(self.next_type_param)
                .map_err(|_| "type parameter end exceeds usize")?,
            usize::try_from(self.next_class_id).map_err(|_| "class end exceeds usize")?,
            self.binder.graph.len(),
            self.binder.symbols.len(),
            self.binder.declarations.len(),
            self.binder.type_groups.len(),
            self.binder.namespaces.len(),
            self.decl_types.len(),
        ])
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn identity_ends_for_test(&self) -> OwnedBaseFinalIdentityEnds {
        OwnedBaseFinalIdentityEnds {
            store: self.interner.store().len(),
            declared_recipes: self.interner.store().all_declared_recipes().count(),
            type_params: usize::try_from(self.next_type_param)
                .expect("type parameter end fits usize"),
            classes: usize::try_from(self.next_class_id).expect("class end fits usize"),
            scopes: self.binder.graph.len(),
            symbols: self.binder.symbols.len(),
            declarations: self.binder.declarations.len(),
            type_groups: self.binder.type_groups.len(),
            namespaces: self.binder.namespaces.len(),
            value_storages: self.decl_types.len(),
            source_units: self.binder.checkpoint_ends().next_source,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn named_type_for_test(&self, name: &str) -> Option<TypeId> {
        let group = self
            .binder
            .resolve_type(self.binder.compilation_global, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))?
            .ty?;
        let PublishedTypeGroupTerminal::Ready(group) = self.published_types.groups().get(group)?
        else {
            return None;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) => Some(ty),
            PublishedTypeGroupSurface::Class(class) => {
                match self.published_types.classes().published_class(class) {
                    DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                    DemandOutcome::Exhausted(_) => None,
                }
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_root_projection_for_test(&self, names: &[String]) -> Vec<String> {
        let mut projection = names
            .iter()
            .map(|name| {
                let symbol = self
                    .binder
                    .graph
                    .get(self.binder.compilation_global)
                    .and_then(|scope| scope.symbols.get(name))
                    .and_then(|symbol| self.binder.symbols.get(*symbol));
                let value = symbol.and_then(|symbol| symbol.value);
                let namespace = symbol.and_then(|symbol| symbol.ns);
                let ty = symbol
                    .and_then(|symbol| symbol.ty)
                    .and_then(|group| self.published_types.groups().get(group))
                    .map(|terminal| match terminal {
                        PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                            PublishedTypeGroupSurface::Template(template) => {
                                render_type(self.interner.store(), template, false)
                            }
                            PublishedTypeGroupSurface::Class(class) => {
                                match self.published_types.classes().published_class(class) {
                                    DemandOutcome::Ready(surface) => render_type(
                                        self.interner.store(),
                                        surface.instance_template(),
                                        false,
                                    ),
                                    DemandOutcome::Exhausted(_) => "class:unavailable".to_owned(),
                                }
                            }
                        },
                        PublishedTypeGroupTerminal::Unavailable(cause) => {
                            format!("unavailable:{:?}", cause.cause)
                        }
                    });
                format!(
                    "{name}:value={}:type={}:namespace={}",
                    value.is_some(),
                    ty.unwrap_or_else(|| "absent".to_owned()),
                    namespace.is_some()
                )
            })
            .collect::<Vec<_>>();
        projection.sort();
        projection
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_semantic_identities_for_test(&self) -> Vec<String> {
        self.semantic_identities
            .as_ref()
            .map_or_else(Vec::new, |identities| {
                identities.normalized_projection_for_test(self.interner.store())
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn frozen_structural_object_probe_for_test(&self) -> Option<(TypeId, ObjectType)> {
        self.interner.frozen_structural_object_probe_for_test()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn reintern_structural_type_for_test(
        &self,
        descriptor: ObjectType,
    ) -> Result<(TypeId, usize), &'static str> {
        let mut delta = self.fork_user_delta_for_test()?;
        let before = delta.interner.store().len();
        let resolved = delta.interner.intern_object(descriptor);
        Ok((resolved, delta.interner.store().len() - before))
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn final_base_family_clone_counts_for_test<Ticket: Copy + PartialEq>(
        &self,
        pass: &super::context::Pass<'_, '_, Ticket>,
        final_next_class_id: u32,
    ) -> BTreeMap<&'static str, u64> {
        let store = self
            .interner
            .store()
            .base_family_sharing_with(pass.interner.store());
        let interner = self.interner.base_index_family_sharing_with(pass.interner);
        let binder = self.binder.base_family_sharing_with(pass.binder);
        let published = self
            .published_types
            .base_family_sharing_with(pass.type_environment.published());
        let identities = match (
            self.semantic_identities.as_ref(),
            pass.library_semantic_identities.as_ref(),
        ) {
            (Some(base), Some(final_state)) => base.shares_storage_with(final_state),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        let runtime = &self.runtime;
        let checks = [
            ("store.rows", store[0]),
            ("store.payload-tables", store[1]),
            ("store.type-param-constraints", store[2]),
            ("store.frozen-type-params", store[3]),
            ("store.template-names", store[4]),
            ("interner.dedup-buckets", interner[0]),
            ("interner.reserved-terminals", interner[1]),
            ("interner.declared-recipes", interner[2]),
            ("interner.well-known", interner[3]),
            ("binder.scopes", binder[0]),
            ("binder.symbols", binder[1]),
            ("binder.declarations", binder[2]),
            ("binder.declaration-site-index", binder[3]),
            ("binder.type-groups", binder[4]),
            ("binder.namespaces", binder[5]),
            ("binder.namespace-indexes", binder[6]),
            ("binder.module-sources", binder[7]),
            (
                "decl-types.slots",
                self.decl_types.shares_base_with(&pass.decl_types),
            ),
            ("published-types.groups", published[0]),
            ("published-types.classes", published[1]),
            (
                "namespace-terminals",
                pass.namespace_values
                    .terminals_share_base_with(&runtime.namespace_terminals),
            ),
            (
                "function-groups.symbols",
                runtime
                    .named_function_symbols
                    .shares_base_with(&pass.named_function_symbols),
            ),
            (
                "class.application-parameters",
                runtime
                    .class_application_parameters
                    .shares_base_with(&pass.class_application_parameters),
            ),
            ("class.parameter-defaults", published[0]),
            (
                "class.parents",
                runtime.class_parents.shares_base_with(&pass.class_parents),
            ),
            (
                "class.names",
                runtime.class_names.shares_base_with(&pass.class_names),
            ),
            (
                "class.new-metadata",
                runtime
                    .class_new_metadata
                    .shares_base_with(&pass.class_new_metadata),
            ),
            (
                "class.value-identities",
                runtime
                    .class_value_bindings
                    .shares_base_with(&pass.class_value_bindings),
            ),
            (
                "class.aliases",
                runtime
                    .class_value_aliases
                    .shares_base_with(&pass.class_value_aliases)
                    && runtime
                        .standalone_namespace_value_aliases
                        .shares_base_with(&pass.standalone_namespace_value_aliases),
            ),
            ("semantic-identities", identities),
            (
                "next-ids",
                pass.next_type_param >= self.next_type_param
                    && final_next_class_id >= self.next_class_id,
            ),
        ];
        checks
            .into_iter()
            .map(|(family, shared)| (family, u64::from(!shared)))
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn final_local_rows_written_for_test<Ticket: Copy + PartialEq>(
        pass: &super::context::Pass<'_, '_, Ticket>,
    ) -> u64 {
        let store = pass.interner.store().local_family_row_counts_for_test();
        let interner = pass.interner.local_index_row_counts_for_test();
        let binder = pass.binder.local_family_row_counts_for_test();
        let published = pass
            .type_environment
            .published()
            .local_family_row_counts_for_test();
        let published_replacements = pass
            .type_environment
            .published()
            .replacement_family_row_counts_for_test();
        let rows = store.into_iter().sum::<usize>()
            + interner.into_iter().sum::<usize>()
            + binder.into_iter().sum::<usize>()
            + pass.decl_types.local_len()
            + pass.decl_types.replacement_len()
            + published.into_iter().sum::<usize>()
            + published_replacements.into_iter().sum::<usize>()
            + pass.namespace_values.local_terminal_row_count_for_test()
            + pass
                .namespace_values
                .replacement_terminal_row_count_for_test()
            + pass.named_function_symbols.local_len()
            + pass.class_application_parameters.local_len()
            + pass.class_application_parameters.replacement_len()
            + pass.class_parents.local_len()
            + pass.class_parents.replacement_len()
            + pass.class_names.local_len()
            + pass.class_names.replacement_len()
            + pass.class_new_metadata.local_len()
            + pass.class_new_metadata.replacement_len()
            + pass.class_value_bindings.local_len()
            + pass.class_value_bindings.replacement_len()
            + pass.class_value_aliases.local_len()
            + pass.class_value_aliases.replacement_len()
            + pass.standalone_namespace_value_aliases.local_len()
            + pass.standalone_namespace_value_aliases.replacement_len();
        u64::try_from(rows).expect("local row count fits u64")
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn type_count(&self) -> usize {
        self.interner.store().len()
    }

    pub(in crate::check::checker) fn into_product_parts(
        self,
    ) -> Result<OwnedLibraryRuntimeProductParts, &'static str> {
        if self
            .semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("product installed semantic identities are not all ready");
        }
        let parts = OwnedLibraryRuntimeProductParts {
            interner: self.interner,
            binder: self.binder,
            published_types: self.published_types.product_parts()?,
            decl_types: self.decl_types.snapshot_slots(),
            semantic_identities: self
                .semantic_identities
                .as_ref()
                .map(super::library_identities::LibrarySemanticIdentities::product_parts),
            runtime: self.runtime.snapshot_parts()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
        };
        validate_owned_library_product_parts(&parts)?;
        Ok(parts)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn replay_index(&self) -> Option<&AdmittedCollisionReplayIndex> {
        self.replay_index.as_deref()
    }

    /// Restore a runtime from its own product parts; the base it yields carries no replay index.
    #[cfg(test)]
    pub(in crate::check::checker) fn from_product_parts(
        parts: OwnedLibraryRuntimeProductParts,
    ) -> Result<Self, &'static str> {
        validate_owned_library_product_parts(&parts)?;
        let library_modules =
            Self::collect_library_modules(&parts.binder, parts.source_file_count)?;
        let decl_types = DeclTypes::from_snapshot_slots(parts.decl_types, parts.binder.decl_count)?;
        let semantic_identities = parts
            .semantic_identities
            .map(super::library_identities::LibrarySemanticIdentities::from_product_parts)
            .transpose()?;
        if semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("product installed semantic identities are not all ready");
        }
        Ok(Self {
            interner: parts.interner,
            binder: parts.binder,
            published_types: PublishedTypeEnvironment::from_product_parts(parts.published_types)?,
            decl_types,
            semantic_identities,
            runtime: FrozenCheckerRuntimeMetadata::from_snapshot_parts(parts.runtime)?,
            next_type_param: parts.next_type_param,
            next_class_id: parts.next_class_id,
            source_file_count: parts.source_file_count,
            library_modules,
            replay_index: None,
            private_collision_epoch: None,
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_synthetic_padding_base_for_test(
    namespace_count: usize,
) -> Result<OwnedLibraryRuntimeState, String> {
    let mut source = String::new();
    for index in 0..namespace_count {
        source.push_str(&format!(
            "declare namespace Pad{index} {{\n\
             export class Box<T> {{ value: T; }}\n\
             export interface Shape {{ value: string; }}\n\
             export const value: string;\n\
             }}\n"
        ));
    }
    let injected = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "synthetic-padding.d.ts",
        source: &source,
    }];
    let (_, mut state) = compile_owned_injected_profile(&injected)
        .map_err(|error| format!("synthetic padding base failed: {error:?}"))?;
    state.freeze_as_library_base().map_err(str::to_owned)?;
    Ok(state)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn with_forced_collision_identity_selection_pending<T>(run: impl FnOnce() -> T) -> T {
    super::library_identities::with_forced_collision_identity_selection_pending(run)
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseUserTimings {
    pub parse: Duration,
    pub bind: Duration,
    pub check: Duration,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct OwnedBaseUserRun {
    pub result: super::CheckResult,
    pub timings: OwnedBaseUserTimings,
    pub witness: OwnedBaseContinuationWitness,
    pub final_identity: OwnedBaseFinalIdentityWitness,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct OwnedBaseUserProjectRun {
    pub reports: Vec<crate::check::test_support::FileReport>,
    pub final_identity: OwnedBaseFinalIdentityWitness,
    pub cross_file: OwnedBaseCrossFileWitness,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedBaseCrossFileWitness {
    pub producer_type_group: TypeGroupId,
    pub consumer_type_group: TypeGroupId,
    pub producer_value_storage: ValueStorageId,
    pub consumer_value_storage: ValueStorageId,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static USER_SOURCE_PARSE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_BIND_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_CHECK_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_DELTA_FORKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PRIVATE_REPLAY_PRODUCTION_CONTROL:
        std::cell::RefCell<Option<PrivateReplayProductionControlForTest>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateReplayProductionFaultForTest {
    None,
    InjectPostBindMutationOwnerAbsentFromPlan,
    OmitExpectedBaselineRecordDuringCompletion,
    DisableSealedExpectedBaselineOwnerBeforeCandidateReservation,
    OmitScheduledOwner(ReplayOwner),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default)]
pub struct PrivateReplayProductionTraceForTest {
    pub bind_completed: bool,
    pub sparse_candidate_execution_started: bool,
    pub completion_selection_started: bool,
    pub mutation_ledger_recorded: u64,
    pub containment_validation_started: u64,
    pub plan_owner_intersection_started: u64,
    pub injected_after_bind: bool,
    pub injected_owner: Option<ReplayOwner>,
    pub omitted_expected_baseline: Option<super::replay_index::ReplayBaselineRecord>,
    pub post_bind_mutation_owners: BTreeSet<ReplayOwner>,
    pub plan_owners: BTreeSet<ReplayOwner>,
    pub scheduled_owners: BTreeSet<ReplayOwner>,
    pub schedule_omission_installed: bool,
    pub omitted_scheduled_owner: Option<ReplayOwner>,
    pub completion_or_semantic_query_steps: u64,
    pub fault_injected: u64,
    pub candidate_reservation_started: u64,
    pub candidate_activation_started: u64,
    pub baseline_validation_started: u64,
    pub disabled_baseline_owner: Option<ReplayOwner>,
    pub epoch_library_record_baselines: Vec<super::replay_index::ReplayBaselineRecord>,
    pub candidate_reserved_library_record_owners: BTreeSet<ReplayOwner>,
    pub candidate_activated_library_record_owners: BTreeSet<ReplayOwner>,
    pub failure: Option<PrivateReplayProductionFailureForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateReplayProductionFailureForTest {
    MutationOwnerOutsidePlan(ReplayOwner),
    RequiredSourceNotLoaded(LibraryFileOrdinal),
    BaselineMissing,
    Other,
}

#[cfg(any(test, feature = "test-utils"))]
struct PrivateReplayProductionControlForTest {
    fault: PrivateReplayProductionFaultForTest,
    next_event: u64,
    trace: PrivateReplayProductionTraceForTest,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateReplayProductionScopeForTest;

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplayProductionScopeForTest {
    pub fn start(fault: PrivateReplayProductionFaultForTest) -> Result<Self, &'static str> {
        PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
            if active.is_some() {
                return Err("private replay production scopes do not nest");
            }
            *active = Some(PrivateReplayProductionControlForTest {
                fault,
                next_event: 0,
                trace: PrivateReplayProductionTraceForTest::default(),
            });
            Ok(Self)
        })
    }

    pub fn finish(self) -> Result<PrivateReplayProductionTraceForTest, &'static str> {
        PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
            active
                .take()
                .map(|control| control.trace)
                .ok_or("private replay production scope is not active")
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn with_private_replay_production_control_for_test(
    update: impl FnOnce(&mut PrivateReplayProductionControlForTest),
) {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
        if let Some(active) = active.as_mut() {
            update(active);
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn private_replay_fault_for_test(
) -> PrivateReplayProductionFaultForTest {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow(|active| {
        active
            .as_ref()
            .map_or(PrivateReplayProductionFaultForTest::None, |active| {
                active.fault
            })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn private_replay_production_trace_active_for_test() -> bool {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow(|active| active.is_some())
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_private_replay_trace_for_test(
    update: impl FnOnce(&mut PrivateReplayProductionTraceForTest, u64),
) {
    with_private_replay_production_control_for_test(|active| {
        active.next_event = active.next_event.saturating_add(1);
        update(&mut active.trace, active.next_event);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub struct UserDeltaForkScopeForTest(u64);

#[cfg(any(test, feature = "test-utils"))]
impl UserDeltaForkScopeForTest {
    pub fn start() -> Self {
        Self(USER_DELTA_FORKS.get())
    }

    pub fn finish(self) -> u64 {
        USER_DELTA_FORKS.get().saturating_sub(self.0)
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserSourceWorkForTest {
    pub parses: u64,
    pub binds: u64,
    pub checks: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn user_source_work_for_test() -> UserSourceWorkForTest {
    UserSourceWorkForTest {
        parses: USER_SOURCE_PARSE_CALLS.get(),
        binds: USER_SOURCE_BIND_CALLS.get(),
        checks: USER_SOURCE_CHECK_CALLS.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_user_source_parses_for_test(count: usize) {
    USER_SOURCE_PARSE_CALLS.set(
        USER_SOURCE_PARSE_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_user_source_binds_for_test(count: usize) {
    USER_SOURCE_BIND_CALLS.set(
        USER_SOURCE_BIND_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_user_source_checks_for_test(count: usize) {
    USER_SOURCE_CHECK_CALLS.set(
        USER_SOURCE_CHECK_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub struct UserSourceWorkScopeForTest(UserSourceWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl UserSourceWorkScopeForTest {
    pub fn start() -> Self {
        Self(user_source_work_for_test())
    }

    pub fn finish(self) -> UserSourceWorkForTest {
        let end = user_source_work_for_test();
        UserSourceWorkForTest {
            parses: end.parses.saturating_sub(self.0.parses),
            binds: end.binds.saturating_sub(self.0.binds),
            checks: end.checks.saturating_sub(self.0.checks),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseFinalIdentityWitness {
    pub ends: OwnedBaseFinalIdentityEnds,
    pub actual_ids: OwnedBaseActualIds,
    pub named_alias_types: BTreeMap<String, TypeId>,
    pub local_names: BTreeSet<String>,
    pub references: OwnedBaseReferenceSummary,
    pub reused_base_shape: Option<OwnedBaseReusedShapeWitness>,
    pub base_row_clone_counts: BTreeMap<&'static str, u64>,
    pub local_rows_written: u64,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseActualIds {
    pub types: Vec<usize>,
    pub type_params: Vec<usize>,
    pub classes: Vec<usize>,
    pub scopes: Vec<usize>,
    pub symbols: Vec<usize>,
    pub declarations: Vec<usize>,
    pub type_groups: Vec<usize>,
    pub namespaces: Vec<usize>,
    pub value_storages: Vec<usize>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseReferenceSummary {
    pub base_to_delta: u64,
    pub delta_to_base: u64,
    pub delta_to_delta: u64,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseFinalIdentityEnds {
    pub store: usize,
    pub declared_recipes: usize,
    pub type_params: usize,
    pub classes: usize,
    pub scopes: usize,
    pub symbols: usize,
    pub declarations: usize,
    pub type_groups: usize,
    pub namespaces: usize,
    pub value_storages: usize,
    pub source_units: usize,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseReusedShapeWitness {
    pub type_id: TypeId,
    pub tag: TypeTag,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseContinuationWitness {
    pub base_store_len: usize,
    pub final_store_len: usize,
    pub base_type_group_count: usize,
    pub final_type_group_count: usize,
    pub base_decl_count: u32,
    pub final_decl_count: u32,
    pub source_key: u32,
    pub base_max_source_key: u32,
    pub array_group_stable: bool,
    pub document_value_stable: bool,
    pub store_prefix_stable: Option<bool>,
}

#[cfg(any(test, feature = "test-utils"))]
fn store_prefix_digest(
    store: &Store,
    type_len: usize,
    declared_recipe_len: usize,
) -> Result<String, String> {
    let mut bytes = CanonicalBytes::domain(b"typokat-owned-base-store-prefix-v1");
    bytes
        .usize(declared_recipe_len)
        .map_err(|error| format!("{error:?}"))?;
    for index in 0..declared_recipe_len {
        let recipe = crate::types::repr::DeclaredRecipeId(
            u32::try_from(index).map_err(|_| "declared recipe prefix overflow")?,
        );
        encode_declared_recipe_row(&mut bytes, store, recipe)
            .map_err(|error| format!("{error:?}"))?;
    }
    for index in 0..type_len {
        let id = TypeId(u32::try_from(index).map_err(|_| "type id prefix overflow")?);
        encode_store_row(&mut bytes, store, id).map_err(|error| format!("{error:?}"))?;
    }
    Ok(format!("{:x}", Sha256::digest(bytes.finish())))
}

#[cfg(any(test, feature = "test-utils"))]
struct FinalIdentityInspection<'a> {
    base_store_len: usize,
    base_value_storage_len: usize,
    base_type_group_len: usize,
    base_namespace_len: usize,
    binder: &'a Binder,
    published: &'a PublishedTypeEnvironment,
    interner: &'a Interner,
    decl_types: &'a DeclTypes,
    next_type_param: u32,
    next_class_id: u32,
    actual_class_ids: Vec<ClassId>,
    references: OwnedBaseReferenceSummary,
    base_row_clone_counts: BTreeMap<&'static str, u64>,
    local_rows_written: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn common_base_domain_limit(ends: &OwnedBaseFinalIdentityEnds, domain: u8) -> Option<usize> {
    match domain {
        1 => Some(ends.store),
        2 => Some(ends.type_params),
        3 => Some(ends.classes),
        4 => Some(ends.scopes),
        5 => Some(ends.symbols),
        6 => Some(ends.declarations),
        7 => Some(ends.type_groups),
        8 => Some(ends.namespaces),
        9 => Some(ends.value_storages),
        _ => None,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_live_reference_with_limits(
    summary: &mut OwnedBaseReferenceSummary,
    owner_limit: Option<usize>,
    owner: u32,
    target_limit: Option<usize>,
    target: u32,
) {
    let Some(target_limit) = target_limit else {
        return;
    };
    let owner_is_base = owner_limit
        .is_some_and(|owner_limit| usize::try_from(owner).is_ok_and(|owner| owner < owner_limit));
    let target_is_base = usize::try_from(target).is_ok_and(|target| target < target_limit);
    match (owner_is_base, target_is_base) {
        (true, false) => summary.base_to_delta += 1,
        (false, true) => summary.delta_to_base += 1,
        (false, false) => summary.delta_to_delta += 1,
        (true, true) => {}
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    classify_live_reference_with_limits(
        summary,
        common_base_domain_limit(base, owner_domain),
        owner,
        common_base_domain_limit(base, target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_interner_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    const DECLARED_RECIPE_DOMAIN: u8 = 10;
    let limit = |domain| {
        (domain == DECLARED_RECIPE_DOMAIN)
            .then_some(base.declared_recipes)
            .or_else(|| common_base_domain_limit(base, domain))
    };
    classify_live_reference_with_limits(
        summary,
        limit(owner_domain),
        owner,
        limit(target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_binder_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    const SOURCE_UNIT_DOMAIN: u8 = 10;
    let limit = |domain| {
        (domain == SOURCE_UNIT_DOMAIN)
            .then_some(base.source_units)
            .or_else(|| common_base_domain_limit(base, domain))
    };
    classify_live_reference_with_limits(
        summary,
        limit(owner_domain),
        owner,
        limit(target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
fn final_reference_summary<Ticket: Copy + PartialEq>(
    pass: &super::context::Pass<'_, '_, Ticket>,
    base: &OwnedBaseFinalIdentityEnds,
) -> OwnedBaseReferenceSummary {
    let mut summary = OwnedBaseReferenceSummary::default();
    for (owner_domain, target_domain, _, owner, target) in
        pass.interner.local_type_reference_records_for_test()
    {
        classify_interner_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner_domain, owner, target_domain, target) in
        pass.binder.local_reference_records_for_test()
    {
        classify_binder_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner, ty) in pass.decl_types.changed_slots() {
        classify_live_reference(&mut summary, base, 9, owner.0, 9, owner.0);
        if let Some(ty) = ty {
            classify_live_reference(&mut summary, base, 9, owner.0, 1, ty.0);
        }
    }

    let published = pass.type_environment.published();
    for (owner, terminal) in published.changed_group_terminals() {
        classify_live_reference(&mut summary, base, 7, owner.0, 7, owner.0);
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0)
            }
            PublishedTypeGroupSurface::Class(class) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 3, class.0)
            }
        }
        for parameter in &group.parameters {
            classify_live_reference(&mut summary, base, 7, owner.0, 2, parameter.0);
        }
        for default in &group.parameter_defaults {
            if let PublishedTypeParameterDefault::Ready(ty) = default {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
        for alternative in &group.conflict_alternatives {
            for ty in &alternative.types {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
    }
    for (class, terminal) in published.changed_class_terminals() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        let OwnedPublishedClassTerminal::Ready(surface) = &terminal else {
            continue;
        };
        classify_live_reference(&mut summary, base, 3, class.0, 3, surface.class().0);
        for parameter in surface.type_params() {
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.0);
        }
        for ty in [
            Some(surface.instance_template()),
            Some(surface.static_template()),
            surface.constructor_template(),
        ]
        .into_iter()
        .flatten()
        {
            classify_live_reference(&mut summary, base, 3, class.0, 1, ty.0);
        }
    }

    let namespace_terminals = pass
        .namespace_values
        .changed_terminal_snapshot_parts_for_test()
        .expect("completed changed namespace terminals snapshot");
    for row in namespace_terminals {
        classify_live_reference(&mut summary, base, 8, row.namespace.0, 8, row.namespace.0);
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 9, storage.0);
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 1, ty.0);
        }
    }

    for (&class, parameters) in pass.class_application_parameters.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        for parameter in parameters {
            let parameter = (*parameter).snapshot_parts();
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.id.0);
            if let Some(constraint) = parameter.constraint {
                classify_live_reference(&mut summary, base, 3, class.0, 1, constraint.0);
            }
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                classify_live_reference(&mut summary, base, 3, class.0, 1, default.0);
            }
        }
    }
    for (&class, metadata) in pass.class_new_metadata.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        classify_live_reference(
            &mut summary,
            base,
            3,
            class.0,
            3,
            metadata.ctor_declaring_class.0,
        );
    }
    for (&class, &parent) in pass.class_parents.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, parent.0);
    }
    for (&alias, &target) in pass.class_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for (&storage, binding) in pass.class_value_bindings.local_iter() {
        classify_live_reference(&mut summary, base, 9, storage.0, 3, binding.class_id.0);
    }
    for (&alias, &target) in pass.standalone_namespace_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for &symbol in pass.named_function_symbols.local_iter() {
        classify_live_reference(&mut summary, base, 5, symbol.0, 5, symbol.0);
    }
    for (&class, _) in pass.class_names.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
    }
    if let Some(identities) = &pass.library_semantic_identities {
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                continue;
            };
            for (domain, target) in std::iter::once((7, identity.group.0))
                .chain(std::iter::once((1, identity.template.0)))
                .chain(identity.parameters.iter().map(|parameter| (2, parameter.0)))
            {
                let Some(limit) = common_base_domain_limit(base, domain) else {
                    continue;
                };
                if usize::try_from(target).is_ok_and(|target| target >= limit) {
                    summary.base_to_delta += 1;
                }
            }
        }
    }
    summary
}

#[cfg(any(test, feature = "test-utils"))]
fn final_identity_witness(inputs: FinalIdentityInspection<'_>) -> OwnedBaseFinalIdentityWitness {
    let FinalIdentityInspection {
        base_store_len,
        base_value_storage_len,
        base_type_group_len,
        base_namespace_len,
        binder,
        published,
        interner,
        decl_types,
        next_type_param,
        next_class_id,
        actual_class_ids,
        references,
        base_row_clone_counts,
        local_rows_written,
    } = inputs;
    let mut named_alias_types = BTreeMap::new();
    let mut local_names = BTreeSet::new();
    let base_type_group_end =
        u32::try_from(base_type_group_len).expect("base type-group end fits u32");
    let base_namespace_end =
        u32::try_from(base_namespace_len).expect("base namespace end fits u32");
    let mut pending_scopes = vec![(binder.module, String::new())];
    let mut visited_namespaces = BTreeSet::new();
    while let Some((scope_id, prefix)) = pending_scopes.pop() {
        let Some(scope) = binder.graph.get(scope_id) else {
            continue;
        };
        for (name, symbol_id) in &scope.symbols {
            let qualified = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            local_names.insert(qualified.clone());
            let Some(symbol) = binder.symbols.get(*symbol_id) else {
                continue;
            };
            if let Some(group) = symbol.ty.filter(|group| group.0 >= base_type_group_end) {
                let alias = binder.type_groups.get(group).is_some_and(|declaration| {
                    declaration
                        .fragments
                        .iter()
                        .any(|fragment| fragment.kind == TypeFragmentKind::TypeAlias)
                });
                if alias {
                    if let Some(PublishedTypeGroupTerminal::Ready(ready)) =
                        published.groups().get(group)
                    {
                        if let PublishedTypeGroupSurface::Template(ty) = ready.surface {
                            named_alias_types.insert(qualified.clone(), ty);
                        }
                    }
                }
            }
            if let Some(namespace) = symbol
                .ns
                .filter(|namespace| namespace.0 >= base_namespace_end)
                .filter(|namespace| visited_namespaces.insert(*namespace))
            {
                if let Some(public_scope) = binder
                    .namespaces
                    .get(namespace)
                    .map(|namespace| namespace.public_scope)
                {
                    pending_scopes.push((public_scope, qualified));
                }
            }
        }
    }

    let eligible_base_shape = |ty: TypeId| {
        (ty.index() < base_store_len)
            .then(|| interner.store().tag(ty))
            .filter(|tag| {
                matches!(
                    tag,
                    TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
                )
            })
            .map(|tag| OwnedBaseReusedShapeWitness { type_id: ty, tag })
    };
    let direct_user_shape = named_alias_types
        .values()
        .copied()
        .chain(
            (base_value_storage_len..decl_types.len())
                .map(|index| ValueStorageId(u32::try_from(index).expect("storage id fits u32")))
                .filter_map(|storage| decl_types.get(storage)),
        )
        .find_map(eligible_base_shape);
    let reused_base_shape = direct_user_shape.or_else(|| {
        const TYPE_DOMAIN: u8 = 1;
        interner
            .local_type_reference_records_for_test()
            .into_iter()
            .find_map(|(owner_domain, target_domain, _, owner, target)| {
                (owner_domain == TYPE_DOMAIN
                    && target_domain == TYPE_DOMAIN
                    && usize::try_from(owner).ok()? >= base_store_len
                    && usize::try_from(target).ok()? < base_store_len)
                    .then_some(TypeId(target))
                    .and_then(eligible_base_shape)
            })
    });

    OwnedBaseFinalIdentityWitness {
        ends: OwnedBaseFinalIdentityEnds {
            store: interner.store().len(),
            declared_recipes: interner.store().all_declared_recipes().count(),
            type_params: usize::try_from(next_type_param).expect("type parameter end fits usize"),
            classes: usize::try_from(next_class_id).expect("class end fits usize"),
            scopes: binder.graph.len(),
            symbols: binder.symbols.len(),
            declarations: binder.declarations.len(),
            type_groups: binder.type_groups.len(),
            namespaces: binder.namespaces.len(),
            value_storages: decl_types.len(),
            source_units: binder.checkpoint_ends().next_source,
        },
        actual_ids: OwnedBaseActualIds {
            types: interner
                .store()
                .local_type_ids_for_test()
                .map(TypeId::index)
                .collect(),
            type_params: interner
                .store()
                .local_type_param_ids_for_test()
                .map(|parameter| {
                    usize::try_from(parameter.0).expect("type parameter id fits usize")
                })
                .collect(),
            classes: actual_class_ids
                .into_iter()
                .map(|class| usize::try_from(class.0).expect("class id fits usize"))
                .collect(),
            scopes: binder
                .graph
                .local_scopes()
                .map(|(scope, _)| usize::try_from(scope.0).expect("scope id fits usize"))
                .collect(),
            symbols: binder
                .symbols
                .local_symbols()
                .map(|(symbol, _)| usize::try_from(symbol.0).expect("symbol id fits usize"))
                .collect(),
            declarations: binder
                .declarations
                .local_declarations()
                .map(|declaration| {
                    usize::try_from(declaration.id.0).expect("declaration id fits usize")
                })
                .collect(),
            type_groups: binder
                .type_groups
                .local_groups()
                .map(|group| usize::try_from(group.id.0).expect("type group id fits usize"))
                .collect(),
            namespaces: binder
                .namespaces
                .local_namespaces()
                .map(|(namespace, _)| {
                    usize::try_from(namespace.0).expect("namespace id fits usize")
                })
                .collect(),
            value_storages: decl_types
                .local_slots()
                .map(|(storage, _)| {
                    usize::try_from(storage.0).expect("value storage id fits usize")
                })
                .collect(),
        },
        named_alias_types,
        local_names,
        references,
        reused_base_shape,
        base_row_clone_counts,
        local_rows_written,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InjectedLibrarySource<'source> {
    pub file_ordinal: LibraryFileOrdinal,
    pub name: &'source str,
    pub source: &'source str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InjectedProfileError {
    EmptyProfile,
    EmptyName {
        file_ordinal: LibraryFileOrdinal,
    },
    DuplicateName(String),
    DuplicateFileOrdinal(LibraryFileOrdinal),
    SourceKeyOverflow,
    Parse {
        file_ordinal: LibraryFileOrdinal,
        messages: Vec<String>,
    },
    Binder(String),
    Reservation(String),
    LibraryIdentitySelectionPending,
    Reporting(LibraryEventLedgerError),
    ReplayIndex(ReplayIndexGenerationError),
    ReplayIndexAdmission(ReplayIndexAdmissionError),
    CanonicalProjection(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryPhaseCounts {
    pub parse_units: usize,
    pub bind_units: usize,
    pub reserved_records: usize,
    pub filled_records: usize,
    pub publication_validations: usize,
    pub statement_check_units: usize,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryPhaseTimings {
    pub parse: Duration,
    pub bind: Duration,
    pub reserve_fill: Duration,
    pub publication_validation: Duration,
    pub statement_check: Duration,
}

#[cfg(any(test, feature = "test-utils"))]
impl LibraryPhaseTimings {
    fn measured_total(&self) -> Duration {
        self.parse
            + self.bind
            + self.reserve_fill
            + self.publication_validation
            + self.statement_check
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeProbe {
    pub identity: TypeGroupId,
    pub declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)>,
    pub declaration_count: usize,
    pub member_names: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureProbe {
    pub parameter_types: Vec<String>,
    pub return_type: String,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableMemberProbe {
    pub name: String,
    pub identity: ValueStorageId,
    pub source: ExactUnit,
    pub reservation_source: ExactUnit,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueProbe {
    pub identity: ValueStorageId,
    visible_type: Option<TypeId>,
    pub participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)>,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub member_names: Vec<String>,
    pub callable_members: Vec<CallableMemberProbe>,
}

#[derive(Debug)]
pub struct InjectedProfileRun {
    pub phase_counts: LibraryPhaseCounts,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_store_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_published_type_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_replacement_type_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_type_param_id: u32,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_class_id: u32,
    #[cfg(any(test, feature = "test-utils"))]
    pub phase_timings: LibraryPhaseTimings,
    #[cfg(any(test, feature = "test-utils"))]
    pub reserved_file_ordinals: Vec<LibraryFileOrdinal>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) reporting_receipts: Vec<LibraryReportingReceipt>,
    /// The library's own records — empty unless the caller asked for
    /// [`LibraryRecordRetention::Collect`]. Nothing downstream of a compile retains them
    /// (ADR-0018).
    pub library_records: Vec<(LibraryEventKey, CheckerRecord)>,
    user_results: Vec<super::CheckResult>,
    #[cfg(any(test, feature = "test-utils"))]
    ordered_user_records: Vec<(LibraryFileOrdinal, CheckerRecord)>,
    #[cfg(any(test, feature = "test-utils"))]
    pub pass_source_units: Vec<ExactUnit>,
    #[cfg(any(test, feature = "test-utils"))]
    pub lexical_source_units: Vec<ExactUnit>,
    #[cfg(any(test, feature = "test-utils"))]
    global_types: BTreeMap<String, TypeProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    module_types: BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    global_values: BTreeMap<String, ValueProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    semantic_identities: LibrarySemanticIdentities,
}

/// The canonical byte projection of the library's own records.
///
/// This is artifact identity, not semantic output: the records themselves are what
/// ADR-0011 requires be preserved exactly, and they reach the base untouched. Nothing in a
/// compile reads these blobs, so the suite — not every process — projects them (ADR-0017).
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalLibraryEvidence {
    pub diagnostics: Vec<u8>,
    pub incompletes: Vec<u8>,
    pub ledger: Vec<u8>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompleteSourceRouteWorkForTest {
    pub profile_loads: u64,
    pub library_parse_units: u64,
    pub library_bind_units: u64,
    pub user_bind_units: u64,
    pub semantic_publications: u64,
    pub replay_trace_constructions: u64,
    pub replay_plan_constructions: u64,
    pub frozen_base_seals: u64,
    pub library_source_reparses: u64,
    pub library_evidence: Option<CanonicalLibraryEvidence>,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static COMPLETE_SOURCE_ROUTE_WORK: RefCell<CompleteSourceRouteWorkForTest> =
        RefCell::new(CompleteSourceRouteWorkForTest::default());
    static COMPLETE_SOURCE_EVIDENCE_GENERATION: Cell<u64> = const { Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
fn record_complete_source_route_work_for_test(
    record: impl FnOnce(&mut CompleteSourceRouteWorkForTest),
) {
    COMPLETE_SOURCE_ROUTE_WORK.with(|work| record(&mut work.borrow_mut()));
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_complete_source_profile_load_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.profile_loads = work.profile_loads.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_complete_source_auxiliary_parse_work_for_test(
    parser_invocations: u64,
    source_reparses: u64,
) {
    record_complete_source_route_work_for_test(|work| {
        work.library_parse_units = work.library_parse_units.saturating_add(parser_invocations);
        work.library_source_reparses = work.library_source_reparses.saturating_add(source_reparses);
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn record_complete_source_bind_for_test(library_count: usize, user_count: usize) {
    record_complete_source_route_work_for_test(|work| {
        work.library_bind_units = work
            .library_bind_units
            .saturating_add(u64::try_from(library_count).unwrap_or(u64::MAX));
        work.user_bind_units = work
            .user_bind_units
            .saturating_add(u64::try_from(user_count).unwrap_or(u64::MAX));
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_complete_source_publication_entry_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.semantic_publications = work.semantic_publications.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_replay_trace_construction_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.replay_trace_constructions = work.replay_trace_constructions.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_replay_plan_construction_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.replay_plan_constructions = work.replay_plan_constructions.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
fn record_complete_source_evidence_for_test(evidence: CanonicalLibraryEvidence) {
    record_complete_source_route_work_for_test(|work| work.library_evidence = Some(evidence));
    COMPLETE_SOURCE_EVIDENCE_GENERATION
        .set(COMPLETE_SOURCE_EVIDENCE_GENERATION.get().saturating_add(1));
}

#[cfg(not(any(test, feature = "test-utils")))]
pub fn record_complete_source_profile_load_for_test() {}

#[cfg(any(test, feature = "test-utils"))]
pub fn complete_source_route_thread_receipt_for_test() -> CompleteSourceRouteWorkForTest {
    COMPLETE_SOURCE_ROUTE_WORK.with(|work| work.borrow().clone())
}

#[cfg(any(test, feature = "test-utils"))]
pub fn merge_complete_source_route_thread_receipt_for_test(
    receipt: CompleteSourceRouteWorkForTest,
) {
    record_complete_source_route_work_for_test(|work| {
        work.profile_loads = work.profile_loads.saturating_add(receipt.profile_loads);
        work.library_parse_units = work
            .library_parse_units
            .saturating_add(receipt.library_parse_units);
        work.library_bind_units = work
            .library_bind_units
            .saturating_add(receipt.library_bind_units);
        work.user_bind_units = work.user_bind_units.saturating_add(receipt.user_bind_units);
        work.semantic_publications = work
            .semantic_publications
            .saturating_add(receipt.semantic_publications);
        work.replay_trace_constructions = work
            .replay_trace_constructions
            .saturating_add(receipt.replay_trace_constructions);
        work.replay_plan_constructions = work
            .replay_plan_constructions
            .saturating_add(receipt.replay_plan_constructions);
        work.frozen_base_seals = work
            .frozen_base_seals
            .saturating_add(receipt.frozen_base_seals);
        work.library_source_reparses = work
            .library_source_reparses
            .saturating_add(receipt.library_source_reparses);
        if receipt.library_evidence.is_some() {
            work.library_evidence = receipt.library_evidence;
            COMPLETE_SOURCE_EVIDENCE_GENERATION
                .set(COMPLETE_SOURCE_EVIDENCE_GENERATION.get().saturating_add(1));
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub struct CompleteSourceRouteWorkScopeForTest {
    start: CompleteSourceRouteWorkForTest,
    evidence_generation: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl CompleteSourceRouteWorkScopeForTest {
    pub fn start() -> Self {
        Self {
            start: complete_source_route_thread_receipt_for_test(),
            evidence_generation: COMPLETE_SOURCE_EVIDENCE_GENERATION.get(),
        }
    }

    pub fn finish(self) -> CompleteSourceRouteWorkForTest {
        let end = complete_source_route_thread_receipt_for_test();
        let evidence_changed =
            COMPLETE_SOURCE_EVIDENCE_GENERATION.get() != self.evidence_generation;
        CompleteSourceRouteWorkForTest {
            profile_loads: end.profile_loads.saturating_sub(self.start.profile_loads),
            library_parse_units: end
                .library_parse_units
                .saturating_sub(self.start.library_parse_units),
            library_bind_units: end
                .library_bind_units
                .saturating_sub(self.start.library_bind_units),
            user_bind_units: end
                .user_bind_units
                .saturating_sub(self.start.user_bind_units),
            semantic_publications: end
                .semantic_publications
                .saturating_sub(self.start.semantic_publications),
            replay_trace_constructions: end
                .replay_trace_constructions
                .saturating_sub(self.start.replay_trace_constructions),
            replay_plan_constructions: end
                .replay_plan_constructions
                .saturating_sub(self.start.replay_plan_constructions),
            frozen_base_seals: end
                .frozen_base_seals
                .saturating_sub(self.start.frozen_base_seals),
            library_source_reparses: end
                .library_source_reparses
                .saturating_sub(self.start.library_source_reparses),
            library_evidence: evidence_changed.then_some(end.library_evidence).flatten(),
        }
    }
}

impl InjectedProfileRun {
    #[cfg(any(test, feature = "test-utils"))]
    pub fn into_complete_source_user_results_for_test(self) -> Vec<super::CheckResult> {
        self.user_results
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn semantic_identities(&self) -> &LibrarySemanticIdentities {
        &self.semantic_identities
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn global_type_probe(&self, name: &str) -> Option<&TypeProbe> {
        self.global_types.get(name)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn module_type_probe(
        &self,
        file_ordinal: LibraryFileOrdinal,
        name: &str,
    ) -> Option<&TypeProbe> {
        self.module_types.get(&(file_ordinal, name.to_owned()))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn global_value_probe(&self, name: &str) -> Option<&ValueProbe> {
        self.global_values.get(name)
    }
}

struct CanonicalInput<'source> {
    file_ordinal: LibraryFileOrdinal,
    source: &'source str,
    kind: SourceFileKind,
    source_key: ExactKey,
    origin: CompilationOrigin,
}

#[cfg(any(test, feature = "test-utils"))]
fn independent_event_owner_sites_for_oracle(
    canonical: &[CanonicalInput<'_>],
    parsed: &[&Program<'_>],
    binder: &Binder,
) -> Result<Vec<CollisionReplayOwnerSite>, InjectedProfileError> {
    let mut allocator = IndependentEventOwnerSiteOracle::new();
    let mut reservations: LexicalReservations<(usize, usize)> = LexicalReservations::default();
    for (input, program) in canonical.iter().zip(parsed) {
        allocator.select_file(input.file_ordinal);
        reservations
            .reserve_program_with(program, &mut allocator)
            .map_err(|message| InjectedProfileError::Reservation(message.to_owned()))?;
        allocator.reserve_exact_site(
            Span::from_oxc(program.span),
            CollisionReplayEventPhase::Immediate,
        );
    }
    for descriptor in independent_library_reporting_site_descriptors(binder) {
        allocator.select_file(descriptor.file_ordinal);
        allocator.reserve_exact_site(descriptor.span, CollisionReplayEventPhase::Immediate);
    }
    allocator.finish()
}

#[cfg(any(test, feature = "test-utils"))]
fn verify_independent_event_owner_sites(
    plan: &CollisionReplayPlan,
    independent: &[CollisionReplayOwnerSite],
) -> Result<(), InjectedProfileError> {
    let captured = plan
        .full_oracle_snapshot_for_test()
        .owner_sites
        .iter()
        .filter(|site| matches!(site.provenance, CollisionReplaySiteProvenance::Event { .. }))
        .cloned()
        .collect::<Vec<_>>();
    if captured != independent {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::IndependentOwnerSiteOracleMismatch,
        ));
    }
    Ok(())
}

struct CanonicalLibraryFrontend<'source, 'ast> {
    canonical: Vec<CanonicalInput<'source>>,
    parsed: Vec<&'ast Program<'ast>>,
    binder: Binder,
    module_scopes: Vec<ScopeId>,
    semantic_scopes: Vec<ScopeId>,
    user_start: Option<usize>,
    user_units: Vec<(ModuleOrdinal, UnitSlot)>,
    user_events: EventStore,
    combined_lexical_events: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    external_effects: BTreeMap<UserRecordTicket, CandidateEffects>,
    module_placeholders: Vec<Vec<ImportPlaceholder>>,
    collision_root_provenance: LibraryRootProjection,
    #[cfg(any(test, feature = "test-utils"))]
    parse_elapsed: Duration,
    #[cfg(any(test, feature = "test-utils"))]
    bind_elapsed: Duration,
}

#[derive(Clone, Copy)]
enum TrustedLibraryMarkerShape {
    IntrinsicAlias,
    ConditionalAlias,
    EmptyInterface,
}

fn exact_type_parameter(
    declaration: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
    name: &str,
    constraint: Option<fn(&oxc_ast::ast::TSType<'_>) -> bool>,
) -> bool {
    let Some(parameter) = declaration
        .and_then(|declaration| (declaration.params.len() == 1).then(|| &declaration.params[0]))
    else {
        return false;
    };
    parameter.name.name == name
        && parameter.default.is_none()
        && !parameter.r#in
        && !parameter.out
        && !parameter.r#const
        && match (parameter.constraint.as_ref(), constraint) {
            (None, None) => true,
            (Some(actual), Some(expected)) => expected(actual),
            _ => false,
        }
}

fn is_string_keyword(ty: &oxc_ast::ast::TSType<'_>) -> bool {
    matches!(ty, oxc_ast::ast::TSType::TSStringKeyword(_))
}

fn exact_plain_type_reference(ty: &oxc_ast::ast::TSType<'_>, name: &str) -> bool {
    let oxc_ast::ast::TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    matches!(
        &reference.type_name,
        oxc_ast::ast::TSTypeName::IdentifierReference(identifier)
            if identifier.name == name
    ) && reference.type_arguments.is_none()
}

fn exact_unary_type_reference(
    ty: &oxc_ast::ast::TSType<'_>,
    name: &str,
    argument_name: &str,
) -> bool {
    let oxc_ast::ast::TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    let Some(arguments) = reference.type_arguments.as_ref() else {
        return false;
    };
    matches!(
        &reference.type_name,
        oxc_ast::ast::TSTypeName::IdentifierReference(identifier)
            if identifier.name == name
    ) && arguments.params.len() == 1
        && exact_plain_type_reference(&arguments.params[0], argument_name)
}

fn exact_infer_type(ty: &oxc_ast::ast::TSType<'_>, name: &str) -> bool {
    let oxc_ast::ast::TSType::TSInferType(infer) = ty else {
        return false;
    };
    let parameter = &infer.type_parameter;
    parameter.name.name == name
        && parameter.constraint.is_none()
        && parameter.default.is_none()
        && !parameter.r#in
        && !parameter.out
        && !parameter.r#const
}

fn exact_rest_function(
    ty: &oxc_ast::ast::TSType<'_>,
    rest_name: &str,
    rest_type: fn(&oxc_ast::ast::TSType<'_>) -> bool,
    return_type: fn(&oxc_ast::ast::TSType<'_>) -> bool,
) -> bool {
    let oxc_ast::ast::TSType::TSFunctionType(function) = ty else {
        return false;
    };
    let Some(rest) = function.params.rest.as_ref() else {
        return false;
    };
    let Some(rest_annotation) = rest.type_annotation.as_ref() else {
        return false;
    };
    matches!(
        &rest.rest.argument,
        oxc_ast::ast::BindingPattern::BindingIdentifier(identifier)
            if identifier.name == rest_name
    ) && function.type_parameters.is_none()
        && function.this_param.is_none()
        && function.params.kind == oxc_ast::ast::FormalParameterKind::Signature
        && function.params.items.is_empty()
        && rest.decorators.is_empty()
        && rest_type(&rest_annotation.type_annotation)
        && return_type(&function.return_type.type_annotation)
}

fn exact_omit_this_parameter_body(annotation: &oxc_ast::ast::TSType<'_>) -> bool {
    fn infer_a(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_infer_type(ty, "A")
    }
    fn infer_r(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_infer_type(ty, "R")
    }
    fn reference_a(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_plain_type_reference(ty, "A")
    }
    fn reference_r(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_plain_type_reference(ty, "R")
    }

    let oxc_ast::ast::TSType::TSConditionalType(outer) = annotation else {
        return false;
    };
    let oxc_ast::ast::TSType::TSConditionalType(inner) = &outer.false_type else {
        return false;
    };
    matches!(outer.check_type, oxc_ast::ast::TSType::TSUnknownKeyword(_))
        && exact_unary_type_reference(&outer.extends_type, "ThisParameterType", "T")
        && exact_plain_type_reference(&outer.true_type, "T")
        && exact_plain_type_reference(&inner.check_type, "T")
        && exact_rest_function(&inner.extends_type, "args", infer_a, infer_r)
        && exact_rest_function(&inner.true_type, "args", reference_a, reference_r)
        && exact_plain_type_reference(&inner.false_type, "T")
}

fn seed_trusted_library_markers(
    roots: &LibraryRootProjection,
    declarations: &mut super::context::TypeDeclTable<'_>,
    resolved: &mut super::context::TypeResolvedTable,
    interner: &Interner,
) {
    let well_known = interner.well_known();
    for (name, marker, shape) in [
        (
            "Uppercase",
            well_known.uppercase,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Lowercase",
            well_known.lowercase,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Capitalize",
            well_known.capitalize,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Uncapitalize",
            well_known.uncapitalize,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "ThisType",
            well_known.this_type,
            TrustedLibraryMarkerShape::EmptyInterface,
        ),
        (
            "OmitThisParameter",
            well_known.omit_this_parameter,
            TrustedLibraryMarkerShape::ConditionalAlias,
        ),
    ] {
        let Some(group) = roots
            .root_rows
            .binary_search_by(|row| row.name.as_str().cmp(name))
            .ok()
            .and_then(|index| roots.root_rows.get(index))
            .and_then(|row| row.ty)
        else {
            continue;
        };
        let trusted_shape = match (shape, declarations.get(group.index())) {
            (
                TrustedLibraryMarkerShape::IntrinsicAlias,
                Some(super::context::TypeDecl::Alias {
                    annotation,
                    param_decl,
                    name: declaration_name,
                    ..
                }),
            ) if declaration_name == name
                && exact_type_parameter(*param_decl, "S", Some(is_string_keyword))
                && matches!(annotation, oxc_ast::ast::TSType::TSIntrinsicKeyword(_)) =>
            {
                true
            }
            (
                TrustedLibraryMarkerShape::ConditionalAlias,
                Some(super::context::TypeDecl::Alias {
                    annotation,
                    param_decl,
                    name: declaration_name,
                    ..
                }),
            ) if declaration_name == name
                && exact_type_parameter(*param_decl, "T", None)
                && exact_omit_this_parameter_body(annotation) =>
            {
                true
            }
            (
                TrustedLibraryMarkerShape::EmptyInterface,
                Some(super::context::TypeDecl::Interface {
                    param_decl,
                    extends,
                    fragments,
                    ..
                }),
            ) if exact_type_parameter(*param_decl, "T", None)
                && extends.is_empty()
                && fragments.len() == 1
                && fragments
                    .iter()
                    .all(|fragment| fragment.members.is_empty() && fragment.extends.is_empty()) =>
            {
                true
            }
            _ => false,
        };
        if !trusted_shape {
            continue;
        }
        if let Some(slot) = resolved.get_mut(group.index()) {
            *slot = Some(marker);
        }
    }
}

fn certify_library_values(roots: &LibraryRootProjection) -> CertifiedLibraryValues {
    let symbol = roots
        .root_rows
        .binary_search_by(|row| row.name.as_str().cmp("Symbol"))
        .ok()
        .and_then(|index| roots.root_rows.get(index))
        .and_then(|row| row.value);
    CertifiedLibraryValues { symbol }
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static CANONICAL_FRONTEND_ENTRIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_PARSE_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_BIND_BATCHES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_BIND_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_FULL_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanonicalLibraryFrontendWorkForTest {
    pub entries: u64,
    pub parse_units: u64,
    pub bind_batches: u64,
    pub bind_units: u64,
    pub full_source_products_consumed: u64,
    pub checkpoint_products_consumed: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn canonical_library_frontend_work_for_test() -> CanonicalLibraryFrontendWorkForTest {
    CanonicalLibraryFrontendWorkForTest {
        entries: CANONICAL_FRONTEND_ENTRIES.get(),
        parse_units: CANONICAL_FRONTEND_PARSE_UNITS.get(),
        bind_batches: CANONICAL_FRONTEND_BIND_BATCHES.get(),
        bind_units: CANONICAL_FRONTEND_BIND_UNITS.get(),
        full_source_products_consumed: CANONICAL_FRONTEND_FULL_PRODUCTS.get(),
        checkpoint_products_consumed: CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub struct CanonicalLibraryFrontendWorkScopeForTest(CanonicalLibraryFrontendWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl CanonicalLibraryFrontendWorkScopeForTest {
    pub fn start() -> Self {
        Self(canonical_library_frontend_work_for_test())
    }

    pub fn finish(self) -> CanonicalLibraryFrontendWorkForTest {
        let end = canonical_library_frontend_work_for_test();
        CanonicalLibraryFrontendWorkForTest {
            entries: end.entries.saturating_sub(self.0.entries),
            parse_units: end.parse_units.saturating_sub(self.0.parse_units),
            bind_batches: end.bind_batches.saturating_sub(self.0.bind_batches),
            bind_units: end.bind_units.saturating_sub(self.0.bind_units),
            full_source_products_consumed: end
                .full_source_products_consumed
                .saturating_sub(self.0.full_source_products_consumed),
            checkpoint_products_consumed: end
                .checkpoint_products_consumed
                .saturating_sub(self.0.checkpoint_products_consumed),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ParserExportClaim {
    file_ordinal: LibraryFileOrdinal,
    span: Span,
}

struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    fn domain(domain: &[u8]) -> Self {
        Self(Vec::from(domain))
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) -> Result<(), InjectedProfileError> {
        self.u64(u64::try_from(value).map_err(|_| {
            InjectedProfileError::CanonicalProjection("usize does not fit u64".to_owned())
        })?);
        Ok(())
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), InjectedProfileError> {
        self.usize(value.len())?;
        self.0.extend_from_slice(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), InjectedProfileError> {
        self.bytes(value.as_bytes())
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn property_key(&mut self, key: &PropertyKey) -> Result<(), InjectedProfileError> {
        match key {
            PropertyKey::String(name) => {
                self.byte(0);
                self.string(name)
            }
            PropertyKey::WellKnownSymbol(symbol) => {
                self.byte(1);
                self.byte(well_known_symbol_code(*symbol));
                Ok(())
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn type_id(&mut self, value: TypeId) {
        self.u32(value.0);
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn optional_type_id(&mut self, value: Option<TypeId>) {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.type_id(value);
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn type_ids(&mut self, values: &[TypeId]) -> Result<(), InjectedProfileError> {
        self.usize(values.len())?;
        for value in values {
            self.type_id(*value);
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn intrinsic_code(kind: IntrinsicKind) -> u8 {
    match kind {
        IntrinsicKind::Error => 0,
        IntrinsicKind::Any => 1,
        IntrinsicKind::Unknown => 2,
        IntrinsicKind::Never => 3,
        IntrinsicKind::Void => 4,
        IntrinsicKind::Null => 5,
        IntrinsicKind::Undefined => 6,
        IntrinsicKind::Boolean => 7,
        IntrinsicKind::Number => 8,
        IntrinsicKind::String => 9,
        IntrinsicKind::Uppercase => 10,
        IntrinsicKind::Lowercase => 11,
        IntrinsicKind::Capitalize => 12,
        IntrinsicKind::Uncapitalize => 13,
        IntrinsicKind::ThisType => 14,
        IntrinsicKind::OmitThisParameter => 15,
        IntrinsicKind::Object => 16,
        IntrinsicKind::BigInt => 17,
        IntrinsicKind::Symbol => 18,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn visibility_code(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 0,
        Visibility::Private => 1,
        Visibility::Protected => 2,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn well_known_symbol_code(symbol: WellKnownSymbol) -> u8 {
    match symbol {
        WellKnownSymbol::Iterator => 0,
        WellKnownSymbol::ToStringTag => 1,
        WellKnownSymbol::AsyncIterator => 2,
        WellKnownSymbol::Species => 3,
        WellKnownSymbol::ToPrimitive => 4,
        WellKnownSymbol::Replace => 5,
        WellKnownSymbol::Unscopables => 6,
        WellKnownSymbol::Split => 7,
        WellKnownSymbol::Search => 8,
        WellKnownSymbol::Match => 9,
        WellKnownSymbol::MatchAll => 10,
        WellKnownSymbol::HasInstance => 11,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn modifier_code(modifier: ModifierOp) -> u8 {
    match modifier {
        ModifierOp::Keep => 0,
        ModifierOp::Add => 1,
        ModifierOp::Remove => 2,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn encode_store_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: TypeId,
) -> Result<(), InjectedProfileError> {
    let tag = store.tag(id);
    bytes.type_id(id);
    bytes.byte(tag.discriminant());
    bytes.u32(store.flags(id).0);
    match tag {
        TypeTag::Intrinsic => bytes.byte(intrinsic_code(store.intrinsic_kind(id).ok_or_else(
            || InjectedProfileError::CanonicalProjection("missing intrinsic payload".to_owned()),
        )?)),
        TypeTag::Literal => match store.literal_value(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing literal payload".to_owned())
        })? {
            LiteralValue::Number(value) => {
                bytes.byte(0);
                bytes.u64(value.to_bits());
            }
            LiteralValue::String(value) => {
                bytes.byte(1);
                bytes.string(value)?;
            }
            LiteralValue::Boolean(value) => {
                bytes.byte(2);
                bytes.bool(*value);
            }
            LiteralValue::WellKnownSymbol(symbol) => {
                bytes.byte(3);
                bytes.byte(well_known_symbol_code(*symbol));
            }
        },
        TypeTag::Object => {
            let object = store.object_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing object payload".to_owned())
            })?;
            bytes.usize(object.properties.len())?;
            for property in &object.properties {
                bytes.property_key(&property.key)?;
                bytes.type_id(property.ty);
                bytes.optional_type_id(property.write_ty);
                bytes.bool(property.optional);
                bytes.byte(visibility_code(property.visibility));
                bytes.bool(property.declaring_class.is_some());
                if let Some(class) = property.declaring_class {
                    bytes.u32(class.0);
                }
                bytes.bool(property.readonly);
                bytes.bool(property.is_accessor);
            }
            bytes.optional_type_id(object.string_index);
            bytes.optional_type_id(object.number_index);
            bytes.type_ids(&object.call_signatures)?;
            bytes.type_ids(&object.construct_signatures)?;
        }
        TypeTag::Union => bytes.type_ids(store.union_members(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing union payload".to_owned())
        })?)?,
        TypeTag::Intersection => {
            bytes.type_ids(store.intersection_members(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing intersection payload".to_owned())
            })?)?
        }
        TypeTag::Function => {
            let function = store.function_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing function payload".to_owned())
            })?;
            bytes.usize(function.type_params.len())?;
            for parameter in &function.type_params {
                bytes.u32(parameter.id.0);
                bytes.optional_type_id(parameter.constraint);
                bytes.optional_type_id(parameter.default);
            }
            bytes.optional_type_id(function.receiver);
            bytes.usize(function.params.len())?;
            for parameter in &function.params {
                bytes.string(&parameter.name)?;
                bytes.type_id(parameter.ty);
                bytes.bool(parameter.optional);
                bytes.bool(parameter.has_default);
                bytes.bool(parameter.rest);
            }
            bytes.type_id(function.ret);
        }
        TypeTag::TypeParam => {
            let parameter = store.type_param(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing type parameter".to_owned())
            })?;
            bytes.u32(parameter.id.0);
            bytes.string(&parameter.name)?;
            bytes.optional_type_id(store.type_param_constraint(parameter.id));
            bytes.bool(store.type_param_metadata_is_frozen(parameter.id));
        }
        TypeTag::Array => bytes.type_id(
            store
                .array_type(id)
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection("missing array payload".to_owned())
                })?
                .element,
        ),
        TypeTag::Tuple => {
            let tuple = store.tuple_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing tuple payload".to_owned())
            })?;
            bytes.type_ids(&tuple.elements)?;
            bytes.bool(tuple.rest.is_some());
            if let Some(rest) = tuple.rest {
                bytes.usize(rest.position)?;
                bytes.type_id(rest.ty);
            }
        }
        TypeTag::Readonly => bytes.type_id(store.readonly_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing readonly operand".to_owned())
        })?),
        TypeTag::Conditional => {
            let conditional = store.conditional_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing conditional".to_owned())
            })?;
            bytes.type_id(conditional.check);
            bytes.type_id(conditional.extends_ty);
            bytes.type_id(conditional.true_branch);
            bytes.type_id(conditional.false_branch);
            bytes.u32(conditional.infer_count);
            bytes.bool(conditional.distributive);
            bytes.bool(conditional.poisoned);
        }
        TypeTag::Instantiation => {
            let instantiation = store.instantiation_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing instantiation".to_owned())
            })?;
            bytes.type_id(instantiation.base);
            bytes.usize(instantiation.args.len())?;
            for (parameter, argument) in &instantiation.args {
                bytes.u32(parameter.0);
                bytes.type_id(*argument);
            }
        }
        TypeTag::Infer => bytes.u32(store.infer_index(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing infer index".to_owned())
        })?),
        TypeTag::Mapped => {
            let mapped = store.mapped_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing mapped payload".to_owned())
            })?;
            bytes.bool(mapped.homomorphic);
            bytes.type_id(mapped.key_source);
            bytes.type_id(mapped.value_template);
            bytes.optional_type_id(mapped.modifiers_source);
            bytes.byte(modifier_code(mapped.optional_modifier));
            bytes.byte(modifier_code(mapped.readonly_modifier));
        }
        TypeTag::MappedValue => {}
        TypeTag::Template => {
            let template = store.template_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing template payload".to_owned())
            })?;
            bytes.usize(template.texts.len())?;
            for text in &template.texts {
                bytes.string(text)?;
            }
            bytes.type_ids(&template.holes)?;
        }
        TypeTag::Keyof => bytes.type_id(store.keyof_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing keyof operand".to_owned())
        })?),
        TypeTag::ClassInstance => {
            let instance = store.class_instance_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing class instance".to_owned())
            })?;
            bytes.u32(instance.class.0);
            bytes.type_ids(&instance.args)?;
        }
        TypeTag::DeferredIndexedAccess => {
            let access = store.deferred_indexed_access_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "missing deferred indexed access".to_owned(),
                )
            })?;
            bytes.type_id(access.object);
            bytes.type_id(access.index);
        }
        TypeTag::Declared => {
            let declared = store.declared_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing declared payload".to_owned())
            })?;
            bytes.u32(declared.recipe.0);
            bytes.usize(declared.mapper.len())?;
            for (parameter, value) in &declared.mapper {
                bytes.u32(parameter.0);
                bytes.type_id(*value);
            }
        }
    }
    bytes.bool(store.template_name(id).is_some());
    if let Some(name) = store.template_name(id) {
        bytes.string(name)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
fn encode_declared_recipe_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: crate::types::repr::DeclaredRecipeId,
) -> Result<(), InjectedProfileError> {
    use crate::types::repr::DeclaredRecipeNode;
    let recipe = store.declared_recipe(id).ok_or_else(|| {
        InjectedProfileError::CanonicalProjection("missing declared recipe".to_owned())
    })?;
    match &recipe.node {
        DeclaredRecipeNode::Type(ty) => {
            bytes.byte(0);
            bytes.type_id(*ty);
        }
        DeclaredRecipeNode::Array(element) => {
            bytes.byte(1);
            bytes.u32(element.0);
        }
        DeclaredRecipeNode::Tuple { elements, rest } => {
            bytes.byte(2);
            bytes.usize(elements.len())?;
            for element in elements {
                bytes.u32(element.0);
            }
            bytes.bool(rest.is_some());
            if let Some((position, rest)) = rest {
                bytes.usize(*position)?;
                bytes.u32(rest.0);
            }
        }
        DeclaredRecipeNode::Readonly(operand) => {
            bytes.byte(3);
            bytes.u32(operand.0);
        }
        DeclaredRecipeNode::Application {
            template,
            parameters,
            arguments,
        } => {
            bytes.byte(4);
            bytes.type_id(*template);
            bytes.usize(parameters.len())?;
            for parameter in parameters {
                bytes.u32(parameter.0);
            }
            bytes.usize(arguments.len())?;
            for argument in arguments {
                bytes.u32(argument.0);
            }
        }
    }
    bytes.usize(recipe.free_params.len())?;
    for parameter in &recipe.free_params {
        bytes.u32(parameter.0);
    }
    Ok(())
}

fn canonical_input_for_record(
    canonical: &[CanonicalInput<'_>],
    file_ordinal: LibraryFileOrdinal,
) -> Result<usize, InjectedProfileError> {
    canonical
        .iter()
        .position(|input| input.file_ordinal == file_ordinal)
        .ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "library record has no canonical source".to_owned(),
            )
        })
}

#[cfg(any(test, feature = "test-utils"))]
fn encode_library_key(
    bytes: &mut CanonicalBytes,
    key: LibraryEventKey,
) -> Result<(), InjectedProfileError> {
    bytes.usize(key.file_ordinal.index())?;
    bytes.u32(key.source_start);
    bytes.usize(key.event_ordinal)?;
    bytes.usize(key.record_ordinal)?;
    Ok(())
}

fn canonical_record_bytes(
    source: &str,
    record: &CheckerRecord,
) -> Result<Vec<u8>, InjectedProfileError> {
    let mut bytes = CanonicalBytes::domain(b"typokat-wu0d-library-record-v1");
    match record {
        CheckerRecord::Diagnostic(diagnostic) => {
            bytes.byte(0);
            bytes.u32(diagnostic.span.start);
            bytes.u32(diagnostic.span.end);
            let mut rendered = Vec::new();
            render_to_writer_with_format(
                &mut rendered,
                "library",
                source,
                std::slice::from_ref(diagnostic),
                DiagnosticFormat::Compact,
            )
            .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
            record_collision_plan_forbidden_work(|work| {
                work.rendered_record_digest_bytes = work
                    .rendered_record_digest_bytes
                    .saturating_add(u64::try_from(rendered.len()).unwrap_or(u64::MAX));
            });
            bytes.bytes(&rendered)?;
        }
        CheckerRecord::Incomplete(incomplete) => {
            bytes.byte(1);
            bytes.string(&incomplete.id)?;
            bytes.u32(incomplete.span.start);
            bytes.u32(incomplete.span.end);
            bytes.string(&incomplete.context)?;
        }
    }
    Ok(bytes.finish())
}

#[cfg(any(test, feature = "test-utils"))]
fn full_structured_record_baseline(
    owner: ReplayOwner,
    records: &[&CheckerRecord],
) -> super::replay_index::ReplayBaselineRecord {
    fn bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(value);
    }

    fn span(hasher: &mut Sha256, span: Span) {
        hasher.update(span.start.to_be_bytes());
        hasher.update(span.end.to_be_bytes());
    }

    let mut hasher = Sha256::new();
    hasher.update(b"typokat/collision-record-fingerprint/v1");
    hasher.update(
        u64::try_from(records.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for record in records {
        match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                hasher.update([0]);
                bytes(&mut hasher, diagnostic.code.as_str().as_bytes());
                hasher.update([match diagnostic.severity {
                    crate::diagnostics::Severity::Error => 0,
                    crate::diagnostics::Severity::Warning => 1,
                }]);
                bytes(&mut hasher, diagnostic.message.as_bytes());
                span(&mut hasher, diagnostic.span);
                hasher.update(
                    u64::try_from(diagnostic.elaboration().len())
                        .unwrap_or(u64::MAX)
                        .to_be_bytes(),
                );
                for line in diagnostic.elaboration() {
                    bytes(&mut hasher, line.as_bytes());
                }
            }
            CheckerRecord::Incomplete(incomplete) => {
                hasher.update([1]);
                bytes(&mut hasher, incomplete.id.as_bytes());
                span(&mut hasher, incomplete.span);
                bytes(&mut hasher, incomplete.context.as_bytes());
            }
        }
    }
    super::replay_index::ReplayBaselineRecord {
        owner,
        record_count: u64::try_from(records.len()).unwrap_or(u64::MAX),
        digest: hasher.finalize().into(),
    }
}

/// Project the library's records onto their canonical byte blobs.
///
/// Test-only by construction: the blobs pinned the retired artifact's sections and no compile
/// reads them, so the projection is a suite assertion rather than startup work (ADR-0017).
#[cfg(any(test, feature = "test-utils"))]
pub fn canonical_library_evidence_for_test(
    sources: &[InjectedLibrarySource<'_>],
    records: &[(LibraryEventKey, CheckerRecord)],
) -> Result<CanonicalLibraryEvidence, InjectedProfileError> {
    let mut diagnostics = CanonicalBytes::domain(b"typokat-wu0d-diagnostics-v1");
    let mut incompletes = CanonicalBytes::domain(b"typokat-wu0d-incomplete-v1");
    let mut ledger = CanonicalBytes::domain(b"typokat-wu0d-library-ledger-v1");
    let diagnostic_count = records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    diagnostics.usize(diagnostic_count)?;
    incompletes.usize(records.len() - diagnostic_count)?;
    ledger.usize(records.len())?;
    for (key, record) in records {
        let source = sources
            .iter()
            .find(|source| source.file_ordinal == key.file_ordinal)
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library record has no canonical source".to_owned(),
                )
            })?
            .source;
        let record_bytes = canonical_record_bytes(source, record)?;
        encode_library_key(&mut ledger, *key)?;
        ledger.bytes(&record_bytes)?;
        let component = match record {
            CheckerRecord::Diagnostic(_) => &mut diagnostics,
            CheckerRecord::Incomplete(_) => &mut incompletes,
        };
        encode_library_key(component, *key)?;
        component.bytes(&record_bytes)?;
    }
    Ok(CanonicalLibraryEvidence {
        diagnostics: diagnostics.finish(),
        incompletes: incompletes.finish(),
        ledger: ledger.finish(),
    })
}

struct ReplayTerminalValidationInputs<'a> {
    binder: &'a Binder,
    published: &'a PublishedTypeEnvironment,
    decl_types: &'a DeclTypes,
    namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: Option<&'a LibrarySemanticIdentities>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalClassDependencyValidationWork {
    owner_root_summaries: u64,
    semantic_node_visits: u64,
    semantic_edge_probes: u64,
    class_edge_probes: u64,
    class_summary_items: u64,
    owner_expression_visits: u64,
}

fn intern_terminal_semantic_node(
    nodes: &mut rustc_hash::FxHashMap<(u8, u32), usize>,
    semantic_edges: &mut Vec<Vec<usize>>,
    reverse_edges: &mut Vec<Vec<usize>>,
    class_edges: &mut Vec<Vec<ClassId>>,
    key: (u8, u32),
) -> usize {
    if let Some(node) = nodes.get(&key) {
        return *node;
    }
    let node = semantic_edges.len();
    nodes.insert(key, node);
    semantic_edges.push(Vec::new());
    reverse_edges.push(Vec::new());
    class_edges.push(Vec::new());
    node
}

enum TerminalOwnerExpression {
    Owner(ReplayOwner),
    Union(Vec<usize>),
}

/// Flattens every owner expression in one ascending pass. A `Union` is always
/// minted after the inputs it merges, so the vector is already topologically
/// ordered and each expression is summarized exactly once — memoizing only at
/// query roots re-walked shared pass-through chains once per root.
fn terminal_expression_owners(
    expressions: &[TerminalOwnerExpression],
    #[cfg(any(test, feature = "test-utils"))] work: &mut Option<
        &mut TerminalClassDependencyValidationWork,
    >,
) -> Vec<Vec<ReplayOwner>> {
    let mut owners = Vec::<Vec<ReplayOwner>>::with_capacity(expressions.len());
    for (expression, node) in expressions.iter().enumerate() {
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_owner_expression_visits(work, 1);
        match node {
            TerminalOwnerExpression::Owner(owner) => owners.push(vec![*owner]),
            TerminalOwnerExpression::Union(inputs) => {
                let mut merged = Vec::new();
                for input in inputs.iter().copied() {
                    // Ascending order is the precondition of this pass; a forward
                    // reference would silently truncate the owner closure.
                    assert!(
                        input < expression,
                        "owner expression {expression} merges a later input {input}"
                    );
                    #[cfg(any(test, feature = "test-utils"))]
                    record_terminal_owner_expression_visits(work, owners[input].len());
                    merged.extend_from_slice(&owners[input]);
                }
                merged.sort_unstable();
                merged.dedup();
                owners.push(merged);
            }
        }
    }
    owners
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_semantic_nodes(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_node_visits = work
            .semantic_node_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_semantic_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_edge_probes = work
            .semantic_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_class_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_edge_probes = work
            .class_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_class_summary_items(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_summary_items = work
            .class_summary_items
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_terminal_owner_expression_visits(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.owner_expression_visits = work
            .owner_expression_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

fn require_terminal_class_dependency_closure(
    trace: &ReplayDependencyTrace,
    sparse_semantic_edges: &BTreeMap<(u8, u32), Vec<(u8, u32)>>,
    sparse_class_edges: &BTreeMap<(u8, u32), Vec<ClassId>>,
    direct: BTreeMap<ReplayOwner, Vec<TypeId>>,
    #[cfg(any(test, feature = "test-utils"))] mut work: Option<
        &mut TerminalClassDependencyValidationWork,
    >,
) {
    const TYPE_DOMAIN: u8 = 1;

    let mut nodes = rustc_hash::FxHashMap::default();
    let mut semantic_edges = Vec::<Vec<usize>>::new();
    let mut reverse_edges = Vec::<Vec<usize>>::new();
    let mut class_edges = Vec::<Vec<ClassId>>::new();
    for (source_key, targets) in sparse_semantic_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        for target_key in targets {
            let target = intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                *target_key,
            );
            semantic_edges[source].push(target);
            reverse_edges[target].push(source);
        }
    }
    for (source_key, classes) in sparse_class_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, classes.len());
            record_terminal_class_summary_items(&mut work, classes.len());
        }
        class_edges[source].extend(classes.iter().copied());
    }
    for roots in direct.values() {
        for root in roots {
            intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                (TYPE_DOMAIN, root.0),
            );
        }
    }
    for classes in &mut class_edges {
        classes.sort_unstable();
        classes.dedup();
    }

    // Condense cycles before propagating terminal classes across shared tails.
    let node_count = semantic_edges.len();
    let mut finish_order = Vec::with_capacity(node_count);
    let mut seen = vec![false; node_count];
    for start in 0..node_count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);
        let mut pending = vec![(start, 0_usize)];
        while let Some((node, next_edge)) = pending.last_mut() {
            if *next_edge == semantic_edges[*node].len() {
                finish_order.push(*node);
                pending.pop();
                continue;
            }
            let target = semantic_edges[*node][*next_edge];
            *next_edge += 1;
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            if !seen[target] {
                seen[target] = true;
                #[cfg(any(test, feature = "test-utils"))]
                record_terminal_semantic_nodes(&mut work, 1);
                pending.push((target, 0));
            }
        }
    }

    let mut component_of = vec![usize::MAX; node_count];
    let mut component_count = 0;
    while let Some(start) = finish_order.pop() {
        if component_of[start] != usize::MAX {
            continue;
        }
        component_of[start] = component_count;
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_nodes(&mut work, 1);
            for source in &reverse_edges[node] {
                #[cfg(any(test, feature = "test-utils"))]
                record_terminal_semantic_edges(&mut work, 1);
                if component_of[*source] == usize::MAX {
                    component_of[*source] = component_count;
                    pending.push(*source);
                }
            }
        }
        component_count += 1;
    }

    let mut component_dependencies = vec![Vec::<usize>::new(); component_count];
    let mut component_dependents = vec![Vec::<usize>::new(); component_count];
    let mut component_classes = vec![Vec::<ClassId>::new(); component_count];
    for node in 0..node_count {
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);
        let component = component_of[node];
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, class_edges[node].len());
            record_terminal_class_summary_items(&mut work, class_edges[node].len());
        }
        component_classes[component].extend_from_slice(&class_edges[node]);
        for target in &semantic_edges[node] {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            let dependency = component_of[*target];
            if component != dependency {
                component_dependencies[component].push(dependency);
            }
        }
    }
    for classes in &mut component_classes {
        classes.sort_unstable();
        classes.dedup();
    }
    for dependencies in &mut component_dependencies {
        dependencies.sort_unstable();
        dependencies.dedup();
    }
    for (component, dependencies) in component_dependencies.iter().enumerate() {
        for dependency in dependencies {
            component_dependents[*dependency].push(component);
        }
    }

    let mut expressions = direct
        .keys()
        .copied()
        .map(TerminalOwnerExpression::Owner)
        .collect::<Vec<_>>();
    let owner_expressions = direct
        .keys()
        .copied()
        .enumerate()
        .map(|(expression, owner)| (owner, expression))
        .collect::<BTreeMap<_, _>>();
    let mut component_owner_inputs = vec![Vec::<usize>::new(); component_count];
    for (owner, roots) in &direct {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(work) = work.as_deref_mut() {
            work.owner_root_summaries = work
                .owner_root_summaries
                .saturating_add(u64::try_from(roots.len()).unwrap_or(u64::MAX));
        }
        let expression = owner_expressions[owner];
        for root in roots {
            let node = nodes[&(TYPE_DOMAIN, root.0)];
            component_owner_inputs[component_of[node]].push(expression);
        }
    }

    // Owner expressions stay persistent across pass-through chains. They are
    // flattened only where terminal classes make the owner × class output real.
    let mut unresolved_dependents = component_dependents
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let mut ready = unresolved_dependents
        .iter()
        .enumerate()
        .filter_map(|(component, count)| (*count == 0).then_some(component))
        .collect::<Vec<_>>();
    let mut processed_components = 0;
    let mut class_owner_inputs = BTreeMap::<ClassId, Vec<usize>>::new();
    while let Some(component) = ready.pop() {
        processed_components += 1;
        #[cfg(any(test, feature = "test-utils"))]
        record_terminal_semantic_nodes(&mut work, 1);

        component_owner_inputs[component].sort_unstable();
        component_owner_inputs[component].dedup();
        let owner_expression = match component_owner_inputs[component].as_slice() {
            [] => None,
            [expression] => Some(*expression),
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                Some(expression)
            }
        };

        if let Some(expression) = owner_expression {
            #[cfg(any(test, feature = "test-utils"))]
            {
                record_terminal_class_edges(&mut work, component_classes[component].len());
                record_terminal_class_summary_items(&mut work, component_classes[component].len());
            }
            for class in &component_classes[component] {
                class_owner_inputs
                    .entry(*class)
                    .or_default()
                    .push(expression);
            }
            for dependency in component_dependencies[component].iter().copied() {
                component_owner_inputs[dependency].push(expression);
            }
        }

        for dependency in component_dependencies[component].iter().copied() {
            #[cfg(any(test, feature = "test-utils"))]
            record_terminal_semantic_edges(&mut work, 1);
            unresolved_dependents[dependency] -= 1;
            if unresolved_dependents[dependency] == 0 {
                ready.push(dependency);
            }
        }
    }
    // An undrained condensation would emit no classes for the stranded components,
    // i.e. silently admit unauthenticated replay dependencies.
    assert_eq!(
        processed_components, component_count,
        "the terminal condensation is acyclic and must drain completely"
    );

    let mut class_expressions = Vec::with_capacity(class_owner_inputs.len());
    for (class, mut inputs) in class_owner_inputs {
        inputs.sort_unstable();
        inputs.dedup();
        let expression = match inputs.as_slice() {
            [expression] => *expression,
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                expression
            }
        };
        class_expressions.push((class, expression));
    }
    let expression_owners = terminal_expression_owners(
        &expressions,
        #[cfg(any(test, feature = "test-utils"))]
        &mut work,
    );
    for (class, expression) in class_expressions {
        let owners = &expression_owners[expression];
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_terminal_class_edges(&mut work, owners.len());
            record_terminal_class_summary_items(&mut work, owners.len());
        }
        for owner in owners {
            trace.require_dependency(*owner, ReplayOwner::Class(class));
        }
    }
}

/// Validate the terminal runtime state the base publishes, and — when a replay trace is present
/// — collect the terminal dependency closure a collision replay index needs.
///
/// The validating half is not about the index. It checks the published class registry is
/// terminal, that every class name and named-function symbol the frozen runtime carries has an
/// owner, and that the library's semantic identities survive an exact recomputation — all of it
/// state the base itself publishes. That half runs on every compile. Only the dependency-edge
/// collection is conditional on assembling an index, so the two paths cannot drift apart.
fn validate_terminal_class_dependencies(
    trace: Option<&ReplayDependencyTrace>,
    interner: &Interner,
    inputs: ReplayTerminalValidationInputs<'_>,
) -> Result<(), InjectedProfileError> {
    let require_dependency = |consumer: ReplayOwner, dependency: ReplayOwner| {
        if let Some(trace) = trace {
            trace.require_dependency(consumer, dependency);
        }
    };
    let ReplayTerminalValidationInputs {
        binder,
        published,
        decl_types,
        namespace_terminals,
        runtime,
        semantic_identities,
    } = inputs;
    record_collision_plan_forbidden_work(|work| {
        let projected = binder
            .type_groups
            .len()
            .saturating_add(decl_types.len())
            .saturating_add(namespace_terminals.len())
            .saturating_add(runtime.class_application_parameters.len())
            .saturating_add(runtime.class_new_metadata.len())
            .saturating_add(runtime.class_parents.len());
        let projected = u64::try_from(projected).unwrap_or(u64::MAX);
        work.transitive_terminal_owner_entries = work
            .transitive_terminal_owner_entries
            .saturating_add(projected);
        work.full_semantic_projection_rows =
            work.full_semantic_projection_rows.saturating_add(projected);
        work.eager_all_owner_scc_memberships = work
            .eager_all_owner_scc_memberships
            .saturating_add(u64::try_from(interner.store().len()).unwrap_or(u64::MAX));
    });
    const TYPE_DOMAIN: u8 = 1;
    const CLASS_DOMAIN: u8 = 3;
    const DECLARED_RECIPE_DOMAIN: u8 = 10;

    let references = interner
        .typed_reference_records_for_replay_generation()
        .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
    let mut semantic_edges: BTreeMap<(u8, u32), Vec<(u8, u32)>> = BTreeMap::new();
    let mut class_edges: BTreeMap<(u8, u32), Vec<ClassId>> = BTreeMap::new();
    for (owner_domain, target_domain, _, owner, target) in references {
        if !matches!(owner_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
            continue;
        }
        match target_domain {
            TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN => semantic_edges
                .entry((owner_domain, owner))
                .or_default()
                .push((target_domain, target)),
            CLASS_DOMAIN => class_edges
                .entry((owner_domain, owner))
                .or_default()
                .push(ClassId(target)),
            _ => {}
        }
    }

    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
    for index in 0..published.groups().len() {
        let group =
            TypeGroupId(u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?);
        let Some(PublishedTypeGroupTerminal::Ready(ready)) = published.groups().get(group) else {
            continue;
        };
        let owner = ReplayOwner::TypeGroup(group);
        match ready.surface {
            PublishedTypeGroupSurface::Template(ty) => direct.entry(owner).or_default().push(ty),
            PublishedTypeGroupSurface::Class(class) => {
                require_dependency(owner, ReplayOwner::Class(class));
            }
        }
        direct
            .entry(owner)
            .or_default()
            .extend(
                ready
                    .parameter_defaults
                    .iter()
                    .filter_map(|default| match default {
                        PublishedTypeParameterDefault::Ready(ty) => Some(*ty),
                        PublishedTypeParameterDefault::Absent
                        | PublishedTypeParameterDefault::Unsupported => None,
                    }),
            );
        direct.entry(owner).or_default().extend(
            ready
                .conflict_alternatives
                .iter()
                .flat_map(|alternative| alternative.types.iter().copied()),
        );
    }
    for (index, ty) in decl_types.snapshot_slots().into_iter().enumerate() {
        let Some(ty) = ty else { continue };
        let storage = ValueStorageId(
            u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        );
        direct
            .entry(ReplayOwner::Value(storage))
            .or_default()
            .push(ty);
    }
    for row in namespace_terminals {
        let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal else {
            continue;
        };
        let owner = ReplayOwner::Namespace(row.namespace);
        require_dependency(owner, ReplayOwner::Value(storage));
        direct.entry(owner).or_default().push(ty);
    }
    let terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    record_collision_plan_forbidden_work(|work| {
        work.canonical_terminal_rows = work
            .canonical_terminal_rows
            .saturating_add(u64::try_from(terminals.len()).unwrap_or(u64::MAX));
    });
    for (class, terminal) in &terminals {
        let CanonicalPublishedClassTerminal::Ready(surface) = terminal else {
            continue;
        };
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        types.push(surface.instance_template());
        types.push(surface.static_template());
        types.extend(surface.constructor_template());
    }

    for (class, parameters) in &runtime.class_application_parameters {
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        for parameter in parameters {
            types.extend(parameter.constraint);
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                types.push(default);
            }
        }
    }
    for (class, metadata) in &runtime.class_new_metadata {
        require_dependency(
            ReplayOwner::Class(*class),
            ReplayOwner::Class(metadata.ctor_declaring_class),
        );
    }
    for (class, parent) in &runtime.class_parents {
        require_dependency(ReplayOwner::Class(*class), ReplayOwner::Class(*parent));
    }
    for (alias, target) in &runtime.class_value_aliases {
        require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    for (storage, binding) in &runtime.class_value_bindings {
        require_dependency(
            ReplayOwner::Value(*storage),
            ReplayOwner::Class(binding.class_id),
        );
    }
    for (alias, target) in &runtime.standalone_namespace_value_aliases {
        require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    let class_ids = terminals
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if runtime
        .class_names
        .iter()
        .any(|(class, _)| !class_ids.contains(class))
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "replay runtime class name has no published class owner".to_owned(),
        ));
    }
    for symbol in &runtime.named_function_symbols {
        let binding = binder.symbols.get(*symbol).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "replay runtime named function symbol is missing".to_owned(),
            )
        })?;
        let Some(canonical) = binding.function_values.first().copied() else {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay runtime named function has no value owner".to_owned(),
            ));
        };
        for participant in &binding.function_values {
            require_dependency(
                ReplayOwner::Value(canonical),
                ReplayOwner::Value(*participant),
            );
        }
    }
    if let Some(identities) = semantic_identities {
        let recomputed = LibrarySemanticIdentities::select(binder, published, interner.store());
        if &recomputed != identities {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay semantic identities differ from exact recomputation".to_owned(),
            ));
        }
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "replay semantic identity is unavailable".to_owned(),
                ));
            };
            direct
                .entry(ReplayOwner::TypeGroup(identity.group))
                .or_default()
                .push(identity.template);
        }
    }

    if let Some(trace) = trace {
        require_terminal_class_dependency_closure(
            trace,
            &semantic_edges,
            &class_edges,
            direct,
            #[cfg(any(test, feature = "test-utils"))]
            None,
        );
    }
    Ok(())
}

fn validate_root_census(
    candidates: &BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    rows: &[RootNameRow],
    explicit_global_this: bool,
) -> Result<(), ReplayIndexGenerationError> {
    let mut by_name = BTreeMap::new();
    for row in rows {
        if by_name.insert(row.name.as_str(), row).is_some() {
            return Err(ReplayIndexGenerationError::DuplicateRootName(
                row.name.clone(),
            ));
        }
    }
    for (name, candidate) in candidates {
        let Some(row) = by_name.get(name.as_str()).copied() else {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
        };
        for (slot, present) in [
            (SourceBindingSlot::Value, row.value.is_some()),
            (SourceBindingSlot::Type, row.ty.is_some()),
            (SourceBindingSlot::Namespace, row.namespace.is_some()),
        ] {
            if candidate.slots.contains(&slot) != present {
                return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
            }
        }
    }
    for row in rows {
        if !candidates.contains_key(&row.name) {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(
                row.name.clone(),
            ));
        }
    }
    if explicit_global_this && !by_name.contains_key("globalThis") {
        return Err(ReplayIndexGenerationError::InvalidRootSlot(
            "globalThis".to_owned(),
        ));
    }
    Ok(())
}

struct NormalizedSourceRootCandidates {
    candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    namespace_contributors: BTreeSet<String>,
}

fn apply_merge_root_semantics(
    candidate: &mut crate::binder::declaration::SourceGlobalBindingCandidate,
    slots: BTreeSet<SourceBindingSlot>,
    ordinary_contributor: bool,
    namespace_instantiated: bool,
) {
    candidate.slots = slots;
    if namespace_instantiated {
        candidate.slots.insert(SourceBindingSlot::Value);
    }
    candidate.global_object_contributor = ordinary_contributor || namespace_instantiated;
}

fn normalize_source_root_candidates(
    mut candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    binder: &Binder,
) -> NormalizedSourceRootCandidates {
    let mut namespace_contributors = BTreeSet::new();
    for record in binder.namespaces.merges().filter(|record| {
        record.owner == crate::binder::namespace::DeclarationOwner::CompilationGlobal
    }) {
        if record.classification.disposition != crate::binder::namespace::MergeDisposition::Admitted
        {
            candidates.remove(record.name.as_ref());
            continue;
        }
        let Some(candidate) = candidates.get_mut(record.name.as_ref()) else {
            continue;
        };
        let mut slots = BTreeSet::new();
        let mut ordinary_contributor = false;
        for declaration in record.declarations.iter() {
            if declaration.spaces.value {
                slots.insert(SourceBindingSlot::Value);
            }
            if declaration.spaces.r#type {
                slots.insert(SourceBindingSlot::Type);
            }
            if declaration.spaces.namespace {
                slots.insert(SourceBindingSlot::Namespace);
            }
            ordinary_contributor |= declaration.kind
                == crate::binder::namespace::MergeDeclarationKind::Function
                || matches!(
                    declaration.syntax,
                    crate::binder::namespace::DeclarationSyntaxFacts::Variable(
                        crate::binder::namespace::VariableKind::Var
                    )
                );
        }
        let namespace = record
            .declarations
            .iter()
            .find_map(|declaration| declaration.namespace_fragment)
            .and_then(|fragment| binder.namespaces.fragment(fragment))
            .map(|fragment| fragment.namespace);
        let namespace_instantiated = namespace.is_some_and(|namespace| {
            binder.namespaces.aggregate_instance_state(namespace)
                == Some(crate::binder::namespace::NamespaceInstanceState::Instantiated)
        });
        apply_merge_root_semantics(
            candidate,
            slots,
            ordinary_contributor,
            namespace_instantiated,
        );
        if namespace_instantiated {
            namespace_contributors.insert(record.name.to_string());
        }
    }
    NormalizedSourceRootCandidates {
        candidates,
        namespace_contributors,
    }
}

fn build_collision_replay_plan(
    trace: ReplayDependencyTrace,
    root_provenance: LibraryRootProjection,
    baselines: Vec<super::replay_index::ReplayBaselineRecord>,
    prefix_cardinalities: [usize; 9],
    forbidden_work: CollisionPlanForbiddenWork,
) -> Result<CollisionReplayPlan, InjectedProfileError> {
    let LibraryRootProjection {
        candidates,
        explicit_global_this,
        contributor_sites,
        explicit_global_this_sites,
        root_rows,
        canonical_unit_count,
        source_census_count,
        uncertain_candidate_count,
        uncertain_relevant_syntax_count,
        normalization_issue_count,
    } = root_provenance;
    if source_census_count != canonical_unit_count
        || uncertain_candidate_count != 0
        || uncertain_relevant_syntax_count != 0
        || normalization_issue_count != 0
    {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::InvalidRootSlot("<binder-source-census>".to_owned()),
        ));
    }
    if forbidden_work
        != (CollisionPlanForbiddenWork {
            library_source_compiles: 1,
            ..CollisionPlanForbiddenWork::default()
        })
    {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::BaselinePartitionMismatch,
        ));
    }
    for site in contributor_sites {
        trace.record_owner_site(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal: site.file_ordinal,
            span: site.span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::GlobalContributor {
                name: site.name,
                kind: site.kind,
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    for (file_ordinal, span) in explicit_global_this_sites {
        trace.record_owner_site(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal,
            span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::ExplicitGlobalThis {
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    validate_root_census(&candidates, &root_rows, explicit_global_this)
        .map_err(InjectedProfileError::ReplayIndex)?;
    let mut roots = Vec::with_capacity(root_rows.len());
    {
        let _global_scope = trace.scope(ReplayOwner::GlobalObject);
        for row in root_rows {
            let candidate = candidates.get(&row.name).ok_or_else(|| {
                InjectedProfileError::ReplayIndex(ReplayIndexGenerationError::InvalidRootSlot(
                    row.name.clone(),
                ))
            })?;
            let contributor = candidate.global_object_contributor;
            if contributor {
                if let Some(value) = row.value {
                    trace.demand(ReplayOwner::Value(value));
                }
                if let Some(ty) = row.ty {
                    trace.demand(ReplayOwner::TypeGroup(ty));
                }
                if let Some(namespace) = row.namespace {
                    trace.demand(ReplayOwner::Namespace(namespace));
                }
            }
            roots.push(ReplayRootSlot {
                explicit_global_this: explicit_global_this && row.name == "globalThis",
                name: row.name,
                value: row.value,
                ty: row.ty,
                namespace: row.namespace,
                global_object_contributor: contributor,
            });
        }
    }

    trace
        .finish_compact_plan(
            roots,
            baselines,
            prefix_cardinalities,
            CollisionReplayConstructionEvidence {
                library_source_compiles: forbidden_work.library_source_compiles,
                binder_source_censuses: u64::try_from(source_census_count).unwrap_or(u64::MAX),
                canonical_source_units: u64::try_from(canonical_unit_count).unwrap_or(u64::MAX),
                second_source_censuses: forbidden_work.second_source_censuses,
                canonical_manifest_bytes: forbidden_work.canonical_manifest_bytes,
                rendered_record_digest_bytes: forbidden_work.rendered_record_digest_bytes,
                transitive_terminal_owner_entries: forbidden_work.transitive_terminal_owner_entries,
                eager_all_owner_scc_memberships: forbidden_work.eager_all_owner_scc_memberships,
                namespace_snapshot_rows: forbidden_work.namespace_snapshot_rows,
                runtime_snapshot_rows: forbidden_work.runtime_snapshot_rows,
                canonical_terminal_rows: forbidden_work.canonical_terminal_rows,
                full_semantic_projection_rows: forbidden_work.full_semantic_projection_rows,
                ticket_slots: 0,
                ticket_owner_ordered_map_inserts: 0,
                owner_site_inner_heap_allocations: 0,
                owner_site_dense_slot_writes: 0,
                owner_site_ordered_map_inserts: 0,
                trace_domain_sealed_after_binder_reporting: false,
            },
        )
        .map_err(InjectedProfileError::ReplayIndex)
}

#[allow(clippy::too_many_arguments)]
fn build_collision_replay_index(
    trace: ReplayDependencyTrace,
    interner: &Interner,
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    decl_types: &DeclTypes,
    namespace_terminals: &[FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: Option<&LibrarySemanticIdentities>,
    canonical: &[CanonicalInput<'_>],
    parsed: &[&Program<'_>],
    module_scopes: &[ScopeId],
    class_declarations: &BTreeMap<ClassId, crate::binder::declaration::DeclId>,
    records: &[(LibraryEventKey, CheckerRecord)],
    #[cfg(any(test, feature = "test-utils"))] independent_event_owner_sites: Vec<
        CollisionReplayOwnerSite,
    >,
) -> Result<CollisionReplayIndex, InjectedProfileError> {
    fn add_owner_site(
        module_ordinals: &rustc_hash::FxHashMap<ScopeId, LibraryFileOrdinal>,
        sites: &mut BTreeSet<(ReplayOwner, LibraryFileOrdinal, u32, u32)>,
        invalid_sites: &mut u64,
        owner: ReplayOwner,
        module: ScopeId,
        span: Span,
    ) {
        if let Some(file_ordinal) = module_ordinals.get(&module).copied() {
            sites.insert((owner, file_ordinal, span.start, span.end));
        } else {
            *invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let statement_keys = trace.statement_keys();
    let module_ordinals = module_scopes
        .iter()
        .copied()
        .zip(canonical.iter().map(|input| input.file_ordinal))
        .collect::<rustc_hash::FxHashMap<_, _>>();
    #[cfg(any(test, feature = "test-utils"))]
    let mut exact_owner_sites = independent_event_owner_sites;
    let mut owners = Vec::new();
    owners.extend((0..binder.type_groups.len()).map(|index| {
        ReplayOwner::TypeGroup(TypeGroupId(
            u32::try_from(index).expect("type-group count fits u32"),
        ))
    }));
    owners.extend(
        (0..usize::try_from(binder.decl_count).unwrap_or(usize::MAX)).map(|index| {
            ReplayOwner::Value(ValueStorageId(
                u32::try_from(index).expect("value-storage count fits u32"),
            ))
        }),
    );
    owners.extend((0..binder.namespaces.len()).map(|index| {
        ReplayOwner::Namespace(NamespaceId(
            u32::try_from(index).expect("namespace count fits u32"),
        ))
    }));
    let class_terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    owners.extend(
        class_terminals
            .iter()
            .map(|(class, _)| ReplayOwner::Class(*class)),
    );
    owners.push(ReplayOwner::GlobalObject);
    owners.extend(statement_keys.iter().copied().map(ReplayOwner::Statement));

    let mut invalid_sites = 0u64;
    let mut sites = BTreeSet::new();
    for index in 0..binder.type_groups.len() {
        let group = TypeGroupId(u32::try_from(index).expect("type-group count fits u32"));
        if let Some(row) = binder.type_groups.get(group) {
            for fragment in &row.fragments {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::TypeGroup(group),
                    fragment.site.module,
                    fragment.site.declaration_span,
                );
                #[cfg(any(test, feature = "test-utils"))]
                if let (Some(file_ordinal), Some(declaration)) = (
                    module_ordinals.get(&fragment.site.module).copied(),
                    binder.declarations.get(fragment.declaration),
                ) {
                    let declaration_scope =
                        declaration.site.scope.unwrap_or(declaration.site.module);
                    exact_owner_sites.push(CollisionReplayOwnerSite {
                        owner: ReplayOwner::TypeGroup(group),
                        file_ordinal,
                        span: declaration.site.declaration_span,
                        provenance: collision_declaration_provenance(
                            binder,
                            declaration,
                            declaration_scope,
                        ),
                    });
                }
            }
        }
    }
    for declaration in binder.declarations.iter() {
        if let Some(storage) = declaration.value_storage {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Value(storage),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Value(storage),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: collision_declaration_provenance(
                        binder,
                        declaration,
                        declaration_scope,
                    ),
                });
            }
        }
    }
    for index in 0..binder.namespaces.len() {
        let namespace = NamespaceId(u32::try_from(index).expect("namespace count fits u32"));
        let Some(row) = binder.namespaces.get(namespace) else {
            continue;
        };
        for fragment_id in &row.fragments {
            let Some(fragment) = binder.namespaces.fragment(*fragment_id) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            let Some(declaration) = binder.declarations.get(fragment.declaration) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Namespace(namespace),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                let provenance =
                    collision_declaration_provenance(binder, declaration, declaration_scope);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Namespace(namespace),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: provenance.clone(),
                });
                if let Some(storage) = binder.namespaces.standalone_value_storage(namespace) {
                    exact_owner_sites.push(CollisionReplayOwnerSite {
                        owner: ReplayOwner::Value(storage),
                        file_ordinal,
                        span: declaration.site.declaration_span,
                        provenance,
                    });
                }
            }
            if let Some(storage) = binder.namespaces.standalone_value_storage(namespace) {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::Value(storage),
                    declaration.site.module,
                    declaration.site.declaration_span,
                );
            }
        }
    }
    for (class, declaration) in class_declarations {
        if let Some(declaration) = binder.declarations.get(*declaration) {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Class(*class),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Class(*class),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: collision_declaration_provenance(
                        binder,
                        declaration,
                        declaration_scope,
                    ),
                });
            }
        } else {
            invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let mut candidates = BTreeMap::new();
    let mut explicit_global_this = false;
    let mut contributor_sites = Vec::new();
    let mut explicit_global_this_sites = Vec::new();
    for ((input, program), module) in canonical.iter().zip(parsed).zip(module_scopes) {
        let provenance = source_global_binding_census_with_provenance(
            program,
            ModuleBindingContext::for_program(program, input.kind),
        );
        record_collision_plan_forbidden_work(|work| {
            work.second_source_censuses = work.second_source_censuses.saturating_add(1);
        });
        explicit_global_this |= provenance.census.explicit_global_this;
        contributor_sites.extend(
            provenance
                .contributor_sites
                .into_iter()
                .map(|site| (site.name, site.kind, input.file_ordinal, site.span)),
        );
        explicit_global_this_sites.extend(
            provenance
                .explicit_global_this_sites
                .into_iter()
                .map(|span| (input.file_ordinal, span)),
        );
        for (name, candidate) in provenance.census.candidates {
            let aggregate = candidates
                .entry(name)
                .or_insert_with(crate::binder::declaration::SourceGlobalBindingCandidate::default);
            aggregate.slots.extend(candidate.slots);
            aggregate.global_object_contributor |= candidate.global_object_contributor;
        }
        let _ = module;
    }
    if let Some(input) = canonical.first() {
        sites.insert((ReplayOwner::GlobalObject, input.file_ordinal, 0, 0));
    }
    for key in &statement_keys {
        sites.insert((
            ReplayOwner::Statement(*key),
            key.file_ordinal,
            key.source_start,
            key.source_start,
        ));
    }

    let root_rows = collect_root_rows(binder)
        .map_err(|error| InjectedProfileError::CanonicalProjection(format!("{error:?}")))?;
    let normalized = normalize_source_root_candidates(candidates, binder);
    let candidates = normalized.candidates;
    for (name, kind, file_ordinal, span) in contributor_sites {
        let admitted = match kind {
            SourceGlobalContributorKind::Ordinary => candidates.contains_key(&name),
            SourceGlobalContributorKind::Namespace => {
                normalized.namespace_contributors.contains(&name)
            }
        };
        if admitted {
            sites.insert((
                ReplayOwner::GlobalObject,
                file_ordinal,
                span.start,
                span.end,
            ));
            #[cfg(any(test, feature = "test-utils"))]
            exact_owner_sites.push(CollisionReplayOwnerSite {
                owner: ReplayOwner::GlobalObject,
                file_ordinal,
                span,
                provenance: super::replay_index::CollisionReplaySiteProvenance::GlobalContributor {
                    name,
                    kind,
                    binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
                },
            });
        }
    }
    for (file_ordinal, span) in explicit_global_this_sites {
        sites.insert((
            ReplayOwner::GlobalObject,
            file_ordinal,
            span.start,
            span.end,
        ));
        #[cfg(any(test, feature = "test-utils"))]
        exact_owner_sites.push(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal,
            span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::ExplicitGlobalThis {
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    validate_root_census(&candidates, &root_rows, explicit_global_this)
        .map_err(InjectedProfileError::ReplayIndex)?;
    let mut roots = Vec::with_capacity(root_rows.len());
    {
        let _global_scope = trace.scope(ReplayOwner::GlobalObject);
        for row in root_rows {
            let candidate = candidates
                .get(&row.name)
                .expect("validated root rows have exact source provenance");
            let contributor = candidate.global_object_contributor;
            if contributor {
                if let Some(value) = row.value {
                    trace.demand(ReplayOwner::Value(value));
                }
                if let Some(ty) = row.ty {
                    trace.demand(ReplayOwner::TypeGroup(ty));
                }
                if let Some(namespace) = row.namespace {
                    trace.demand(ReplayOwner::Namespace(namespace));
                }
            }
            roots.push(ReplayRootSlot {
                explicit_global_this: explicit_global_this && row.name == "globalThis",
                name: row.name,
                value: row.value,
                ty: row.ty,
                namespace: row.namespace,
                global_object_contributor: contributor,
            });
        }
    }

    let mut record_bytes = BTreeMap::<LibraryEventKey, Vec<Vec<u8>>>::new();
    #[cfg(any(test, feature = "test-utils"))]
    let mut record_rows = BTreeMap::<LibraryEventKey, Vec<&CheckerRecord>>::new();
    for (key, record) in records {
        let input = canonical_input_for_record(canonical, key.file_ordinal)?;
        record_bytes
            .entry(*key)
            .or_default()
            .push(canonical_record_bytes(canonical[input].source, record)?);
        #[cfg(any(test, feature = "test-utils"))]
        record_rows.entry(*key).or_default().push(record);
    }
    let baselines = owners
        .iter()
        .copied()
        .map(|owner| {
            let records = match owner {
                ReplayOwner::Statement(key) => {
                    record_bytes.get(&key).map_or(&[][..], Vec::as_slice)
                }
                _ => &[],
            };
            baseline_record(owner, records).map_err(InjectedProfileError::ReplayIndex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(any(test, feature = "test-utils"))]
    let structured_baselines = record_rows
        .iter()
        .map(|(key, records)| {
            full_structured_record_baseline(ReplayOwner::Statement(*key), records)
        })
        .collect::<Vec<_>>();

    validate_terminal_class_dependencies(
        Some(&trace),
        interner,
        ReplayTerminalValidationInputs {
            binder,
            published,
            decl_types,
            namespace_terminals,
            runtime,
            semantic_identities,
        },
    )?;
    let owner_sites = sites
        .into_iter()
        .map(|(owner, file_ordinal, start, end)| ReplayOwnerSite {
            owner,
            file_ordinal,
            span: Span::new(start, end),
        })
        .collect();
    let replay_index = trace
        .finish(owners, roots, owner_sites, baselines, invalid_sites)
        .map_err(InjectedProfileError::ReplayIndex)?;
    #[cfg(any(test, feature = "test-utils"))]
    {
        canonicalize_collision_replay_owner_sites(&mut exact_owner_sites);
        FULL_COLLISION_PLAN_ORACLE.with(|oracle| {
            *oracle.borrow_mut() = Some(FullCollisionPlanOracleForTest {
                owner_sites: exact_owner_sites,
                baseline_records: structured_baselines,
                reverse_edges: replay_index.reverse_edges.clone(),
            });
        });
    }
    Ok(replay_index)
}

#[cfg(any(test, feature = "test-utils"))]
fn collision_declaration_provenance(
    binder: &Binder,
    declaration: &crate::binder::declaration::LexicalDeclaration,
    declaration_scope: ScopeId,
) -> super::replay_index::CollisionReplaySiteProvenance {
    let binder_owner = binder
        .namespaces
        .declaration_owner_for_scope(declaration_scope);
    let containing_namespace = match binder_owner {
        crate::binder::namespace::DeclarationOwner::NamespacePublic(namespace) => Some(namespace),
        crate::binder::namespace::DeclarationOwner::NamespacePrivate(fragment) => binder
            .namespaces
            .fragment(fragment)
            .map(|row| row.namespace),
        _ => None,
    };
    super::replay_index::CollisionReplaySiteProvenance::Declaration {
        declaration: declaration.id,
        kind: declaration.kind,
        binder_owner,
        containing_namespace,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn run_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<InjectedProfileRun, InjectedProfileError> {
    compile_owned_injected_profile(sources).map(|(run, _)| run)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_combined_profile_for_test(
    sources: &[InjectedLibrarySource<'_>],
    library_count: usize,
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    with_canonical_frontend(sources, Some(library_count), |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::None,
            LibraryRecordRetention::Collect,
            OwnerSiteStorageMode::Flat,
        )
        .map(|(run, runtime, _)| (run, runtime))
    })
}

/// Compile the packaged library and one resolved user project in one semantic publication.
pub fn compile_complete_source_project_programs<'ast>(
    sources: &[InjectedLibrarySource<'_>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    let frontend = complete_source_project_frontend(sources, library_programs, units)?;
    let (run, _runtime, replay_plan) = compile_owned_injected_frontend(
        frontend,
        ReplayIndexPlan::None,
        LibraryRecordRetention::Collect,
        OwnerSiteStorageMode::Flat,
    )?;
    if replay_plan.is_some() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source project retained a replay plan".to_owned(),
        ));
    }

    Ok(run.user_results)
}

fn complete_source_project_frontend<'source, 'ast>(
    sources: &[InjectedLibrarySource<'source>],
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[crate::frontend::ProjectProgram<'ast>],
) -> Result<CanonicalLibraryFrontend<'source, 'ast>, InjectedProfileError> {
    let mut canonical = canonical_inputs(sources)?;
    if canonical.len() != library_programs.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source frontend changed the library source count".to_owned(),
        ));
    }
    let mut parser_export_claims = Vec::new();
    let mut parsed = Vec::with_capacity(canonical.len().saturating_add(units.len()));
    for ((input, source), program) in canonical.iter().zip(sources).zip(library_programs) {
        if input.file_ordinal.index() != program.source_ordinal || source.name != program.name {
            return Err(InjectedProfileError::CanonicalProjection(
                "complete-source frontend changed library source identity".to_owned(),
            ));
        }
        if program.parser_panicked {
            return Err(InjectedProfileError::Parse {
                file_ordinal: input.file_ordinal,
                messages: if program.parser_diagnostics.is_empty() {
                    vec!["parser panicked without a diagnostic".to_owned()]
                } else {
                    program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect()
                },
            });
        }
        for diagnostic in &program.parser_diagnostics {
            let [span] = diagnostic.labels.as_slice() else {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            };
            if diagnostic.scope.as_deref() != Some("TS")
                || diagnostic.number.as_deref() != Some("1319")
            {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: program
                        .parser_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.rendered.clone())
                        .collect(),
                });
            }
            parser_export_claims.push(ParserExportClaim {
                file_ordinal: input.file_ordinal,
                span: *span,
            });
        }
        parsed.push(program.program);
    }

    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let library_units = canonical
        .iter()
        .zip(&parsed)
        .map(|(input, program)| {
            (
                *program,
                CompilationUnit {
                    source: input.source_key,
                    origin: input.origin,
                    binding: ModuleBindingContext::for_program(program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let mut module_scopes = builder
        .try_add_library_modules(&library_units)
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let mut library_binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    let collision_root_provenance =
        library_binder
            .take_library_root_projection()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library binder did not retain its one-pass root projection".to_owned(),
                )
            })?;
    let checkpoint_units = canonical
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(input, module)| LibraryBinderUnit {
            ordinal: input.file_ordinal,
            source: input.source_key,
            module,
        })
        .collect();
    let checkpoint = build_library_binder_checkpoint(library_binder, checkpoint_units);
    let checkpoint_ends = checkpoint.checkpoint_ends();
    let (builder, _) = checkpoint.into_continuation();
    let source_offset = u32::try_from(checkpoint_ends.next_source)
        .map_err(|_| InjectedProfileError::SourceKeyOverflow)?
        .checked_sub(1)
        .ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "library source prefix omits the prelude".to_owned(),
            )
        })?;
    let mut user_events = EventStore::default();
    let mut combined_lexical_events: LexicalReservations<PrivateCombinedRecordTicket> =
        LexicalReservations::default();
    for unit in units {
        combined_lexical_events
            .reserve_private_continuation_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                unit.compilation_unit.binding,
                &mut user_events,
            )
            .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    }
    let mut external_effects = BTreeMap::new();
    let bound = super::bind_authoritative_project_core(
        builder,
        units,
        source_offset,
        &combined_lexical_events,
        &mut external_effects,
        super::AuthoritativeProjectBinderFinish::Continuation,
    )
    .map_err(InjectedProfileError::Binder)?;
    validate_parser_export_claims(
        &bound.binder,
        parser_export_claims,
        canonical[0].file_ordinal,
    )?;
    let super::BoundProjectBinder {
        binder,
        module_scopes: user_module_scopes,
        module_placeholders,
        ..
    } = bound;
    #[cfg(any(test, feature = "test-utils"))]
    record_complete_source_bind_for_test(canonical.len(), units.len());

    let library_count = canonical.len();
    for unit in units {
        let ordinal = library_count
            .checked_add(unit.module_ordinal.index())
            .ok_or(InjectedProfileError::SourceKeyOverflow)?;
        let source_key = source_offset
            .checked_add(unit.compilation_unit.source.0)
            .map(exact_key)
            .ok_or(InjectedProfileError::SourceKeyOverflow)?;
        canonical.push(CanonicalInput {
            file_ordinal: LibraryFileOrdinal::new(ordinal),
            source: "",
            kind: source_file_kind(&unit.normalized_path),
            source_key,
            origin: unit.compilation_unit.origin,
        });
        parsed.push(unit.program);
    }
    module_scopes.extend(user_module_scopes);
    let semantic_scopes = canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .map(|((input, program), module)| {
            if matches!(input.origin, CompilationOrigin::User(_))
                || ModuleBindingContext::for_program(program, input.kind).external_module
            {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect();
    Ok(CanonicalLibraryFrontend {
        canonical,
        parsed,
        binder,
        module_scopes,
        semantic_scopes,
        user_start: Some(library_count),
        user_units: units
            .iter()
            .map(|unit| (unit.module_ordinal, unit.unit_slot))
            .collect(),
        user_events,
        combined_lexical_events: Some(combined_lexical_events),
        external_effects,
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed: Duration::ZERO,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed: Duration::ZERO,
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub struct CompleteCombinedOracleForTest {
    pub normalized_records: Vec<(LibraryFileOrdinal, String)>,
    pub normalized_root_projection: Vec<String>,
    pub normalized_semantic_identities: Vec<String>,
    pub initial_store_rows: usize,
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_combined_oracle_for_test(
    sources: &[InjectedLibrarySource<'_>],
    library_count: usize,
    root_names: &[String],
) -> Result<CompleteCombinedOracleForTest, InjectedProfileError> {
    let (run, runtime) = compile_complete_combined_profile_for_test(sources, library_count)?;
    if run.initial_store_rows == 0
        || run.initial_published_type_rows != 0
        || run.initial_replacement_type_rows != 0
        || run.initial_type_param_id != 0
        || run.initial_class_id != 0
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete combined oracle did not start from fixed intrinsics and empty semantic state"
                .to_owned(),
        ));
    }
    let mut normalized_records = run
        .library_records
        .into_iter()
        .map(|(key, record)| {
            let normalized = match record {
                CheckerRecord::Diagnostic(diagnostic) => {
                    format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                }
                CheckerRecord::Incomplete(incomplete) => format!("incomplete {incomplete:?}"),
            };
            (key.file_ordinal, normalized)
        })
        .collect::<Vec<_>>();
    for (file_ordinal, record) in run.ordered_user_records {
        let normalized = match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
            }
            CheckerRecord::Incomplete(incomplete) => format!("incomplete {incomplete:?}"),
        };
        normalized_records.push((file_ordinal, normalized));
    }
    Ok(CompleteCombinedOracleForTest {
        normalized_records,
        normalized_root_projection: runtime.normalized_root_projection_for_test(root_names),
        normalized_semantic_identities: runtime.normalized_semantic_identities_for_test(),
        initial_store_rows: run.initial_store_rows,
    })
}

fn with_canonical_library_frontend<'source, Output>(
    sources: &[InjectedLibrarySource<'source>],
    consume: impl for<'ast> FnOnce(
        CanonicalLibraryFrontend<'source, 'ast>,
    ) -> Result<Output, InjectedProfileError>,
) -> Result<Output, InjectedProfileError> {
    with_canonical_frontend(sources, None, consume)
}

fn with_canonical_frontend<'source, Output>(
    sources: &[InjectedLibrarySource<'source>],
    user_start: Option<usize>,
    consume: impl for<'ast> FnOnce(
        CanonicalLibraryFrontend<'source, 'ast>,
    ) -> Result<Output, InjectedProfileError>,
) -> Result<Output, InjectedProfileError> {
    #[cfg(any(test, feature = "test-utils"))]
    let parse_started = Instant::now();
    let mut canonical = canonical_inputs(sources)?;
    if let Some(user_start) = user_start {
        if user_start == 0 || user_start > canonical.len() {
            return Err(InjectedProfileError::EmptyProfile);
        }
        for (index, input) in canonical.iter_mut().enumerate().skip(user_start) {
            input.origin = CompilationOrigin::User(crate::source::OriginalModuleOrdinal::new(
                index - user_start,
            ));
        }
    }
    let allocators = (0..canonical.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed_and_claims = allocators
        .iter()
        .zip(&canonical)
        .map(|(allocator, input)| {
            let source_type = if input.kind.is_declaration() {
                SourceType::d_ts()
            } else {
                SourceType::ts()
            };
            let parsed = Parser::new(allocator, input.source, source_type).parse();
            if parsed.panicked {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: if parsed.diagnostics.is_empty() {
                        vec!["parser panicked without a diagnostic".to_owned()]
                    } else {
                        parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect()
                    },
                });
            }
            let mut claims = Vec::with_capacity(parsed.diagnostics.len());
            for diagnostic in &parsed.diagnostics {
                let code_is_ts1319 = diagnostic.code.scope.as_deref() == Some("TS")
                    && diagnostic.code.number.as_deref() == Some("1319");
                let [label] = diagnostic.labels.as_slice() else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                let start = label.offset();
                let Some(end) = start.checked_add(label.len()) else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                if !code_is_ts1319 {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                }
                claims.push(ParserExportClaim {
                    file_ordinal: input.file_ordinal,
                    span: Span::new(start, end),
                });
            }
            Ok((parsed, claims))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut parser_returns = Vec::with_capacity(parsed_and_claims.len());
    let mut claims = Vec::with_capacity(parsed_and_claims.len());
    for (parsed_unit, claims_unit) in parsed_and_claims {
        parser_returns.push(parsed_unit);
        claims.push(claims_unit);
    }
    let parser_export_claims = claims.into_iter().flatten().collect::<Vec<_>>();
    let parsed = parser_returns
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let parse_elapsed = parse_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let bind_started = Instant::now();
    let units = parsed
        .iter()
        .zip(&canonical)
        .map(|(program, input)| {
            (
                *program,
                CompilationUnit {
                    source: input.source_key,
                    origin: input.origin,
                    binding: ModuleBindingContext::for_program(program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let (user_units, user_events, combined_lexical_events) = if let Some(user_start) = user_start {
        let mut user_units = Vec::with_capacity(units.len().saturating_sub(user_start));
        let mut user_events = EventStore::default();
        let mut reservations: LexicalReservations<PrivateCombinedRecordTicket> =
            LexicalReservations::default();
        for (user_index, (program, unit)) in units.iter().skip(user_start).enumerate() {
            let module_ordinal = ModuleOrdinal::new(user_index);
            let unit_slot = UnitSlot::new(user_index);
            reservations
                .reserve_private_continuation_program(
                    module_ordinal,
                    unit_slot,
                    program,
                    unit.binding,
                    &mut user_events,
                )
                .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
            user_units.push((module_ordinal, unit_slot));
        }
        (user_units, user_events, Some(reservations))
    } else {
        (Vec::new(), EventStore::default(), None)
    };
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let library_count = user_start.unwrap_or(units.len());
    let mut module_scopes = builder
        .try_add_library_modules(&units[..library_count])
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let mut library_binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    let collision_root_provenance =
        library_binder
            .take_library_root_projection()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "library binder did not retain its one-pass root projection".to_owned(),
                )
            })?;
    let library_units = canonical[..library_count]
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(input, module)| LibraryBinderUnit {
            ordinal: input.file_ordinal,
            source: input.source_key,
            module,
        })
        .collect();
    let checkpoint = build_library_binder_checkpoint(library_binder, library_units);
    let (mut builder, _) = checkpoint.into_continuation();
    let mut appended_module = None;
    let mut module_placeholders = Vec::with_capacity(units.len().saturating_sub(library_count));
    if library_count < units.len() {
        builder.reserve_script_namespace_roots(units[library_count..].iter().copied());
        for (program, unit) in &units[library_count..] {
            let (module, placeholders) = builder.add_module(program, &[], *unit);
            module_scopes.push(module);
            module_placeholders.push(placeholders);
            appended_module = Some(module);
        }
    }
    let binder = builder
        .finish_frozen_library_continuation(appended_module)
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
    validate_parser_export_claims(&binder, parser_export_claims, canonical[0].file_ordinal)?;
    let semantic_scopes = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|((_, unit), module)| {
            if matches!(unit.origin, CompilationOrigin::User(_)) || unit.binding.external_module {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let bind_elapsed = bind_started.elapsed();
    #[cfg(any(test, feature = "test-utils"))]
    {
        CANONICAL_FRONTEND_ENTRIES.set(CANONICAL_FRONTEND_ENTRIES.get().saturating_add(1));
        CANONICAL_FRONTEND_PARSE_UNITS.set(
            CANONICAL_FRONTEND_PARSE_UNITS
                .get()
                .saturating_add(u64::try_from(parsed.len()).unwrap_or(u64::MAX)),
        );
        CANONICAL_FRONTEND_BIND_BATCHES
            .set(CANONICAL_FRONTEND_BIND_BATCHES.get().saturating_add(1));
        CANONICAL_FRONTEND_BIND_UNITS.set(
            CANONICAL_FRONTEND_BIND_UNITS
                .get()
                .saturating_add(u64::try_from(module_scopes.len()).unwrap_or(u64::MAX)),
        );
    }
    consume(CanonicalLibraryFrontend {
        canonical,
        parsed,
        binder,
        module_scopes,
        semantic_scopes,
        user_start,
        user_units,
        user_events,
        combined_lexical_events,
        external_effects: BTreeMap::new(),
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed,
    })
}

/// Whether a compile assembles the legacy full oracle or retains the direct compact plan.
///
/// Full assembly remains a test oracle. Production consumes the live source trace into the compact
/// plan retained beside the frozen base (ADR-0020).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayIndexPlan {
    /// Assemble the index as part of this compile.
    Assemble,
    /// Skip full assembly and consume the live trace into the compact plan.
    Deferred,
    /// Complete combined-source oracle: retain no replay product.
    None,
}

/// Whether a compile hands its own records back, or drops them with the ledger.
///
/// The library's records are not user-facing errors — they are typokat's own model gaps
/// against a library real `tsc` checks clean — so no process retains them and the pinned
/// suite census is their sole witness (ADR-0018). Every compile completes and fingerprints the
/// ledger; the drop route releases each record payload immediately after fingerprinting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryRecordRetention {
    /// Drop the records with the ledger. The production route to a base uses this.
    Drop,
    /// Hand the records back to a caller that asked for them explicitly.
    Collect,
}

/// Compile a profile into its complete runtime product, collision replay index included.
pub fn compile_owned_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Assemble,
            LibraryRecordRetention::Collect,
            OwnerSiteStorageMode::Flat,
        )
        .map(|(run, runtime, _)| (run, runtime))
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_source_replay_runtime_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<OwnedLibraryRuntimeState, String> {
    let (_, mut runtime, plan) = compile_owned_injected_base_profile_with_plan(sources)
        .map_err(|error| format!("{error:?}"))?;
    runtime.freeze_as_library_base().map_err(str::to_owned)?;
    let permit = acquire_private_collision_replay_permit().map_err(str::to_owned)?;
    runtime
        .fork_sparse_collision_epoch()
        .map_err(str::to_owned)?
        .install_complete_source_replay_plan_with_permit(plan, &[], permit)
        .map_err(str::to_owned)
}

pub fn compile_complete_source_replay_runtime(
    sources: &[InjectedLibrarySource<'_>],
    seeds: &[PrivateCollisionReplaySeed],
    permit: PrivateCollisionReplayPermitToken,
) -> Result<(OwnedLibraryRuntimeState, LibraryBinderCheckpoint), String> {
    let (_, mut runtime, plan) = compile_owned_injected_base_profile_with_plan(sources)
        .map_err(|error| format!("{error:?}"))?;
    runtime.freeze_as_library_base().map_err(str::to_owned)?;
    let checkpoint = runtime
        .complete_replay_binder_checkpoint()
        .map_err(str::to_owned)?;
    let runtime = runtime
        .fork_sparse_collision_epoch()
        .map_err(str::to_owned)?
        .install_complete_source_replay_plan_with_permit(plan, seeds, permit)
        .map_err(str::to_owned)?;
    Ok((runtime, checkpoint))
}

/// Compile a profile into the runtime state a shared default-library base is sealed from.
///
/// Full-oracle assembly is skipped; the direct compact plan is retained with the base (ADR-0020),
/// and the library's own records are dropped rather than carried into it (ADR-0018).
pub fn compile_owned_injected_base_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    compile_owned_injected_base_profile_with_plan(sources).map(|(run, runtime, _)| (run, runtime))
}

pub fn compile_owned_injected_base_profile_with_plan(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            OwnerSiteStorageMode::Flat,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_owned_injected_base_profile_with_ordered_owner_sites_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            OwnerSiteStorageMode::Ordered,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_owned_injected_base_profile_with_nested_owner_sites_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            OwnerSiteStorageMode::Nested,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn force_collision_plan_failure_for_test(
    sources: &[InjectedLibrarySource<'_>],
    failure: ForcedCollisionPlanFailure,
) -> Result<bool, String> {
    if FORCED_COLLISION_PLAN_FAILURE
        .replace(Some(failure))
        .is_some()
    {
        return Err("forced collision-plan failure is already active".to_owned());
    }
    let _scope = ForcedCollisionPlanFailureScope;
    let _event_capture_scope = if failure == ForcedCollisionPlanFailure::EventCaptureCorruption {
        Some(
            super::events_library::EventCaptureCorruptionScope::start()
                .map_err(|message| message.to_owned())?,
        )
    } else {
        None
    };
    match compile_owned_injected_base_profile_with_plan(sources) {
        Ok(_) => Ok(false),
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::TypedReferenceCoverage { .. },
        )) if matches!(
            failure,
            ForcedCollisionPlanFailure::UnownedTypedReference
                | ForcedCollisionPlanFailure::RawSemanticAccess
        ) =>
        {
            Ok(true)
        }
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::BaselinePartitionMismatch,
        )) if failure == ForcedCollisionPlanFailure::ForbiddenProjection => Ok(true),
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::IndependentOwnerSiteOracleMismatch,
        )) if failure == ForcedCollisionPlanFailure::EventCaptureCorruption => Ok(true),
        Err(InjectedProfileError::Reporting(LibraryEventLedgerError::TraceDomainSealed))
            if failure == ForcedCollisionPlanFailure::LateOwnerReservation =>
        {
            Ok(true)
        }
        Err(error) => Err(format!(
            "forced collision-plan failure reached the wrong guard: {error:?}"
        )),
    }
}

/// Compile a profile for its own records alone, then drop the semantic product.
///
/// The deliberate record-inspection route costs a full source compilation and answers only to a
/// caller that explicitly requested retained records (ADR-0018).
pub fn compile_owned_injected_records(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<Vec<(LibraryEventKey, CheckerRecord)>, InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Collect,
            OwnerSiteStorageMode::Flat,
        )
    })
    .map(|(run, _, _)| run.library_records)
}

fn intern_compiled_global_object<Ticket: Copy + PartialEq>(
    pass: &mut super::Pass<'_, '_, Ticket>,
    projection: &LibraryRootProjection,
    additional_contributors: &[(String, ValueStorageId)],
) -> TypeId {
    let mut contributors = projection
        .root_rows
        .iter()
        .filter(|row| {
            projection
                .candidates
                .get(&row.name)
                .is_some_and(|candidate| candidate.global_object_contributor)
        })
        .filter_map(|row| row.value.map(|storage| (row.name.clone(), storage)))
        .collect::<BTreeMap<_, _>>();
    contributors.extend(additional_contributors.iter().cloned());
    if pass.combined_source_library_value_precedence {
        contributors.extend(
            pass.private_collision_value_winners_by_name
                .iter()
                .map(|(name, storage)| (name.clone(), *storage)),
        );
    }
    let mut properties = contributors
        .into_iter()
        .filter(|(name, _)| name != "undefined")
        .filter_map(|(name, storage)| {
            pass.decl_type_replay(storage)
                .map(|ty| PropertyType::public(name, ty))
        })
        .collect::<Vec<_>>();
    properties.push(PropertyType::public(
        "undefined",
        pass.interner.well_known().undefined,
    ));
    pass.interner.intern_object(ObjectType {
        properties,
        ..Default::default()
    })
}

trait InjectedCompileRoute {
    type Ticket: Copy + Ord + super::UserReportingOwner<Error = &'static str>;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError>;

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError>;

    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
    );

    fn attach_user_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        module_ordinal: ModuleOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
    ) -> Result<(), InjectedProfileError>;

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket>;

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize);

    fn user_ticket(owner: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError>;

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        user_events: &mut EventStore,
    ) -> Result<(), InjectedProfileError>;
}

struct LegacyLibraryRoute;

impl InjectedCompileRoute for LegacyLibraryRoute {
    type Ticket = LibraryRecordTicket;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError> {
        if combined.is_some() {
            return Err(InjectedProfileError::CanonicalProjection(
                "library-only compiler received user lexical reservations".to_owned(),
            ));
        }
        Ok(LexicalReservations::default())
    }

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError> {
        reservations.reserve_library_program(file_ordinal, program, ledger)
    }

    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
    ) {
        reservations.attach_library_declaration_owners(file_ordinal, binder, scope, program, spans);
        reservations.attach_library_class_bindings(
            file_ordinal,
            binder,
            scope,
            program,
            declarations,
        );
    }

    fn attach_user_owners(
        _: &mut LexicalReservations<Self::Ticket>,
        _: ModuleOrdinal,
        _: &Binder,
        _: ScopeId,
        _: &Program<'_>,
        _: &super::ModuleDeclarationSpans,
        _: &super::context::TypeDeclTable<'_>,
    ) -> Result<(), InjectedProfileError> {
        Err(InjectedProfileError::CanonicalProjection(
            "library-only compiler received a user source".to_owned(),
        ))
    }

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket> {
        reservations.library_semantic_tickets()
    }

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize) {
        library_record_ticket_key
    }

    fn user_ticket(_: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError> {
        Err(InjectedProfileError::CanonicalProjection(
            "library-only compiler received user effects".to_owned(),
        ))
    }

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        _: &mut EventStore,
    ) -> Result<(), InjectedProfileError> {
        LibrarySemanticReportingAdapter::new(ledger)
            .complete_semantic_batches(batches)
            .map_err(InjectedProfileError::Reporting)
    }
}

struct CompleteSourceRoute;

impl InjectedCompileRoute for CompleteSourceRoute {
    type Ticket = PrivateCombinedRecordTicket;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError> {
        combined.ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "complete-source compiler lost user lexical reservations".to_owned(),
            )
        })
    }

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError> {
        reservations.reserve_complete_library_program(file_ordinal, program, ledger)
    }

    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
    ) {
        reservations.attach_complete_library_declaration_owners(
            file_ordinal,
            binder,
            scope,
            program,
            spans,
        );
        reservations.attach_complete_library_class_bindings(
            file_ordinal,
            binder,
            scope,
            program,
            declarations,
        );
    }

    fn attach_user_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        module_ordinal: ModuleOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
    ) -> Result<(), InjectedProfileError> {
        attach_type_decl_owners(
            reservations,
            SourceOrdinal::User(module_ordinal),
            binder,
            scope,
            program,
            spans,
        );
        attach_class_bindings(
            reservations,
            SourceOrdinal::User(module_ordinal),
            binder,
            scope,
            program,
            declarations,
            None,
        );
        Ok(())
    }

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket> {
        let mut tickets = reservations.source_anchor_tickets();
        tickets.extend(reservations.tickets());
        tickets
    }

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize) {
        private_combined_record_ticket_key
    }

    fn user_ticket(owner: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError> {
        Ok(PrivateCombinedRecordTicket::User(owner))
    }

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        user_events: &mut EventStore,
    ) -> Result<(), InjectedProfileError> {
        for batch in batches {
            let (owner, records) = batch.into_parts();
            match owner {
                PrivateCombinedRecordTicket::Library(owner) => ledger
                    .complete(owner, records)
                    .map_err(InjectedProfileError::Reporting)?,
                PrivateCombinedRecordTicket::DisabledLibrary(_) => {}
                PrivateCombinedRecordTicket::User(owner) => user_events
                    .complete(owner, records)
                    .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?,
            }
        }
        Ok(())
    }
}

fn compile_owned_injected_frontend(
    frontend: CanonicalLibraryFrontend<'_, '_>,
    replay_index_plan: ReplayIndexPlan,
    record_retention: LibraryRecordRetention,
    owner_site_storage_mode: OwnerSiteStorageMode,
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        Option<std::sync::Arc<CollisionReplayPlan>>,
    ),
    InjectedProfileError,
> {
    if frontend.user_start.is_some() {
        compile_owned_injected_frontend_for_route::<CompleteSourceRoute>(
            frontend,
            replay_index_plan,
            record_retention,
            owner_site_storage_mode,
        )
    } else {
        compile_owned_injected_frontend_for_route::<LegacyLibraryRoute>(
            frontend,
            replay_index_plan,
            record_retention,
            owner_site_storage_mode,
        )
    }
}

fn validate_complete_source_user_units(
    canonical_len: usize,
    user_start: Option<usize>,
    user_units: &[(ModuleOrdinal, UnitSlot)],
) -> Result<(), InjectedProfileError> {
    if user_start.is_some_and(|start| canonical_len.saturating_sub(start) != user_units.len()) {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user identities do not match the parsed suffix".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some((module_ordinal, _)) = user_units
        .iter()
        .find(|(module_ordinal, _)| !seen.insert(*module_ordinal))
    {
        return Err(InjectedProfileError::CanonicalProjection(format!(
            "complete-source user module ordinal {} is duplicated",
            module_ordinal.index()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
fn complete_source_user_file_ordinals_for_test(
    canonical: &[CanonicalInput<'_>],
    user_start: Option<usize>,
    user_units: &[(ModuleOrdinal, UnitSlot)],
) -> Result<BTreeMap<ModuleOrdinal, LibraryFileOrdinal>, InjectedProfileError> {
    let Some(user_start) = user_start else {
        if user_units.is_empty() {
            return Ok(BTreeMap::new());
        }
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping omits the suffix".to_owned(),
        ));
    };
    let suffix = canonical.get(user_start..).ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping starts outside the canonical inputs"
                .to_owned(),
        )
    })?;
    if suffix.len() != user_units.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping is incomplete".to_owned(),
        ));
    }
    let mut expected_modules = BTreeSet::new();
    for (module_ordinal, _) in user_units {
        if !expected_modules.insert(*module_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates module ordinal {}",
                module_ordinal.index()
            )));
        }
    }
    let mut file_ordinals = BTreeMap::new();
    let mut seen_file_ordinals = BTreeSet::new();
    for input in suffix {
        let CompilationOrigin::User(original_module_ordinal) = input.origin else {
            return Err(InjectedProfileError::CanonicalProjection(
                "complete-source user file ordinal mapping contains a library origin".to_owned(),
            ));
        };
        let module_ordinal = ModuleOrdinal::new(original_module_ordinal.index());
        if !expected_modules.contains(&module_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping contains unexpected module ordinal {}",
                module_ordinal.index()
            )));
        }
        if file_ordinals
            .insert(module_ordinal, input.file_ordinal)
            .is_some()
        {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates module ordinal {}",
                module_ordinal.index()
            )));
        }
        if !seen_file_ordinals.insert(input.file_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates file ordinal {}",
                input.file_ordinal.index()
            )));
        }
    }
    if file_ordinals.len() != expected_modules.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping omits a module ordinal".to_owned(),
        ));
    }
    Ok(file_ordinals)
}

fn assemble_complete_source_user_results(
    user_units: &[(ModuleOrdinal, UnitSlot)],
    mut user_records: BTreeMap<ModuleOrdinal, Vec<CheckerRecord>>,
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    let mut results = Vec::with_capacity(user_units.len());
    for (module_ordinal, unit_slot) in user_units {
        let mut diagnostics = Vec::new();
        let mut incomplete = Vec::new();
        for record in user_records.remove(module_ordinal).unwrap_or_default() {
            match record {
                CheckerRecord::Diagnostic(diagnostic) => diagnostics.push(diagnostic),
                CheckerRecord::Incomplete(record) => incomplete.push(record),
            }
        }
        results.push(super::CheckResult {
            module_ordinal: *module_ordinal,
            unit_slot: *unit_slot,
            diagnostics,
            incomplete,
        });
    }
    if let Some(module_ordinal) = user_records.keys().next() {
        return Err(InjectedProfileError::CanonicalProjection(format!(
            "complete-source retained records for unexpected user module ordinal {}",
            module_ordinal.index()
        )));
    }
    Ok(results)
}

fn compile_owned_injected_frontend_for_route<Route: InjectedCompileRoute>(
    frontend: CanonicalLibraryFrontend<'_, '_>,
    replay_index_plan: ReplayIndexPlan,
    record_retention: LibraryRecordRetention,
    owner_site_storage_mode: OwnerSiteStorageMode,
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        Option<std::sync::Arc<CollisionReplayPlan>>,
    ),
    InjectedProfileError,
> {
    let collision_plan_work_scope = CollisionPlanForbiddenWorkScope::start();
    record_collision_plan_forbidden_work(|work| {
        work.library_source_compiles = work.library_source_compiles.saturating_add(1);
    });
    let CanonicalLibraryFrontend {
        canonical,
        parsed,
        binder,
        module_scopes,
        semantic_scopes,
        user_start,
        user_units,
        mut user_events,
        combined_lexical_events,
        mut external_effects,
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed,
    } = frontend;
    validate_complete_source_user_units(canonical.len(), user_start, &user_units)?;
    #[cfg(any(test, feature = "test-utils"))]
    let user_file_ordinals =
        complete_source_user_file_ordinals_for_test(&canonical, user_start, &user_units)?;
    let source_unit_for_index = |index: usize, input: &CanonicalInput<'_>| {
        user_start
            .and_then(|start| index.checked_sub(start))
            .and_then(|user_index| user_units.get(user_index).copied())
            .map_or_else(
                || library_unit(input.file_ordinal),
                |(module_ordinal, unit_slot)| SourceUnit::User {
                    module_ordinal,
                    unit_slot,
                },
            )
    };
    #[cfg(any(test, feature = "test-utils"))]
    CANONICAL_FRONTEND_FULL_PRODUCTS.set(CANONICAL_FRONTEND_FULL_PRODUCTS.get().saturating_add(1));
    #[cfg(any(test, feature = "test-utils"))]
    let independent_event_owner_sites = if replay_index_plan == ReplayIndexPlan::None {
        Vec::new()
    } else {
        independent_event_owner_sites_for_oracle(&canonical, &parsed, &binder)?
    };

    #[cfg(any(test, feature = "test-utils"))]
    let reserve_fill_started = Instant::now();
    let retain_library_records = matches!(record_retention, LibraryRecordRetention::Collect);
    let mut ledger = if replay_index_plan == ReplayIndexPlan::None {
        LibraryEventLedger::new_without_replay(retain_library_records)
    } else {
        LibraryEventLedger::new_with_owner_site_storage(
            retain_library_records,
            owner_site_storage_mode,
        )
    };
    let mut lexical_events = Route::lexical_events(combined_lexical_events)?;
    for (index, (input, program)) in canonical.iter().zip(&parsed).enumerate() {
        if user_start.is_some_and(|user_start| index >= user_start) {
            continue;
        }
        Route::reserve_library_program(
            &mut lexical_events,
            input.file_ordinal,
            program,
            &mut ledger,
        )
        .map_err(InjectedProfileError::Reporting)?;
    }

    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls: super::context::TypeDeclTable<'_> = Vec::new().into();
    let mut type_resolved: super::context::TypeResolvedTable =
        vec![None; binder.type_groups.len()].into();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_store_rows = interner.store().len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_published_type_rows = type_decls.published_len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_replacement_type_rows = type_decls.replacement_indices().len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_type_param_id = next_type_param;
    #[cfg(any(test, feature = "test-utils"))]
    let initial_class_id = next_class_id;
    let declaration_spans = super::ModuleDeclarationSpans::index(&binder);
    let mut combined_library_declarations = None;
    for (index, ((input, program), scope)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .enumerate()
    {
        if user_start.is_some() && matches!(input.origin, CompilationOrigin::Library(_)) {
            reserve_type_decls_for_combined_library(
                super::decls::TypeDeclReservationState {
                    interner: &mut interner,
                    binder: &binder,
                    next_type_param: &mut next_type_param,
                    next_class_id: &mut next_class_id,
                    decls: &mut type_decls,
                    resolved: &mut type_resolved,
                },
                scope,
                program,
            );
        } else if user_start.is_some() {
            if combined_library_declarations.is_none() {
                combined_library_declarations = Some(
                    binder
                        .declarations
                        .iter()
                        .filter(|declaration| {
                            lexical_events
                                .declaration_source(declaration.id)
                                .is_some_and(|source| {
                                    matches!(source.unit, SourceUnit::Library { .. })
                                })
                        })
                        .map(|declaration| declaration.id)
                        .collect::<rustc_hash::FxHashSet<_>>(),
                );
            }
            let Some(library_declarations) = combined_library_declarations.as_ref() else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "complete-source library declaration provenance is missing".to_owned(),
                ));
            };
            reserve_type_decls_for_combined_user(
                super::decls::TypeDeclReservationState {
                    interner: &mut interner,
                    binder: &binder,
                    next_type_param: &mut next_type_param,
                    next_class_id: &mut next_class_id,
                    decls: &mut type_decls,
                    resolved: &mut type_resolved,
                },
                scope,
                program,
                library_declarations,
            );
        } else {
            reserve_type_decls(
                &mut interner,
                &binder,
                scope,
                program,
                &mut next_type_param,
                &mut next_class_id,
                &mut type_decls,
                &mut type_resolved,
            );
        }
        if let Some(user_index) = user_start.and_then(|start| index.checked_sub(start)) {
            let Some((module_ordinal, _)) = user_units.get(user_index).copied() else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "complete-source user unit identity is missing".to_owned(),
                ));
            };
            Route::attach_user_owners(
                &mut lexical_events,
                module_ordinal,
                &binder,
                scope,
                program,
                &declaration_spans,
                &type_decls,
            )?;
        } else {
            Route::attach_library_owners(
                &mut lexical_events,
                input.file_ordinal,
                &binder,
                scope,
                program,
                &declaration_spans,
                &type_decls,
            );
        }
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    seed_trusted_library_markers(
        &collision_root_provenance,
        &mut type_decls,
        &mut type_resolved,
        &interner,
    );

    let replay_class_declarations = if replay_index_plan == ReplayIndexPlan::Assemble {
        lexical_events
            .classes()
            .iter()
            .filter_map(|reservation| {
                let binding = reservation.binding.as_ref()?;
                let TypeDecl::Class { declaration, .. } =
                    type_decls.get(binding.type_decl.index())?
                else {
                    return None;
                };
                Some((binding.class_id, *declaration))
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };

    let reporting_receipts = LibraryReportingConsumer::new(&mut ledger)
        .consume_binder_outcomes(&binder)
        .map_err(InjectedProfileError::Reporting)?;
    #[cfg(not(any(test, feature = "test-utils")))]
    let _ = &reporting_receipts;
    if user_start.is_some() {
        enqueue_local_ambient_export_alias_diagnostics(
            &binder,
            &lexical_events,
            &mut external_effects,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
        enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects)
            .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
        enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects)
            .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
    }
    let mut decl_types = DeclTypes::new(binder.decl_count);
    let error = interner.well_known().error;
    super::seed_module_placeholder_errors(&mut decl_types, &module_placeholders, error);
    let user_declaration_contributors = binder
        .declarations
        .iter()
        .filter(|declaration| {
            lexical_events
                .declaration_source(declaration.id)
                .is_some_and(|source| matches!(source.unit, SourceUnit::User { .. }))
        })
        .map(|declaration| declaration.id)
        .collect::<rustc_hash::FxHashSet<_>>();
    let user_value_contributors = binder
        .declarations
        .iter()
        .filter(|declaration| user_declaration_contributors.contains(&declaration.id))
        .filter_map(|declaration| declaration.value_storage)
        .collect::<rustc_hash::FxHashSet<ValueStorageId>>();
    let pending_tickets = Route::pending_tickets(&lexical_events);
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(canonical[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
            ticket_key: Route::ticket_key(),
        },
    );
    if user_start.is_some() {
        pass.install_early_native_array_groups(NativeArrayGroups::select_from_library_roots(
            &collision_root_provenance,
        ));
    }
    for effects in external_effects.into_values() {
        let (owner, records) = effects.into_parts();
        let mut combined = super::context::CheckerEffects::new(Route::user_ticket(owner)?);
        for record in records {
            combined.records.record(record);
        }
        pass.enqueue_effects(combined);
    }
    pass.certified_library_values = certify_library_values(&collision_root_provenance);
    if user_start.is_some() {
        pass.combined_source_library_value_precedence = true;
        for group in collision_root_provenance
            .root_rows
            .iter()
            .filter_map(|row| row.ty)
        {
            let user_fragment = binder.type_groups.get(group).is_some_and(|group| {
                group
                    .fragments
                    .iter()
                    .any(|fragment| user_declaration_contributors.contains(&fragment.declaration))
            });
            if user_fragment {
                pass.combined_user_library_type_groups.insert(group);
            }
        }
        for row in &collision_root_provenance.root_rows {
            let Some(library_participant) = row.value else {
                continue;
            };
            let Some(symbol_id) = pass.resolve_value_replay(binder.compilation_global, &row.name)
            else {
                continue;
            };
            let Some(symbol) = binder.symbols.get(symbol_id) else {
                continue;
            };
            if symbol.value.is_some_and(|current| {
                current != library_participant && user_value_contributors.contains(&current)
            }) {
                pass.private_collision_value_winners
                    .insert(symbol_id, library_participant);
                pass.private_collision_value_winners_by_name
                    .insert(row.name.clone(), library_participant);
            }
            if symbol.function_values.len() > 1
                && symbol.function_values.contains(&library_participant)
                && symbol
                    .function_values
                    .iter()
                    .any(|participant| user_value_contributors.contains(participant))
            {
                pass.function_group_precedence_tails_by_name
                    .insert(row.name.clone(), library_participant);
            }
        }
    }
    if replay_index_plan == ReplayIndexPlan::None {
        ledger
            .seal_reporting_without_replay()
            .map_err(InjectedProfileError::Reporting)?;
    } else {
        let mut replay_trace_seed = ledger
            .take_replay_trace_seed()
            .map_err(InjectedProfileError::Reporting)?;
        #[cfg(any(test, feature = "test-utils"))]
        if FORCED_COLLISION_PLAN_FAILURE.get()
            == Some(ForcedCollisionPlanFailure::LateOwnerReservation)
        {
            let _late = ledger
                .replay_reservation_domain()
                .map_err(InjectedProfileError::Reporting)?;
        }
        replay_trace_seed.extend_owner_sites(pass.lexical_events.take_collision_owner_sites());
        pass.replay_trace = Some(ReplayDependencyTrace::new(replay_trace_seed));
    }
    pass.capture_compact_replay_dependencies = replay_index_plan != ReplayIndexPlan::None;

    let declaration_count = pass.type_decls.len();
    pass.fill_type_decls_range(binder.module, 0, declaration_count);
    #[cfg(any(test, feature = "test-utils"))]
    let reserve_fill_elapsed = reserve_fill_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let publication_validation_started = Instant::now();
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, program)| (scope, program.body.as_slice()))
        .collect::<Vec<_>>();
    if let Some(user_start) = user_start {
        for (index, ((scope, program), input)) in module_scopes
            .iter()
            .copied()
            .zip(&parsed)
            .zip(&canonical)
            .enumerate()
            .skip(user_start)
        {
            pass.current_module = scope;
            pass.current_source = source_unit_for_index(index, input);
            pass.reserve_local_type_annotation_surfaces(scope, &program.body);
        }
    }
    pass.combined_user_source = user_start.is_some();
    pass.prepare_project_attached_namespace_values(&module_programs);
    pass.prepare_project_standalone_namespace_values(&module_programs);
    pass.combined_user_source = false;
    pass.publish_class_surfaces();
    if user_start.is_none() {
        pass.finalize_standalone_namespace_values();
        pass.precompute_standalone_namespace_value_aliases(&module_programs);
    }
    pass.fill_pending_interfaces_range(binder.module, 0, declaration_count);
    if let Some(user_start) = user_start {
        let user_namespace_modules = module_scopes[user_start..]
            .iter()
            .copied()
            .collect::<rustc_hash::FxHashSet<_>>();
        pass.refresh_colliding_standalone_variable_surfaces(
            &module_programs,
            &user_namespace_modules,
        );
        pass.finalize_standalone_namespace_values();
        pass.precompute_standalone_namespace_value_aliases(&module_programs);
    }
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        return Err(InjectedProfileError::LibraryIdentitySelectionPending);
    }
    let publication_validations = publication.publication_validations();
    pass.validate_published_class_surfaces();
    #[cfg(any(test, feature = "test-utils"))]
    let lexical_source_units = pass
        .lexical_events
        .retained_source_units()
        .into_iter()
        .filter(|unit| matches!(unit, crate::source::SourceUnit::Library { .. }))
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let publication_validation_elapsed = publication_validation_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let statement_check_started = Instant::now();
    let mut pass_source_units = Vec::with_capacity(canonical.len());
    let mut mixed_semantic_identities = None;
    let mut user_global_contributors = Vec::new();
    let mut user_global_contributor_names = Vec::new();
    if let Some(user_start) = user_start {
        let identities = LibrarySemanticIdentities::select_from_library_roots(
            &collision_root_provenance,
            pass.type_environment.published(),
            pass.interner.store(),
        );
        pass.install_library_semantic_identities(identities.clone());
        mixed_semantic_identities = Some(identities);
        let base_global_object = pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(pass, &collision_root_provenance, &[])
        });
        pass.global_object_type = Some(base_global_object);

        for (((user_input, user_program), _user_module), semantic_scope) in canonical
            .iter()
            .zip(&parsed)
            .zip(module_scopes.iter().copied())
            .zip(semantic_scopes.iter().copied())
            .skip(user_start)
        {
            let census = source_global_binding_census_with_provenance(
                user_program,
                ModuleBindingContext::for_program(user_program, user_input.kind),
            );
            let mut contributor_names = rustc_hash::FxHashSet::default();
            for (name, candidate) in census.census.candidates {
                if name == "globalThis" || !candidate.global_object_contributor {
                    continue;
                }
                contributor_names.insert(name.clone());
                let Some(storage) = pass.value_decl_id_replay(semantic_scope, &name) else {
                    continue;
                };
                user_global_contributors.push((name, storage));
            }
            user_global_contributor_names.push(contributor_names);
        }
    }
    let mut surfaces = Vec::with_capacity(canonical.len());
    for (index, (((input, program), module), semantic_scope)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .enumerate()
    {
        pass.combined_user_source = user_start.is_some_and(|user_start| index >= user_start);
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        let mut reserved = pass.reserve_function_surfaces(semantic_scope, &program.body);
        pass.reserve_var_annotation_surfaces(semantic_scope, &program.body);
        pass.reserve_continuation_global_augmentation_surfaces(&program.body, &mut reserved);
        surfaces.push(reserved);
    }
    pass.combined_user_source = false;
    if let Some(user_start) = user_start {
        let global_object = pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(
                pass,
                &collision_root_provenance,
                &user_global_contributors,
            )
        });
        pass.global_object_type = Some(global_object);
        pass.refresh_user_global_object(
            canonical
                .iter()
                .zip(&parsed)
                .zip(module_scopes.iter().copied())
                .skip(user_start)
                .map(|((input, program), module)| {
                    (
                        module,
                        *program,
                        ModuleBindingContext::for_program(program, input.kind),
                    )
                }),
        );
    }
    for (index, ((((input, program), module), semantic_scope), mut reserved)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .zip(surfaces)
        .enumerate()
    {
        if user_start == Some(index) {
            pass.combined_user_source = true;
        }
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        pass_source_units.push(pass.current_source);
        pass.build_flow_graph(semantic_scope, &program.body);
        let mut no_return = None;
        if let Some(contributor_names) = user_start
            .and_then(|start| index.checked_sub(start))
            .and_then(|user_index| user_global_contributor_names.get(user_index))
        {
            pass.check_statement_list_with_global_contributors(
                semantic_scope,
                &program.body,
                None,
                &mut no_return,
                &mut reserved,
                contributor_names,
            );
        } else {
            pass.check_statement_list_with_surfaces(
                semantic_scope,
                &program.body,
                None,
                &mut no_return,
                &mut reserved,
            );
        }
        if matches!(pass.current_source, SourceUnit::User { .. }) {
            super::emit_test_incomplete(&mut pass);
        }
    }
    let batches = finish_semantic_effects(&mut pass);
    let semantic_identities = mixed_semantic_identities.unwrap_or_else(|| {
        LibrarySemanticIdentities::select(
            &binder,
            pass.type_environment.published(),
            pass.interner.store(),
        )
    });
    #[cfg(any(test, feature = "test-utils"))]
    let (global_types, module_types) = collect_type_probes(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
        &canonical,
        &module_scopes,
    );
    #[cfg(any(test, feature = "test-utils"))]
    let global_values = collect_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );
    Route::complete_batches(batches, &mut ledger, &mut user_events)?;
    let global_object_type = pass.global_object_type.or_else(|| {
        Some(pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(pass, &collision_root_provenance, &[])
        }))
    });
    let snapshot = ledger.snapshot();
    let ledger_output = if replay_index_plan == ReplayIndexPlan::None {
        super::events_library::LibraryEventLedgerOutput {
            records: ledger.finish().map_err(InjectedProfileError::Reporting)?,
            fingerprints: Vec::new(),
        }
    } else {
        ledger
            .finish_with_fingerprints()
            .map_err(InjectedProfileError::Reporting)?
    };
    let library_records = ledger_output.records;
    let ordered_user_records = user_events
        .finish()
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    #[cfg(any(test, feature = "test-utils"))]
    let ordered_user_record_evidence = ordered_user_records
        .iter()
        .map(|(key, record)| {
            let file_ordinal = user_file_ordinals
                .get(&key.module_ordinal)
                .copied()
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection(format!(
                        "complete-source user file ordinal mapping omits module ordinal {}",
                        key.module_ordinal.index()
                    ))
                })?;
            Ok((file_ordinal, record.clone()))
        })
        .collect::<Result<Vec<_>, InjectedProfileError>>()?;
    let user_records = ordered_user_records.into_iter().fold(
        BTreeMap::<ModuleOrdinal, Vec<_>>::new(),
        |mut records, (key, record)| {
            records.entry(key.module_ordinal).or_default().push(record);
            records
        },
    );
    let user_results = assemble_complete_source_user_results(&user_units, user_records)?;
    #[cfg(any(test, feature = "test-utils"))]
    if let Some(library_count) = user_start {
        let evidence_sources = canonical[..library_count]
            .iter()
            .map(|input| InjectedLibrarySource {
                file_ordinal: input.file_ordinal,
                name: "",
                source: input.source,
            })
            .collect::<Vec<_>>();
        let evidence = canonical_library_evidence_for_test(&evidence_sources, &library_records)?;
        record_complete_source_evidence_for_test(evidence);
    }
    let replay_baselines = ledger_output
        .fingerprints
        .into_iter()
        .map(|fingerprint| super::replay_index::ReplayBaselineRecord {
            owner: ReplayOwner::Statement(fingerprint.key),
            record_count: fingerprint.record_count,
            digest: fingerprint.digest,
        })
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let statement_check_elapsed = statement_check_started.elapsed();

    let namespace_terminals = pass
        .namespace_values
        .try_freeze_terminals()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    let mut replay_trace = pass.replay_trace.take();
    if replay_index_plan != ReplayIndexPlan::None && replay_trace.is_none() {
        return Err(InjectedProfileError::CanonicalProjection(
            "source library compilation lost its replay trace".to_owned(),
        ));
    }
    let compact_only_replay_edges = pass.compact_only_replay_edges.borrow().clone();
    if replay_index_plan == ReplayIndexPlan::Assemble {
        if let Some(trace) = replay_trace.as_ref() {
            trace.remove_direct_dependencies(&compact_only_replay_edges);
        }
    }
    #[cfg(any(test, feature = "test-utils"))]
    if let Some(replay_trace) = replay_trace.as_ref() {
        match FORCED_COLLISION_PLAN_FAILURE.get() {
            Some(ForcedCollisionPlanFailure::UnownedTypedReference) => {
                let observation =
                    replay_trace.observe_typed_demand("forced-unowned-typed-reference");
                drop(observation);
            }
            Some(ForcedCollisionPlanFailure::RawSemanticAccess) => {
                replay_trace.record_raw_semantic_access();
            }
            Some(ForcedCollisionPlanFailure::ForbiddenProjection) => {
                let _ = snapshot_namespace_terminals_for_replay(&namespace_terminals)?;
            }
            Some(
                ForcedCollisionPlanFailure::EventCaptureCorruption
                | ForcedCollisionPlanFailure::LateOwnerReservation,
            ) => {}
            None => {}
        }
    }
    let super::context::Pass {
        type_environment,
        decl_types,
        next_type_param,
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        function_groups,
        certified_library_values,
        ..
    } = pass;
    let super::type_groups::TypeEnvironmentState::Published(published_types) = type_environment
    else {
        return Err(InjectedProfileError::CanonicalProjection(
            "owned library runtime requires a published environment".to_owned(),
        ));
    };
    let named_function_symbols = function_groups.frozen_symbols();
    let runtime = FrozenCheckerRuntimeMetadata {
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        namespace_terminals,
        named_function_symbols: named_function_symbols.into(),
        global_object_type,
        certified_library_values,
    };
    let selected_semantic_identities = semantic_identities
        .all_ready()
        .then_some(semantic_identities.clone());
    let (replay_index, collision_plan) = match replay_index_plan {
        ReplayIndexPlan::Assemble => {
            let replay_trace = replay_trace.take().ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "full replay assembly lost its source trace".to_owned(),
                )
            })?;
            let namespace_terminal_rows =
                snapshot_namespace_terminals_for_replay(&runtime.namespace_terminals)?;
            let runtime_parts = runtime
                .snapshot_parts()
                .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
            record_collision_plan_forbidden_work(|work| {
                let rows = runtime_parts
                    .class_application_parameters
                    .len()
                    .saturating_add(runtime_parts.class_new_metadata.len())
                    .saturating_add(runtime_parts.class_parents.len())
                    .saturating_add(runtime_parts.class_value_aliases.len())
                    .saturating_add(runtime_parts.class_value_bindings.len())
                    .saturating_add(runtime_parts.standalone_namespace_value_aliases.len())
                    .saturating_add(runtime_parts.class_names.len())
                    .saturating_add(runtime_parts.namespace_terminals.len())
                    .saturating_add(runtime_parts.named_function_symbols.len())
                    .saturating_add(usize::from(runtime_parts.global_object_type.is_some()));
                work.runtime_snapshot_rows = work
                    .runtime_snapshot_rows
                    .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX));
            });
            let replay_index = build_collision_replay_index(
                replay_trace,
                &interner,
                &binder,
                &published_types,
                &decl_types,
                &namespace_terminal_rows,
                &runtime_parts,
                selected_semantic_identities.as_ref(),
                &canonical,
                &parsed,
                &module_scopes,
                &replay_class_declarations,
                &library_records,
                #[cfg(any(test, feature = "test-utils"))]
                independent_event_owner_sites,
            )?;
            #[cfg(any(test, feature = "test-utils"))]
            FULL_COLLISION_PLAN_ORACLE.with(|oracle| {
                if let Some(oracle) = oracle.borrow_mut().as_mut() {
                    oracle
                        .reverse_edges
                        .extend(compact_only_replay_edges.iter().copied());
                    oracle.reverse_edges.sort();
                    oracle.reverse_edges.dedup();
                }
            });
            let replay_roots = collect_root_rows(&binder)
                .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?
                .into_iter()
                .map(|row| (row.name, row.value, row.ty, row.namespace))
                .collect::<Vec<_>>();
            let replay_index = admit_generated_collision_replay_index(
                replay_index,
                ReplayIndexAdmissionLimits {
                    type_groups: binder.type_groups.len(),
                    value_storages: decl_types.len(),
                    namespaces: binder.namespaces.len(),
                    classes: usize::try_from(next_class_id)
                        .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                    source_files: canonical
                        .iter()
                        .map(|input| input.file_ordinal.index().saturating_add(1))
                        .max()
                        .unwrap_or(0),
                    roots: &replay_roots,
                },
                None,
            )
            .map_err(InjectedProfileError::ReplayIndexAdmission)?;
            (Some(Box::new(replay_index)), None)
        }
        // The production path consumes the one live source trace directly into the compact plan.
        ReplayIndexPlan::Deferred => {
            let replay_trace = replay_trace.take().ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "deferred replay plan lost its source trace".to_owned(),
                )
            })?;
            let forbidden_work = collision_plan_work_scope.finish();
            let prefix_cardinalities = [
                interner.store().len(),
                usize::try_from(next_type_param)
                    .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                usize::try_from(next_class_id)
                    .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                binder.graph.len(),
                binder.symbols.len(),
                binder.declarations.len(),
                binder.type_groups.len(),
                binder.namespaces.len(),
                decl_types.len(),
            ];
            let plan = build_collision_replay_plan(
                replay_trace,
                collision_root_provenance,
                replay_baselines,
                prefix_cardinalities,
                forbidden_work,
            )?;
            #[cfg(any(test, feature = "test-utils"))]
            verify_independent_event_owner_sites(&plan, &independent_event_owner_sites)?;
            (None, Some(std::sync::Arc::new(plan)))
        }
        ReplayIndexPlan::None => {
            let _ = collision_plan_work_scope.finish();
            debug_assert!(replay_trace.is_none());
            (None, None)
        }
    };
    let runtime_state = OwnedLibraryRuntimeState {
        interner,
        binder,
        published_types,
        decl_types,
        semantic_identities: selected_semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: u32::try_from(canonical.len())
            .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        library_modules: module_scopes.clone().into(),
        replay_index,
        private_collision_epoch: None,
    };

    let run = InjectedProfileRun {
        phase_counts: LibraryPhaseCounts {
            parse_units: parsed.len(),
            bind_units: module_scopes.len(),
            reserved_records: snapshot.reserved_records,
            filled_records: snapshot.filled_records,
            publication_validations,
            statement_check_units: pass_source_units.len(),
        },
        #[cfg(any(test, feature = "test-utils"))]
        initial_store_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_published_type_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_replacement_type_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_type_param_id,
        #[cfg(any(test, feature = "test-utils"))]
        initial_class_id,
        #[cfg(any(test, feature = "test-utils"))]
        phase_timings: LibraryPhaseTimings {
            parse: parse_elapsed,
            bind: bind_elapsed,
            reserve_fill: reserve_fill_elapsed,
            publication_validation: publication_validation_elapsed,
            statement_check: statement_check_elapsed,
        },
        #[cfg(any(test, feature = "test-utils"))]
        reserved_file_ordinals: snapshot.reserved_file_ordinals,
        #[cfg(any(test, feature = "test-utils"))]
        reporting_receipts,
        library_records: match record_retention {
            LibraryRecordRetention::Drop => Vec::new(),
            LibraryRecordRetention::Collect => library_records,
        },
        user_results,
        #[cfg(any(test, feature = "test-utils"))]
        ordered_user_records: ordered_user_record_evidence,
        #[cfg(any(test, feature = "test-utils"))]
        pass_source_units,
        #[cfg(any(test, feature = "test-utils"))]
        lexical_source_units,
        #[cfg(any(test, feature = "test-utils"))]
        global_types,
        #[cfg(any(test, feature = "test-utils"))]
        module_types,
        #[cfg(any(test, feature = "test-utils"))]
        global_values,
        #[cfg(any(test, feature = "test-utils"))]
        semantic_identities,
    };
    Ok((run, runtime_state, collision_plan))
}

pub fn compile_library_binder_checkpoint(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<LibraryBinderCheckpoint, InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        #[cfg(any(test, feature = "test-utils"))]
        CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.set(
            CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS
                .get()
                .saturating_add(1),
        );
        let CanonicalLibraryFrontend {
            canonical,
            binder,
            module_scopes,
            ..
        } = frontend;
        let library_units = canonical
            .iter()
            .zip(module_scopes)
            .map(|(input, module)| LibraryBinderUnit {
                ordinal: input.file_ordinal,
                source: input.source_key,
                module,
            })
            .collect();
        Ok(build_library_binder_checkpoint(binder, library_units))
    })
}

fn build_library_binder_checkpoint(
    binder: Binder,
    library_units: Vec<LibraryBinderUnit>,
) -> LibraryBinderCheckpoint {
    LibraryBinderCheckpoint::new(binder, library_units)
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinderContinuationModuleSourceForTest {
    pub module: ScopeId,
    pub source: crate::binder::namespace::SourceUnitKey,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct LibraryBinderContinuationForTest {
    pub bound: super::BoundProjectBinder,
    pub checkpoint_ends: LibraryBinderCheckpointEnds,
    pub ends: LibraryBinderCheckpointEnds,
    pub array_symbol_before_augmentation: SymbolId,
    pub array_type_group_before_augmentation: TypeGroupId,
    pub array_symbol_after_augmentation: SymbolId,
    pub array_type_group_after_augmentation: TypeGroupId,
    pub consumer_array_type_group: TypeGroupId,
    pub augmentation_declaration: DeclId,
    pub appended_scopes: Vec<ScopeId>,
    pub appended_symbols: Vec<SymbolId>,
    pub appended_declarations: Vec<DeclId>,
    pub appended_type_groups: Vec<TypeGroupId>,
    pub appended_namespaces: Vec<NamespaceId>,
    pub appended_value_storages: Vec<ValueStorageId>,
    pub appended_module_sources: Vec<BinderContinuationModuleSourceForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectBindingLookupForTest {
    pub symbol: SymbolId,
    pub value: Option<ValueStorageId>,
    pub type_group: Option<TypeGroupId>,
    pub namespace: Option<NamespaceId>,
    pub blocks_type_lookup: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl LibraryBinderContinuationForTest {
    pub fn normalized_per_path_binding_shape_for_test(&self) -> Vec<String> {
        self.bound
            .normalized
            .normalized_per_path_binding_shape
            .clone()
    }

    pub fn project_sources_for_test(&self) -> &[super::ProjectSourceBindingRow] {
        &self.bound.project_sources
    }

    fn project_module_for_test(&self, path: &str) -> Option<ScopeId> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.module)
    }

    pub fn lookup_binding_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ProjectBindingLookupForTest> {
        let module = self.project_module_for_test(path)?;
        let symbol = self.bound.binder.graph.resolve(module, name)?;
        let slots = self.bound.binder.symbols.get(symbol)?;
        Some(ProjectBindingLookupForTest {
            symbol,
            value: slots.value,
            type_group: slots.ty,
            namespace: slots.ns,
            blocks_type_lookup: slots.blocks_type_lookup,
        })
    }

    pub fn import_placeholders_for_test(&self, path: &str) -> Vec<ValueStorageId> {
        let Some(module) = self.project_module_for_test(path) else {
            return Vec::new();
        };
        self.bound
            .module_scopes
            .iter()
            .position(|candidate| *candidate == module)
            .and_then(|index| self.bound.module_placeholders.get(index))
            .into_iter()
            .flatten()
            .filter_map(|placeholder| placeholder.value)
            .collect()
    }

    pub fn script_namespace_root_reservation_for_test(&self, name: &str) -> Option<SymbolId> {
        self.bound
            .binder
            .graph
            .get(self.bound.binder.script_namespace_root)?
            .lookup_local(name)
    }

    pub fn standalone_namespace_value_storage_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ValueStorageId> {
        let namespace = self.lookup_binding_for_test(path, name)?.namespace?;
        self.bound
            .binder
            .namespaces
            .standalone_value_storage(namespace)
    }

    pub fn attached_namespace_value_disposition_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<crate::binder::namespace::NamespaceValueAttachmentDisposition> {
        let module = self.project_module_for_test(path)?;
        self.bound
            .binder
            .namespace_value_attachment(module, name)
            .map(|attachment| attachment.disposition)
    }
}

pub fn continue_library_project_binder(
    checkpoint: LibraryBinderCheckpoint,
    inputs: Vec<crate::frontend::FileInput>,
) -> Result<super::BoundProjectBinder, String> {
    crate::frontend::run_project_frontend(inputs, |_, units| {
        #[cfg(any(test, feature = "test-utils"))]
        record_user_source_parses_for_test(units.len());
        let bound = super::bind_library_checkpoint_project_programs(checkpoint, units)?;
        #[cfg(any(test, feature = "test-utils"))]
        {
            record_user_source_binds_for_test(units.len());
            super::record_continuation_project_binding_consumed_for_test();
        }
        Ok(bound)
    })
    .into_product()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn continuation_receipt_for_test(
    checkpoint_ends: LibraryBinderCheckpointEnds,
    array_symbol_before_augmentation: SymbolId,
    array_type_group_before_augmentation: TypeGroupId,
    bound: super::BoundProjectBinder,
) -> Result<LibraryBinderContinuationForTest, String> {
    let binder = &bound.binder;
    let ends = binder.checkpoint_ends();
    let array_symbol_after_augmentation =
        binder
            .resolve_type(binder.compilation_global, "Array")
            .ok_or_else(|| "continued binder lost Array".to_owned())?;
    let array_type_group_after_augmentation = binder
        .symbols
        .get(array_symbol_after_augmentation)
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "continued Array lost its type group".to_owned())?;
    // Generic project receipts retain the first appended declaration when Array is untouched.
    let augmentation_declaration = binder
        .type_groups
        .get(array_type_group_after_augmentation)
        .and_then(|group| {
            group
                .fragments
                .iter()
                .find(|fragment| fragment.declaration.index() >= checkpoint_ends.declarations)
        })
        .map(|fragment| fragment.declaration)
        .or_else(|| {
            (checkpoint_ends.declarations < ends.declarations).then(|| {
                DeclId(
                    u32::try_from(checkpoint_ends.declarations)
                        .expect("declaration prefix fits u32"),
                )
            })
        })
        .ok_or_else(|| "continued project appended no declaration".to_owned())?;
    let consumer_array_type_group = bound
        .module_scopes
        .last()
        .and_then(|module| binder.resolve_type(*module, "Array"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "consumer cannot resolve continued Array".to_owned())?;
    let appended_module_sources = bound
        .project_sources
        .iter()
        .map(|row| BinderContinuationModuleSourceForTest {
            module: row.module,
            source: row.source,
        })
        .collect();
    Ok(LibraryBinderContinuationForTest {
        bound,
        checkpoint_ends,
        ends,
        array_symbol_before_augmentation,
        array_type_group_before_augmentation,
        array_symbol_after_augmentation,
        array_type_group_after_augmentation,
        consumer_array_type_group,
        augmentation_declaration,
        appended_scopes: (checkpoint_ends.scopes..ends.scopes)
            .map(|id| ScopeId(u32::try_from(id).expect("scope id fits u32")))
            .collect(),
        appended_symbols: (checkpoint_ends.symbols..ends.symbols)
            .map(|id| SymbolId(u32::try_from(id).expect("symbol id fits u32")))
            .collect(),
        appended_declarations: (checkpoint_ends.declarations..ends.declarations)
            .map(|id| DeclId(u32::try_from(id).expect("declaration id fits u32")))
            .collect(),
        appended_type_groups: (checkpoint_ends.type_groups..ends.type_groups)
            .map(|id| TypeGroupId(u32::try_from(id).expect("type-group id fits u32")))
            .collect(),
        appended_namespaces: (checkpoint_ends.namespaces..ends.namespaces)
            .map(|id| NamespaceId(u32::try_from(id).expect("namespace id fits u32")))
            .collect(),
        appended_value_storages: (checkpoint_ends.value_storages..ends.value_storages)
            .map(|id| ValueStorageId(u32::try_from(id).expect("value-storage id fits u32")))
            .collect(),
        appended_module_sources,
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_project_with_owned_library(
    state: OwnedLibraryRuntimeState,
    inputs: Vec<crate::frontend::FileInput>,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserProjectRun, String> {
    let base_ends = state.identity_ends_for_test();
    let initial_visible_names = state.initial_visible_user_names_for_test();
    if !initial_visible_names.is_empty() {
        return Err("fresh project delta exposes prior user names".to_owned());
    }
    let cross_file = std::cell::RefCell::new(None);
    let final_identity = std::cell::RefCell::new(None);
    let state = std::cell::RefCell::new(Some(state));
    super::decls::reset_class_allocation_events_for_test();
    let reports =
        crate::check::test_support::check_project_with_owned_checker_for_test(inputs, |units| {
            let state = state
                .borrow_mut()
                .take()
                .expect("owned project delta is consumed once");
            super::check_project_programs_with_owned_library(
                state,
                units,
                |binder, module_scopes| {
                    let producer = module_scopes
                        .iter()
                        .copied()
                        .find_map(|scope| {
                            let ty = binder
                                .resolve_type(scope, "SharedShape")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .ty?;
                            let value = binder
                                .resolve_value(scope, "sharedValue")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .value?;
                            Some((scope, ty, value))
                        })
                        .expect("project producer exports both test bindings");
                    let consumer = module_scopes
                        .iter()
                        .copied()
                        .filter(|scope| *scope != producer.0)
                        .find_map(|scope| {
                            let ty = binder
                                .resolve_type(scope, "SharedShape")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .ty?;
                            let value = binder
                                .resolve_value(scope, "sharedValue")
                                .and_then(|symbol| binder.symbols.get(symbol))?
                                .value?;
                            Some((ty, value))
                        })
                        .expect("project consumer imports both test bindings");
                    cross_file.replace(Some(OwnedBaseCrossFileWitness {
                        producer_type_group: producer.1,
                        consumer_type_group: consumer.0,
                        producer_value_storage: producer.2,
                        consumer_value_storage: consumer.1,
                    }));
                },
                |pass, final_next_class_id| {
                    final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                        base_store_len: base_ends.store,
                        base_value_storage_len: base_ends.value_storages,
                        base_type_group_len: base_ends.type_groups,
                        base_namespace_len: base_ends.namespaces,
                        binder: pass.binder,
                        published: pass.type_environment.published(),
                        interner: pass.interner,
                        decl_types: &pass.decl_types,
                        next_type_param: pass.next_type_param,
                        next_class_id: final_next_class_id,
                        actual_class_ids: super::decls::class_allocation_events_for_test(),
                        references: final_reference_summary(pass, &base_ends),
                        base_row_clone_counts: expected_base
                            .final_base_family_clone_counts_for_test(pass, final_next_class_id),
                        local_rows_written:
                            OwnedLibraryRuntimeState::final_local_rows_written_for_test(pass),
                    })));
                },
            )
            .expect("the owned-base project route's continuation binds")
        });
    Ok(OwnedBaseUserProjectRun {
        reports,
        final_identity: final_identity
            .into_inner()
            .expect("owned project route captures final identities"),
        cross_file: cross_file
            .into_inner()
            .expect("owned project route captures cross-file identities"),
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_source_with_owned_library(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, false, None)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_caller_certified_collision_free_source_with_base_evidence(
    state: OwnedLibraryRuntimeState,
    source: &str,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(
        state,
        source,
        false,
        Some(expected_base),
    )
}

#[cfg(test)]
fn check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, true, None)
}

#[cfg(any(test, feature = "test-utils"))]
fn check_caller_certified_collision_free_source_with_owned_library_impl(
    state: OwnedLibraryRuntimeState,
    source: &str,
    verify_store_prefix: bool,
    expected_base: Option<&OwnedLibraryRuntimeState>,
) -> Result<OwnedBaseUserRun, String> {
    // WU5 owns routing for suffixes that collide with the frozen global base.
    let parse_started = Instant::now();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return Err(format!(
            "user source parse failed: {:?}",
            parsed.diagnostics
        ));
    }
    record_user_source_parses_for_test(1);
    let parse = parse_started.elapsed();

    let OwnedLibraryRuntimeState {
        mut interner,
        binder,
        published_types,
        mut decl_types,
        semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: _,
        library_modules: _,
        replay_index: _,
        private_collision_epoch: _,
    } = state;
    let base_store_len = interner.store().len();
    let base_declared_recipe_len = interner.store().all_declared_recipes().count();
    let base_store_digest = verify_store_prefix
        .then(|| store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len))
        .transpose()?;
    let base_type_group_count = binder.type_groups.len();
    let base_namespace_count = binder.namespaces.len();
    let base_decl_count = binder.decl_count;
    let base_identity_ends = OwnedBaseFinalIdentityEnds {
        store: base_store_len,
        declared_recipes: base_declared_recipe_len,
        type_params: usize::try_from(next_type_param).expect("base type parameter end fits usize"),
        classes: usize::try_from(next_class_id).expect("base class end fits usize"),
        scopes: binder.graph.len(),
        symbols: binder.symbols.len(),
        declarations: binder.declarations.len(),
        type_groups: base_type_group_count,
        namespaces: base_namespace_count,
        value_storages: usize::try_from(base_decl_count).expect("base storage end fits usize"),
        source_units: binder.checkpoint_ends().next_source,
    };
    let base_max_source_key = binder.max_source_key().0;
    assert_eq!(
        decl_types.len(),
        usize::try_from(base_decl_count).expect("base declaration count fits usize"),
        "owned declaration types cover the complete binder prefix"
    );
    let base_array_group = binder
        .resolve_type(binder.compilation_global, "Array")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty);
    let base_document_value = binder
        .resolve_value(binder.compilation_global, "document")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.value);
    let bind_started = Instant::now();
    let (mut builder, source_key) = ProjectBinderBuilder::resume_frozen_library(binder);
    let unit = CompilationUnit::implementation(source_key, &parsed.program);
    builder.reserve_script_namespace_roots([(&parsed.program, unit)]);
    let (module, _) = builder.add_module(&parsed.program, &[], unit);
    let binder = builder
        .finish_frozen_library_continuation(Some(module))
        .map_err(str::to_owned)?;
    decl_types.resize(binder.decl_count);
    let witness_source_key = source_key.0;
    let array_group_stable = base_array_group
        == binder
            .resolve_type(binder.module, "Array")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty);
    let document_value_stable = base_document_value
        == binder
            .resolve_value(binder.module, "document")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.value);
    let final_type_group_count = binder.type_groups.len();
    let final_decl_count = binder.decl_count;
    record_user_source_binds_for_test(1);
    let bind = bind_started.elapsed();

    let check_started = Instant::now();
    let final_identity = std::cell::RefCell::new(None);
    super::decls::reset_class_allocation_events_for_test();
    let result = check_bound_user_program_with_final_identity_inspector(
        &mut interner,
        binder,
        &parsed.program,
        BoundUserBase {
            published_types,
            library_semantic_identities: semantic_identities,
            lexical_array_alias: None,
            decl_types,
            next_type_param,
            next_class_id,
            runtime,
            private_collision_epoch: None,
            library_modules: std::sync::Arc::from([]),
        },
        |_, _, _, _, _| {},
        |pass, final_next_class_id| {
            final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                base_store_len,
                base_value_storage_len: usize::try_from(base_decl_count)
                    .expect("base storage end fits usize"),
                base_type_group_len: base_type_group_count,
                base_namespace_len: base_namespace_count,
                binder: pass.binder,
                published: pass.type_environment.published(),
                interner: pass.interner,
                decl_types: &pass.decl_types,
                next_type_param: pass.next_type_param,
                next_class_id: final_next_class_id,
                actual_class_ids: super::decls::class_allocation_events_for_test(),
                references: final_reference_summary(pass, &base_identity_ends),
                base_row_clone_counts: expected_base.map_or_else(BTreeMap::new, |expected_base| {
                    expected_base.final_base_family_clone_counts_for_test(pass, final_next_class_id)
                }),
                local_rows_written: OwnedLibraryRuntimeState::final_local_rows_written_for_test(
                    pass,
                ),
            })));
        },
    )?;
    record_user_source_checks_for_test(1);
    let check = check_started.elapsed();
    let final_identity = final_identity
        .into_inner()
        .expect("owned-base route captures final identities after effects");
    let final_store_len = final_identity.ends.store;
    let store_prefix_stable = base_store_digest
        .map(|base_store_digest| {
            store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len)
                .map(|final_store_digest| final_store_digest == base_store_digest)
        })
        .transpose()?;
    Ok(OwnedBaseUserRun {
        result,
        timings: OwnedBaseUserTimings { parse, bind, check },
        final_identity,
        witness: OwnedBaseContinuationWitness {
            base_store_len,
            final_store_len,
            base_type_group_count,
            final_type_group_count,
            base_decl_count,
            final_decl_count,
            source_key: witness_source_key,
            base_max_source_key,
            array_group_stable,
            document_value_stable,
            store_prefix_stable,
        },
    })
}

fn validate_parser_export_claims(
    binder: &Binder,
    parser_claims: Vec<ParserExportClaim>,
    fallback_file_ordinal: LibraryFileOrdinal,
) -> Result<(), InjectedProfileError> {
    let mut binder_claims = Vec::new();
    for context in binder.namespaces.export_contexts() {
        if context.syntax != ExportSyntaxDisposition::FutureTk1319 {
            continue;
        }
        let CompilationOrigin::Library(file_ordinal) = context.origin else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: fallback_file_ordinal,
                messages: vec!["binder produced an unowned TK1319 export context".to_owned()],
            });
        };
        if context.kind != ExportContextKind::ExportDefault {
            return Err(InjectedProfileError::Parse {
                file_ordinal,
                messages: vec!["binder produced a non-default TK1319 export context".to_owned()],
            });
        }
        binder_claims.push(ParserExportClaim {
            file_ordinal,
            span: context.span,
        });
    }
    match_parser_export_claims(parser_claims, binder_claims)
}

fn match_parser_export_claims(
    mut parser_claims: Vec<ParserExportClaim>,
    binder_claims: Vec<ParserExportClaim>,
) -> Result<(), InjectedProfileError> {
    for binder_claim in binder_claims {
        let Some(index) = parser_claims
            .iter()
            .position(|parser_claim| *parser_claim == binder_claim)
        else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: binder_claim.file_ordinal,
                messages: vec![format!(
                    "binder TK1319 claim has no parser owner at {}..{}",
                    binder_claim.span.start, binder_claim.span.end
                )],
            });
        };
        parser_claims.remove(index);
    }
    if let Some(parser_claim) = parser_claims.first() {
        return Err(InjectedProfileError::Parse {
            file_ordinal: parser_claim.file_ordinal,
            messages: vec![format!(
                "parser TS1319 claim has no binder owner at {}..{}",
                parser_claim.span.start, parser_claim.span.end
            )],
        });
    }
    Ok(())
}

fn canonical_inputs<'source>(
    sources: &[InjectedLibrarySource<'source>],
) -> Result<Vec<CanonicalInput<'source>>, InjectedProfileError> {
    if sources.is_empty() {
        return Err(InjectedProfileError::EmptyProfile);
    }
    let mut names = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for source in sources {
        if source.name.is_empty() {
            return Err(InjectedProfileError::EmptyName {
                file_ordinal: source.file_ordinal,
            });
        }
        if !names.insert(source.name) {
            return Err(InjectedProfileError::DuplicateName(source.name.to_owned()));
        }
        if !ordinals.insert(source.file_ordinal) {
            return Err(InjectedProfileError::DuplicateFileOrdinal(
                source.file_ordinal,
            ));
        }
    }
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.file_ordinal);
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| InjectedProfileError::SourceKeyOverflow)?;
            Ok(CanonicalInput {
                file_ordinal: source.file_ordinal,
                source: source.source,
                kind: source_file_kind(source.name),
                source_key,
                origin: CompilationOrigin::Library(source.file_ordinal),
            })
        })
        .collect()
}

#[cfg(any(test, feature = "test-utils"))]
fn collect_type_probes(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    canonical: &[CanonicalInput<'_>],
    module_scopes: &[ScopeId],
) -> (
    BTreeMap<String, TypeProbe>,
    BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
) {
    let mut globals = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for group in binder.type_groups.iter() {
        let probe = type_probe(binder, published, store, group.id);
        let global_symbol = binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&group.name))
            .and_then(|symbol| binder.symbols.get(symbol));
        if global_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
            globals.insert(group.name.clone(), probe.clone());
        }
        for (input, scope) in canonical.iter().zip(module_scopes) {
            let local_symbol = binder
                .graph
                .get(*scope)
                .and_then(|scope| scope.lookup_local(&group.name))
                .and_then(|symbol| binder.symbols.get(symbol));
            if local_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
                modules.insert((input.file_ordinal, group.name.clone()), probe.clone());
            }
        }
    }
    (globals, modules)
}

#[cfg(any(test, feature = "test-utils"))]
fn type_probe(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    identity: TypeGroupId,
) -> TypeProbe {
    let group = binder
        .type_groups
        .get(identity)
        .expect("published type probe has a binder group");
    let declaration_identities = group
        .fragments
        .iter()
        .filter_map(|fragment| {
            library_ordinal(
                binder
                    .namespaces
                    .compilation_origin_for_source(fragment.source)?,
            )
            .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let member_names = published
        .groups()
        .get(identity)
        .and_then(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(ty) => Some(ty),
                PublishedTypeGroupSurface::Class(class) => {
                    match published.classes().published_class(class) {
                        DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                        DemandOutcome::Exhausted(_) => None,
                    }
                }
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.key.to_string())
                .collect()
        })
        .unwrap_or_default();
    TypeProbe {
        identity,
        declaration_identities,
        declaration_count: group.fragments.len(),
        member_names,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn collect_value_probes<Ticket: Copy>(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<Ticket>,
) -> BTreeMap<String, ValueProbe> {
    let mut probes = BTreeMap::new();
    for (symbol_id, symbol) in binder.symbols.iter() {
        if binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&symbol.name))
            != Some(symbol_id)
        {
            continue;
        }
        if let Some(probe) = value_probe_for_symbol(
            binder,
            decl_types,
            store,
            namespace_values,
            binder.compilation_global,
            symbol_id,
        ) {
            probes.insert(symbol.name.clone(), probe);
        }
    }
    probes
}

#[cfg(any(test, feature = "test-utils"))]
fn value_probe_for_symbol<Ticket: Copy>(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<Ticket>,
    owner_scope: ScopeId,
    symbol_id: SymbolId,
) -> Option<ValueProbe> {
    let symbol = binder.symbols.get(symbol_id)?;
    let identity = symbol.value?;
    let participant_identities = symbol
        .declarations
        .iter()
        .filter_map(|declaration| {
            let declaration = binder.declarations.get(*declaration)?;
            origin_for_module(binder, declaration.site.module)
                .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let visible = decl_types.get(identity);
    let call_signature_count = visible
        .map(|ty| signature_ids(store, ty).len())
        .unwrap_or_default();
    let member_names = visible
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.key.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let callable_members = binder
        .namespace_value_attachment(owner_scope, &symbol.name)
        .map(|attachment| {
            attachment
                .members
                .into_iter()
                .filter(|member| member.kind == MergeDeclarationKind::Function)
                .filter_map(|member| {
                    let member_identity = member.value_storage?;
                    let file_ordinal = library_ordinal(member.origin)?;
                    let reservation =
                        namespace_values.namespace_function_reservation(member.declaration)?;
                    let property_ty = visible
                        .and_then(|ty| store.object_type(ty))
                        .and_then(|object| object.property(member.name))
                        .map(|property| property.ty)?;
                    let signature_ids = signature_ids(store, property_ty);
                    let signatures = signature_ids
                        .iter()
                        .filter_map(|signature| signature_probe(store, *signature))
                        .collect::<Vec<_>>();
                    Some(CallableMemberProbe {
                        name: member.name.to_owned(),
                        identity: member_identity,
                        source: library_unit(file_ordinal),
                        reservation_source: reservation.unit,
                        source_start: member.site.declaration_span.start,
                        call_signature_count: signature_ids.len(),
                        signatures,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ValueProbe {
        identity,
        visible_type: visible,
        participant_identities,
        declaration_count: symbol.declarations.len(),
        call_signature_count,
        member_names,
        callable_members,
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn signature_ids(store: &Store, ty: TypeId) -> Vec<TypeId> {
    match store.tag(ty) {
        TypeTag::Function => vec![ty],
        TypeTag::Object => store
            .object_type(ty)
            .map(|object| object.call_signatures.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn signature_probe(store: &Store, ty: TypeId) -> Option<SignatureProbe> {
    let signature = store.function_type(ty)?;
    Some(SignatureProbe {
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| render_type(store, parameter.ty, false))
            .collect(),
        return_type: render_type(store, signature.ret, false),
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn origin_for_module(binder: &Binder, module: ScopeId) -> Option<LibraryFileOrdinal> {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .and_then(|unit| library_ordinal(unit.origin))
}

#[cfg(any(test, feature = "test-utils"))]
fn library_ordinal(origin: CompilationOrigin) -> Option<LibraryFileOrdinal> {
    match origin {
        CompilationOrigin::Library(file_ordinal) => Some(file_ordinal),
        CompilationOrigin::User(_) => None,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn release_phase_line(
    process: usize,
    registry_validation: Duration,
    timings: &LibraryPhaseTimings,
    total: Duration,
) -> String {
    format!(
        "typokat-wu0b-phase-v1 process={process} registry_validation_us={} parse_us={} bind_us={} reserve_fill_us={} publication_validation_us={} statement_check_us={} total_us={}",
        registry_validation.as_micros(),
        timings.parse.as_micros(),
        timings.bind.as_micros(),
        timings.reserve_fill.as_micros(),
        timings.publication_validation.as_micros(),
        timings.statement_check.as_micros(),
        total.as_micros(),
    )
}

#[cfg(any(test, feature = "test-utils"))]
struct ReleaseOutcomeLine {
    process: usize,
    file_count: usize,
    reserved_records: usize,
    filled_records: usize,
    publication_validations: usize,
    library_diagnostics: usize,
    library_incompletes: usize,
    tiny_parse_errors: usize,
    tiny_diagnostics: usize,
    tiny_incompletes: usize,
}

#[cfg(any(test, feature = "test-utils"))]
impl ReleaseOutcomeLine {
    fn render(&self) -> String {
        format!(
            "typokat-wu0b-outcome-v1 process={} file_count={} reserved_records={} filled_records={} publication_validations={} library_diagnostics={} library_incompletes={} tiny_parse_errors={} tiny_diagnostics={} tiny_incompletes={}",
            self.process,
            self.file_count,
            self.reserved_records,
            self.filled_records,
            self.publication_validations,
            self.library_diagnostics,
            self.library_incompletes,
            self.tiny_parse_errors,
            self.tiny_diagnostics,
            self.tiny_incompletes,
        )
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_interner_has_no_pending_reservations(
    injected: &[InjectedLibrarySource<'_>],
) {
    let (_, state) =
        compile_owned_injected_profile(injected).expect("source-compiled full-library profile");
    state
        .interner
        .strict_terminal_state_for_test()
        .expect("full profile must close every reserved type");
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_replay_index_is_complete_and_deterministic(
    injected: &[InjectedLibrarySource<'_>],
) {
    let started = Instant::now();
    let (_, first_state) =
        compile_owned_injected_profile(injected).expect("first exact replay index generation");
    let first_elapsed = started.elapsed();
    let started = Instant::now();
    let (_, second_state) =
        compile_owned_injected_profile(injected).expect("second exact replay index generation");
    let second_elapsed = started.elapsed();
    let first = first_state
        .replay_index()
        .expect("source compiler retains its replay index");
    let second = second_state
        .replay_index()
        .expect("source compiler retains its replay index");
    assert_eq!(
        first.canonical_manifest_len(),
        second.canonical_manifest_len()
    );
    assert_eq!(
        first.canonical_manifest_sha256,
        second.canonical_manifest_sha256
    );
    assert_eq!(first.unowned_demand_count, 0);
    assert_eq!(first.invalid_owner_site_count, 0);
    assert_eq!(first.noncanonical_edge_count, 0);
    assert_eq!(first.typed_reference_coverage_misses, 0);
    let digest = first
        .canonical_manifest_sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    eprintln!(
        "replay-index owners={} roots={} sites={} edges={} root_edges={} sccs={} statements={} baselines={} bytes={} sha256={} first={:?} second={:?}",
        first.owner_partition.len(),
        first.root_slots.len(),
        first.owner_sites.len(),
        first.reverse_edges.len(),
        first.root_slot_consumers.len(),
        first.scc_membership.len(),
        first.statement_owners.len(),
        first.baseline_records.len(),
        first.canonical_manifest_len(),
        digest,
        first_elapsed,
        second_elapsed,
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_profile_selects_complete_native_bridge_identities(
    injected: &[InjectedLibrarySource<'_>],
) {
    let run = run_injected_profile(injected).expect("source-compiled full-library profile");
    let identities = run.semantic_identities();
    assert!(identities.all_ready());
    assert_eq!(
        identities.callable_function_group(),
        run.global_type_probe("CallableFunction")
            .map(|probe| probe.identity)
    );
    assert_ne!(
        identities.callable_function_group(),
        run.global_type_probe("Function")
            .map(|probe| probe.identity)
    );
    let expected_arities = [1, 1, 0, 0, 0, 0, 0, 0];
    for (terminal, expected_arity) in identities.terminals().into_iter().zip(expected_arities) {
        let super::library_identities::LibraryIdentityTerminal::Ready(identity) = terminal else {
            panic!("exact profile native bridge identity must be ready")
        };
        assert_eq!(identity.parameters.len(), expected_arity);
        assert_ne!(identity.template, TypeId(0));
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn run_library_release_probe(
    injected: &[InjectedLibrarySource<'_>],
    registry_validation: Duration,
    total_started: Instant,
) {
    const TINY_SOURCE: &str = "export const typokatLibraryProbe: number = 1;\n";

    let process = std::env::var("TYPOKAT_WU0B_PROCESS")
        .expect("TYPOKAT_WU0B_PROCESS must identify release process 1..5")
        .parse::<usize>()
        .expect("TYPOKAT_WU0B_PROCESS must be an integer in 1..5");
    assert!(
        (1..=5).contains(&process),
        "TYPOKAT_WU0B_PROCESS must be in 1..5"
    );

    let run = run_injected_profile(injected).expect("exact library profile execution");
    assert_eq!(run.phase_counts.parse_units, 82);
    assert_eq!(run.phase_counts.bind_units, 82);
    assert_eq!(run.phase_counts.statement_check_units, 82);
    assert!(run.phase_counts.reserved_records > 0);
    assert_eq!(
        run.phase_counts.reserved_records,
        run.phase_counts.filled_records
    );
    assert!(run.phase_counts.publication_validations > 0);
    let expected = (0..82)
        .map(LibraryFileOrdinal::new)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        run.reserved_file_ordinals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert!(run
        .library_records
        .iter()
        .all(|(key, _)| expected.contains(&key.file_ordinal)));
    assert!(run
        .reporting_receipts
        .iter()
        .all(|receipt| expected.contains(&receipt.file_ordinal)));

    let tiny = crate::check::test_support::check_source(TINY_SOURCE);
    assert!(tiny.parse_errors.is_empty(), "{:?}", tiny.parse_errors);
    assert!(tiny.diagnostics.is_empty(), "{:?}", tiny.diagnostics);
    assert!(tiny.incomplete.is_empty(), "{:?}", tiny.incomplete);
    let library_diagnostics = run
        .library_records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    let library_incompletes = run.library_records.len() - library_diagnostics;
    let total = total_started.elapsed();
    assert!(
        registry_validation + run.phase_timings.measured_total() <= total,
        "external total must cover every measured phase"
    );

    println!(
        "{}",
        release_phase_line(process, registry_validation, &run.phase_timings, total)
    );
    println!(
        "{}",
        ReleaseOutcomeLine {
            process,
            file_count: injected.len(),
            reserved_records: run.phase_counts.reserved_records,
            filled_records: run.phase_counts.filled_records,
            publication_validations: run.phase_counts.publication_validations,
            library_diagnostics,
            library_incompletes,
            tiny_parse_errors: tiny.parse_errors.len(),
            tiny_diagnostics: tiny.diagnostics.len(),
            tiny_incompletes: tiny.incomplete.len(),
        }
        .render()
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub fn assert_exact_full_profile_owned_base_checks_caller_certified_suffix(
    injected: &[InjectedLibrarySource<'_>],
) {
    let (compiled, state) =
        compile_owned_injected_profile(injected).expect("exact source-compiled owned library");
    eprintln!(
        "owned-base compile timings: parse={:?} bind={:?} reserve_fill={:?} publication={:?} statements={:?} total={:?}",
        compiled.phase_timings.parse,
        compiled.phase_timings.bind,
        compiled.phase_timings.reserve_fill,
        compiled.phase_timings.publication_validation,
        compiled.phase_timings.statement_check,
        compiled.phase_timings.measured_total(),
    );
    let source = concat!(
        include_str!("../../../../../tooling/full-lib-bench/workloads/fast-clean/main.ts"),
        "\nconst directDomProbe: HTMLDivElement = document.createElement(\"div\");\n",
    );
    let run = check_caller_certified_collision_free_source_with_owned_library(state, source)
        .expect("exact owned base accepts the WU0A suffix");
    eprintln!(
        "owned-base user timings: parse={:?} bind={:?} check={:?}",
        run.timings.parse, run.timings.bind, run.timings.check
    );

    let (_, state) = compile_owned_injected_profile(injected)
        .expect("second exact source-compiled owned library");
    let focused = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
            const mutableBad: string[] = [1].map(value => value);
            const readonlyValues: readonly number[] = [1, 2];
            const readonlyBad: string[] = readonlyValues.map(value => value);
            readonlyValues.push(3);
            const upperBad: number = "x".toUpperCase();
            const fixedBad: number = (1).toFixed();
            const booleanBad: string = true.valueOf();
            const calledBad: number = ((value: string) => value).call(undefined, "x");
            const objectBad: number = ({ value: 1 }).toString();
            const regexpBad: string = /x/.test("x");
            const domBad: number = document.createElement("div");
        "#,
    )
    .expect("exact owned base accepts the focused semantic suffix");
    eprintln!(
        "owned-base focused timings: parse={:?} bind={:?} check={:?}",
        focused.timings.parse, focused.timings.bind, focused.timings.check
    );
    assert_eq!(
        focused
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2339,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
            crate::diagnostics::DiagnosticCode::TK2322,
        ]
    );
    assert!(focused.result.incomplete.is_empty());
    assert_eq!(focused.witness.store_prefix_stable, None);
    assert_eq!(run.witness.store_prefix_stable, None);
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    assert!(
        run.result.diagnostics.is_empty(),
        "{:?}",
        run.result.diagnostics
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::type_groups::PublishedTypeGroup;

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    #[test]
    fn frozen_base_receipt_tracks_the_successful_central_freeze() {
        let (_, mut state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "freeze-receipt.d.ts",
            source: "interface FreezeReceipt { value: string }",
        }])
        .expect("freeze receipt fixture compiles");
        let scope = CompleteSourceRouteWorkScopeForTest::start();

        state
            .freeze_as_library_base()
            .expect("runtime state freezes through the central operation");

        let work = scope.finish();
        assert_eq!(work.frozen_base_seals, 1);
        assert_eq!(work.semantic_publications, 0);
        assert_eq!(work.replay_trace_constructions, 0);
        assert_eq!(work.replay_plan_constructions, 0);
    }

    #[test]
    fn semantic_publication_receipt_tracks_the_actual_publish_entry() {
        let scope = CompleteSourceRouteWorkScopeForTest::start();

        compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "publication-receipt.d.ts",
            source: "interface PublicationReceipt { value: string }",
        }])
        .expect("publication receipt fixture compiles");

        let work = scope.finish();
        assert_eq!(work.semantic_publications, 1);
        assert_eq!(work.replay_plan_constructions, 0);
        assert_eq!(work.frozen_base_seals, 0);
    }

    #[test]
    fn replay_receipts_track_the_central_trace_and_plan_entries() {
        let scope = CompleteSourceRouteWorkScopeForTest::start();

        ReplayDependencyTrace::default()
            .finish_compact_plan(
                Vec::new(),
                Vec::new(),
                [0; 9],
                CollisionReplayConstructionEvidence::default(),
            )
            .expect("empty replay plan is structurally valid");

        let work = scope.finish();
        assert_eq!(work.replay_trace_constructions, 1);
        assert_eq!(work.replay_plan_constructions, 1);
        assert_eq!(work.semantic_publications, 0);
        assert_eq!(work.frozen_base_seals, 0);
    }

    #[test]
    fn complete_source_user_unit_validation_rejects_duplicate_module_ordinals() {
        let duplicate = ModuleOrdinal::new(4);
        let error = validate_complete_source_user_units(
            3,
            Some(1),
            &[(duplicate, UnitSlot::new(0)), (duplicate, UnitSlot::new(1))],
        )
        .expect_err("duplicate user module ordinal must fail closed");

        assert_eq!(
            error,
            InjectedProfileError::CanonicalProjection(
                "complete-source user module ordinal 4 is duplicated".to_owned()
            )
        );
    }

    #[test]
    fn complete_source_result_assembly_rejects_unexpected_module_records() {
        let unexpected = ModuleOrdinal::new(9);
        let error = assemble_complete_source_user_results(
            &[(ModuleOrdinal::new(2), UnitSlot::new(0))],
            BTreeMap::from([(unexpected, Vec::new())]),
        )
        .expect_err("unexpected user module records must fail closed");

        assert_eq!(
            error,
            InjectedProfileError::CanonicalProjection(
                "complete-source retained records for unexpected user module ordinal 9".to_owned()
            )
        );
    }

    #[test]
    fn canonical_property_key_bytes_pin_symbol_discriminants() {
        let cases = [
            (WellKnownSymbol::Iterator, 0),
            (WellKnownSymbol::ToStringTag, 1),
            (WellKnownSymbol::AsyncIterator, 2),
            (WellKnownSymbol::Species, 3),
            (WellKnownSymbol::ToPrimitive, 4),
            (WellKnownSymbol::Replace, 5),
            (WellKnownSymbol::Unscopables, 6),
            (WellKnownSymbol::Split, 7),
            (WellKnownSymbol::Search, 8),
            (WellKnownSymbol::Match, 9),
            (WellKnownSymbol::MatchAll, 10),
            (WellKnownSymbol::HasInstance, 11),
        ];

        for (symbol, discriminant) in cases {
            let mut bytes = CanonicalBytes::domain(b"");
            bytes
                .property_key(&PropertyKey::WellKnownSymbol(symbol))
                .expect("well-known symbol key encodes");
            assert_eq!(bytes.finish(), vec![1, discriminant], "{symbol}");
        }

        let mut string = CanonicalBytes::domain(b"");
        string
            .property_key(&PropertyKey::String("iterator".to_owned()))
            .expect("string property key encodes");
        assert_eq!(
            string.finish(),
            vec![0, 0, 0, 0, 0, 0, 0, 0, 8, b'i', b't', b'e', b'r', b'a', b't', b'o', b'r']
        );
    }

    fn compile_symbol_product_parts() -> OwnedLibraryRuntimeProductParts {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "certified-symbol.d.ts",
            source: r#"
                declare const Other: {};
                declare const Symbol: {};
                interface CertifiedSymbolSurface {
                    [Symbol.iterator](): string;
                }
            "#,
        }])
        .expect("certified Symbol library compiles");
        state.into_product_parts().expect("extract Symbol product")
    }

    #[test]
    fn authenticated_symbol_root_is_installed_before_interface_fill() {
        let records = compile_owned_injected_records(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "early-certified-symbol.d.ts",
            source: r#"
                declare const Symbol: {};
                interface CertifiedSymbolSurface {
                    [Symbol.iterator](): string;
                    [Symbol.asyncIterator](): number;
                }
            "#,
        }])
        .expect("certified Symbol library compiles");
        assert!(records.iter().all(|(_, record)| {
            !matches!(
                record,
                CheckerRecord::Incomplete(incomplete)
                    if incomplete.id == "signature/method-signature/computed-key"
            )
        }));
    }

    #[test]
    fn symbol_certificate_round_trips_and_rejects_corruption() {
        let parts = compile_symbol_product_parts();
        let certified = parts
            .runtime
            .certified_library_values
            .symbol
            .expect("Symbol root is certified");
        let restored = OwnedLibraryRuntimeState::from_product_parts(parts)
            .expect("certified Symbol product restores");
        assert_eq!(
            restored.runtime.certified_library_values.symbol,
            Some(certified)
        );

        let mut mismatched = compile_symbol_product_parts();
        let other = collect_root_rows(&mismatched.binder)
            .expect("product binder roots project")
            .into_iter()
            .find(|row| row.name == "Other")
            .and_then(|row| row.value)
            .expect("other in-range value root");
        mismatched.runtime.certified_library_values.symbol = Some(other);
        assert_product_restore_error(
            mismatched,
            "product certified Symbol value does not match the authenticated root",
        );

        let mut out_of_range = compile_symbol_product_parts();
        out_of_range.runtime.certified_library_values.symbol =
            Some(ValueStorageId(out_of_range.binder.decl_count));
        assert_product_restore_error(
            out_of_range,
            "product certified Symbol value is out of range",
        );
    }

    #[test]
    fn symbol_certificate_survives_dense_and_sparse_forks() {
        let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "forked-certified-symbol.d.ts",
            source: "declare const Symbol: {};",
        }])
        .expect("certified Symbol library compiles");
        let certified = base
            .runtime
            .certified_library_values
            .symbol
            .expect("Symbol root is certified");
        base.freeze_as_library_base().expect("Symbol base freezes");
        let dense = base.fork_user_delta_for_test().expect("dense fork");
        let sparse = base.fork_sparse_collision_epoch().expect("sparse fork");
        assert_eq!(
            dense.runtime.certified_library_values.symbol,
            Some(certified)
        );
        assert_eq!(
            sparse.runtime.certified_library_values.symbol,
            Some(certified)
        );
    }

    const PRODUCT_SEMANTIC_LIBRARY: &str = r#"
        interface Array<T> { item: T; }
        interface ReadonlyArray<T> { item: T; }
        interface String { stringMarker: string; }
        interface Number { numberMarker: number; }
        interface Boolean { booleanMarker: boolean; }
        interface RegExp { regexpMarker: boolean; }
        interface Object { objectMarker: string; }
        interface Function { functionMarker: string; }
        interface CallableFunction extends Function { callableMarker: number; }
        interface SeamWitness { value: number; }
        declare class SeamBase { base: string; }
        declare class SeamCtorBase { protected constructor(); }
        declare class SeamInheritedCtor extends SeamCtorBase {}
        declare class SeamClass<T> extends SeamBase {
            constructor(value: T);
            value: T;
        }
        declare namespace SeamClass { export const tag: string; }
        declare namespace SeamSpace { export const enabled: boolean; }
        declare function seamNamed(value: number): string;
        declare namespace seamNamed { export const version: number; }
    "#;

    fn compile_semantic_product_parts() -> OwnedLibraryRuntimeProductParts {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "product-seam.d.ts",
            source: PRODUCT_SEMANTIC_LIBRARY,
        }])
        .expect("focused owned library profile");
        state.into_product_parts().expect("extract product parts")
    }

    fn compile_reservation_fixture(file_name: &str, source: &str) -> OwnedLibraryRuntimeState {
        compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: file_name,
            source,
        }])
        .expect("reservation lifecycle fixture compiles")
        .1
    }

    #[test]
    fn exact_library_roots_install_only_the_six_trusted_marker_shapes() {
        let state = compile_reservation_fixture(
            "trusted-markers.d.ts",
            r#"
                type ThisParameterType<T> = T extends
                    (this: infer Receiver, ...args: never) => any ? Receiver : unknown;
                type OmitThisParameter<T> = unknown extends ThisParameterType<T>
                    ? T
                    : T extends (...args: infer A) => infer R
                        ? (...args: A) => R
                        : T;
                type Uppercase<S extends string> = intrinsic;
                type Lowercase<S extends string> = intrinsic;
                type Capitalize<S extends string> = intrinsic;
                type Uncapitalize<S extends string> = intrinsic;
                interface ThisType<T> {}
            "#,
        );
        let well_known = state.interner.well_known();
        for (name, marker) in [
            ("Uppercase", well_known.uppercase),
            ("Lowercase", well_known.lowercase),
            ("Capitalize", well_known.capitalize),
            ("Uncapitalize", well_known.uncapitalize),
            ("ThisType", well_known.this_type),
            ("OmitThisParameter", well_known.omit_this_parameter),
        ] {
            assert_eq!(published_template_type(&state, name), marker, "{name}");
        }
    }

    #[test]
    fn early_omit_this_parameter_demand_does_not_freeze_its_library_root_twice() {
        let state = compile_reservation_fixture(
            "early-omit-demand.d.ts",
            r#"
                interface EarlyDemand<
                    T extends OmitThisParameter<
                        (this: { prefix: string }, value: string) => number
                    >
                > {}
                type ThisParameterType<T> = T extends
                    (this: infer Receiver, ...args: never) => any ? Receiver : unknown;
                type OmitThisParameter<T> = unknown extends ThisParameterType<T>
                    ? T
                    : T extends (...args: infer A) => infer R
                        ? (...args: A) => R
                        : T;
            "#,
        );
        assert_eq!(
            published_template_type(&state, "OmitThisParameter"),
            state.interner.well_known().omit_this_parameter
        );
    }

    #[test]
    fn same_named_wrong_shapes_and_unlisted_intrinsics_are_not_trusted_markers() {
        let state = compile_reservation_fixture(
            "untrusted-markers.d.ts",
            r#"
                type Uppercase<S extends string> = S;
                type Lowercase<S extends string> = S;
                type Capitalize<S extends string> = S;
                type Uncapitalize<S extends string> = S;
                type OmitThisParameter<T> = T;
                interface ThisType<T> { marker: T; }
                type NoInfer<T> = intrinsic;
                type IntrinsicAlias<T> = intrinsic;
            "#,
        );
        let well_known = state.interner.well_known();
        for (name, marker) in [
            ("Uppercase", well_known.uppercase),
            ("Lowercase", well_known.lowercase),
            ("Capitalize", well_known.capitalize),
            ("Uncapitalize", well_known.uncapitalize),
            ("ThisType", well_known.this_type),
            ("OmitThisParameter", well_known.omit_this_parameter),
        ] {
            assert_ne!(published_template_type(&state, name), marker, "{name}");
        }
        assert_type_group_is_error_or_unavailable(&state, "NoInfer");
        assert_type_group_is_error_or_unavailable(&state, "IntrinsicAlias");
    }

    #[test]
    fn trusted_string_marker_rejects_each_near_shape_dimension() {
        for source in [
            "type Uppercase<S> = intrinsic;",
            "type Uppercase<S extends number> = intrinsic;",
            "type Uppercase<S extends string = string> = intrinsic;",
            "type Uppercase<S extends string, T> = intrinsic;",
            "type Uppercase<S extends string> = S;",
        ] {
            let state = compile_reservation_fixture("string-marker-near-shape.d.ts", source);
            assert_ne!(
                published_template_type(&state, "Uppercase"),
                state.interner.well_known().uppercase,
                "{source}"
            );
        }
    }

    #[test]
    fn trusted_this_type_marker_rejects_each_near_shape_dimension() {
        for source in [
            "interface ThisType {}",
            "interface ThisType<T, U> {}",
            "interface ThisType<T extends string> {}",
            "interface ThisType<T = unknown> {}",
            "interface ThisType<T> { value: T; }",
        ] {
            let state = compile_reservation_fixture("this-type-near-shape.d.ts", source);
            assert_ne!(
                published_template_type(&state, "ThisType"),
                state.interner.well_known().this_type,
                "{source}"
            );
        }
    }

    #[test]
    fn trusted_omit_this_parameter_rejects_parameter_near_shapes() {
        let body = "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T";
        for source in [
            format!("type OmitThisParameter = {body};"),
            format!("type OmitThisParameter<T, U> = {body};"),
            format!("type OmitThisParameter<T extends unknown> = {body};"),
            format!("type OmitThisParameter<T = unknown> = {body};"),
        ] {
            let state = compile_reservation_fixture("omit-parameter-near-shape.d.ts", &source);
            assert_ne!(
                published_template_type(&state, "OmitThisParameter"),
                state.interner.well_known().omit_this_parameter,
                "{source}"
            );
        }
    }

    #[test]
    fn trusted_omit_this_parameter_rejects_conditional_near_shapes() {
        for body in [
            "any extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends T ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? unknown : T extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : unknown extends (...args: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (value: infer A) => infer R ? (...args: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (value: A) => R : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => unknown : T",
            "unknown extends ThisParameterType<T> ? T : T extends (...args: infer A) => infer R ? (...args: A) => R : unknown",
        ] {
            let source = format!("type OmitThisParameter<T> = {body};");
            let state = compile_reservation_fixture("omit-body-near-shape.d.ts", &source);
            assert_ne!(
                published_template_type(&state, "OmitThisParameter"),
                state.interner.well_known().omit_this_parameter,
                "{source}"
            );
        }
    }

    fn published_template_type(state: &OwnedLibraryRuntimeState, name: &str) -> TypeId {
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == name)
            .unwrap_or_else(|| panic!("missing type group {name}"));
        match state.published_types.groups().get(group.id) {
            Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                surface: PublishedTypeGroupSurface::Template(ty),
                ..
            })) => *ty,
            terminal => panic!("{name} did not publish a template: {terminal:?}"),
        }
    }

    fn published_object_property_type(
        state: &OwnedLibraryRuntimeState,
        owner: &str,
        property: &str,
    ) -> TypeId {
        let owner = published_template_type(state, owner);
        state
            .interner
            .store()
            .object_type(owner)
            .unwrap_or_else(|| panic!("{owner:?} is not an object template"))
            .property(property)
            .unwrap_or_else(|| panic!("{owner:?} has no {property} property"))
            .ty
    }

    fn store_type_reaches(
        state: &OwnedLibraryRuntimeState,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        const TYPE_DOMAIN: u8 = 1;
        let (references, _) = state.interner.reference_records_for_test();
        let mut edges = rustc_hash::FxHashMap::<u32, Vec<u32>>::default();
        for (owner_domain, target_domain, _, owner, referenced) in references {
            if owner_domain == TYPE_DOMAIN && target_domain == TYPE_DOMAIN {
                edges.entry(owner).or_default().push(referenced);
            }
        }
        let mut visited = rustc_hash::FxHashSet::default();
        let mut pending = edges.get(&source.0).cloned().unwrap_or_default();
        while let Some(current) = pending.pop() {
            if current == target.0 {
                return true;
            }
            if visited.insert(current) {
                pending.extend(edges.get(&current).into_iter().flatten().copied());
            }
        }
        false
    }

    fn assert_strict_interner_state(state: &OwnedLibraryRuntimeState, label: &str) {
        state
            .interner
            .strict_terminal_state_for_test()
            .unwrap_or_else(|error| panic!("{label}: interner is not in terminal state: {error}"));
    }

    fn assert_type_group_is_error_or_unavailable(state: &OwnedLibraryRuntimeState, name: &str) {
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == name)
            .expect("fixture type group");
        match state.published_types.groups().get(group.id) {
            Some(PublishedTypeGroupTerminal::Unavailable(_)) => {}
            Some(PublishedTypeGroupTerminal::Ready(group)) => match group.surface {
                PublishedTypeGroupSurface::Template(ty)
                    if ty == state.interner.well_known().error => {}
                PublishedTypeGroupSurface::Template(ty) => panic!(
                    "{name} accidentally published TypeId {} ({:?}) instead of error/unavailable",
                    ty.0,
                    state.interner.store().tag(ty)
                ),
                PublishedTypeGroupSurface::Class(class) => {
                    panic!("{name} accidentally published class {class:?}")
                }
            },
            None => panic!("{name} has no published terminal"),
        }
    }

    fn strict_interner_state_failure(
        label: &str,
        source: &'static str,
        root: &str,
    ) -> Option<String> {
        let state = compile_reservation_fixture("reservation-lifecycle.d.ts", source);
        assert_type_group_is_error_or_unavailable(&state, root);
        state
            .interner
            .strict_terminal_state_for_test()
            .err()
            .map(|error| format!("{label}: {error}"))
    }

    fn assert_valid_class_interface_merge(label: &str, source: &'static str) {
        let state = compile_reservation_fixture("class-interface-merge.d.ts", source);
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "MergedClassInterface")
            .expect("merged class/interface type group");
        let probe = type_probe(
            &state.binder,
            &state.published_types,
            state.interner.store(),
            group.id,
        );
        assert_eq!(probe.declaration_count, 2, "{label}: {probe:?}");
        assert_eq!(
            probe.member_names,
            ["classMember", "interfaceMember"],
            "{label}: {probe:?}"
        );
        assert!(
            matches!(
                state.published_types.groups().get(group.id),
                Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    surface: PublishedTypeGroupSurface::Class(_),
                    ..
                }))
            ),
            "{label}: merged type group must publish a class surface"
        );
        state
            .interner
            .strict_terminal_state_for_test()
            .unwrap_or_else(|error| panic!("{label}: interner is not in terminal state: {error}"));
    }

    fn replace_module_sources(
        parts: &mut OwnedLibraryRuntimeProductParts,
        module_sources: rustc_hash::FxHashMap<ScopeId, crate::binder::namespace::SourceUnitKey>,
    ) {
        let placeholder = Binder::from_product_parts(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            ScopeId(0),
            ScopeId(0),
            ScopeId(0),
            ScopeId(0),
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let binder = std::mem::replace(&mut parts.binder, placeholder);
        parts.binder = Binder::from_product_parts(
            binder.graph,
            binder.symbols,
            binder.declarations,
            binder.type_groups,
            binder.namespaces,
            binder.module,
            binder.prelude_module,
            binder.compilation_global,
            binder.script_namespace_root,
            binder.decl_count,
            binder.prelude_type_group_count,
            module_sources,
            binder.fn_scopes,
            binder.fn_decl_ids,
            binder.block_scopes,
        );
    }

    fn assert_product_restore_error(
        parts: OwnedLibraryRuntimeProductParts,
        expected: &'static str,
    ) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            OwnedLibraryRuntimeState::from_product_parts(parts)
        }));
        let Ok(result) = outcome else {
            panic!("product-parts corruption must return an error, not panic")
        };
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("product-parts corruption unexpectedly restored"),
        }
    }

    #[test]
    fn injected_results_are_ast_free_owned_terminals() {
        assert_owned_terminal::<InjectedProfileRun>();
        assert_owned_terminal::<InjectedProfileError>();
        assert_owned_terminal::<LibraryPhaseTimings>();
        assert_owned_terminal::<OwnedLibraryRuntimeState>();
    }

    #[test]
    fn failed_conditional_alias_recovery_leaves_no_pending_reservation() {
        let source = r#"
            type FailedAwaited<T> =
                T extends typeof unsupportedValue ? FailedAwaited<T> : T;
        "#;
        let failure = strict_interner_state_failure("FailedAwaited", source, "FailedAwaited");
        assert!(failure.is_none(), "{failure:#?}");
    }

    #[test]
    fn failed_mapped_alias_recovery_leaves_no_pending_reservation() {
        let source = r#"
            type FailedMapped<T> = { [K in FailedMapped<T>]: T };
        "#;
        let failure = strict_interner_state_failure("FailedMapped", source, "FailedMapped");
        assert!(failure.is_none(), "{failure:#?}");
    }

    #[test]
    fn conflicting_conditional_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type ConditionalCollision<T> = T extends string ? string : number;
            interface ConditionalCollision<T> { interfaceMember: T; }
        "#;
        let interface_first = r#"
            interface ConditionalCollision<T> { interfaceMember: T; }
            type ConditionalCollision<T> = T extends string ? string : number;
        "#;
        let failures = [
            strict_interner_state_failure(
                "conditional-alias-first",
                alias_first,
                "ConditionalCollision",
            ),
            strict_interner_state_failure(
                "conditional-interface-first",
                interface_first,
                "ConditionalCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn conflicting_mapped_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type MappedCollision<T> = { [K in keyof T]: T[K] };
            interface MappedCollision<T> { interfaceMember: T; }
        "#;
        let interface_first = r#"
            interface MappedCollision<T> { interfaceMember: T; }
            type MappedCollision<T> = { [K in keyof T]: T[K] };
        "#;
        let failures = [
            strict_interner_state_failure("mapped-alias-first", alias_first, "MappedCollision"),
            strict_interner_state_failure(
                "mapped-interface-first",
                interface_first,
                "MappedCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn conflicting_object_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type ObjectCollision = { aliasMember: string };
            interface ObjectCollision { interfaceMember: number; }
        "#;
        let interface_first = r#"
            interface ObjectCollision { interfaceMember: number; }
            type ObjectCollision = { aliasMember: string };
        "#;
        let failures = [
            strict_interner_state_failure("object-alias-first", alias_first, "ObjectCollision"),
            strict_interner_state_failure(
                "object-interface-first",
                interface_first,
                "ObjectCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn valid_class_interface_merge_closes_reservations_in_both_orders() {
        let class_first = r#"
            declare class MergedClassInterface { classMember: string; }
            interface MergedClassInterface { interfaceMember: number; }
        "#;
        let interface_first = r#"
            interface MergedClassInterface { interfaceMember: number; }
            declare class MergedClassInterface { classMember: string; }
        "#;

        assert_valid_class_interface_merge("class-first control", class_first);
        assert_valid_class_interface_merge("interface-first", interface_first);
    }

    #[test]
    fn acyclic_object_aliases_publish_one_structural_identity() {
        let state = compile_reservation_fixture(
            "acyclic-object-aliases.d.ts",
            r#"
                type FirstShape = {
                    value: number;
                    run(input: string): boolean;
                };
                type SecondShape = {
                    run(input: string): boolean;
                    value: number;
                };
            "#,
        );
        let first = published_template_type(&state, "FirstShape");
        let second = published_template_type(&state, "SecondShape");

        assert_eq!(first, second, "equal acyclic aliases must hash-cons");
        assert!(!store_type_reaches(&state, first, first));
        assert_strict_interner_state(&state, "equal acyclic aliases");
    }

    #[test]
    fn acyclic_object_alias_reuses_an_existing_anonymous_shape() {
        let state = compile_reservation_fixture(
            "anonymous-object-collision.d.ts",
            r#"
                type Carrier = {
                    nested: {
                        value: number;
                        run(input: string): boolean;
                    };
                };
                type NamedShape = {
                    value: number;
                    run(input: string): boolean;
                };
            "#,
        );
        let anonymous = published_object_property_type(&state, "Carrier", "nested");
        let named = published_template_type(&state, "NamedShape");

        assert_eq!(named, anonymous, "named alias must reuse the anonymous row");
        assert_strict_interner_state(&state, "anonymous shape collision");
    }

    #[test]
    fn recursive_object_aliases_retain_their_stable_reserved_roots() {
        let state = compile_reservation_fixture(
            "recursive-object-aliases.d.ts",
            r#"
                type SelfNode = { next: SelfNode | null };
                type MutualLeft = { right: MutualRight | null };
                type MutualRight = { left: MutualLeft | null };
                type NestedNode = {
                    next: (() => readonly NestedNode[]) | null;
                };
            "#,
        );
        let self_node = published_template_type(&state, "SelfNode");
        let mutual_left = published_template_type(&state, "MutualLeft");
        let mutual_right = published_template_type(&state, "MutualRight");
        let nested = published_template_type(&state, "NestedNode");

        assert!(store_type_reaches(&state, self_node, self_node));
        assert_ne!(mutual_left, mutual_right);
        assert!(store_type_reaches(&state, mutual_left, mutual_right));
        assert!(store_type_reaches(&state, mutual_right, mutual_left));
        assert!(store_type_reaches(&state, nested, nested));
        assert_strict_interner_state(&state, "recursive aliases");
    }

    #[test]
    fn captured_object_alias_roots_are_never_remapped_behind_existing_edges() {
        let state = compile_reservation_fixture(
            "captured-object-aliases.d.ts",
            r#"
                type ForwardConsumer = { value: ForwardLater };
                type ForwardLater = { marker: number };

                interface InterfaceConsumer { value: InterfaceLater; }
                type InterfaceLater = { marker: string };

                type BackwardEarlier = { marker: boolean };
                type BackwardConsumer = { value: BackwardEarlier };
            "#,
        );

        for (consumer, dependency) in [
            ("ForwardConsumer", "ForwardLater"),
            ("InterfaceConsumer", "InterfaceLater"),
            ("BackwardConsumer", "BackwardEarlier"),
        ] {
            assert_eq!(
                published_object_property_type(&state, consumer, "value"),
                published_template_type(&state, dependency),
                "{consumer} must retain the published identity of {dependency}"
            );
        }
        assert_strict_interner_state(&state, "captured alias roots");
    }

    #[test]
    fn type_parameter_defaults_follow_final_canonical_alias_identity() {
        let state = compile_reservation_fixture(
            "type-parameter-default-alias-capture.d.ts",
            r#"
                type FirstShape = { marker: number };
                type Box<T = NamedShape> = T;
                type NamedShape = { marker: number };
            "#,
        );
        let first = published_template_type(&state, "FirstShape");
        let named = published_template_type(&state, "NamedShape");
        let box_group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "Box")
            .expect("Box type group");
        let Some(PublishedTypeGroupTerminal::Ready(box_surface)) =
            state.published_types.groups().get(box_group.id)
        else {
            panic!("Box did not publish a ready type group");
        };

        assert_eq!(first, named, "equal acyclic aliases must hash-cons");
        assert_strict_interner_state(&state, "type parameter default alias capture");
        assert_eq!(
            box_surface.parameter_defaults,
            [PublishedTypeParameterDefault::Ready(named)],
            "published defaults must follow the final canonical alias identity"
        );
    }

    #[test]
    fn replay_index_covers_class_references_in_interface_parameter_defaults() {
        let state = compile_reservation_fixture(
            "replay-interface-class-default.d.ts",
            r#"
                declare class DefaultClass {}
                interface Box<T = DefaultClass> { value: T; }
            "#,
        );
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        assert_eq!(replay.unowned_demand_count, 0);
        assert_eq!(replay.invalid_owner_site_count, 0);
        assert_eq!(replay.noncanonical_edge_count, 0);
        assert_eq!(replay.typed_reference_coverage_misses, 0);
    }

    #[test]
    fn replay_index_covers_class_references_copied_across_overload_rows() {
        let state = compile_reservation_fixture(
            "replay-overload-class-provenance.d.ts",
            r#"
                declare class DefaultClass {}
                declare function consume(value: DefaultClass): void;
                declare function consume(value: string): void;
            "#,
        );
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        assert_eq!(replay.unowned_demand_count, 0);
        assert_eq!(replay.invalid_owner_site_count, 0);
        assert_eq!(replay.noncanonical_edge_count, 0);
        assert_eq!(replay.typed_reference_coverage_misses, 0);
    }

    #[test]
    fn terminal_class_validator_follows_declared_recipe_template_dependencies() {
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const TYPE_DOMAIN: u8 = 1;
        const CLASS_DOMAIN: u8 = 3;

        let state = compile_reservation_fixture(
            "replay-declared-recipe-class-template.d.ts",
            r#"
                declare class RecipeDependency {}
                interface RecipeTemplate<T> {
                    item: T;
                    dependency: RecipeDependency;
                }
                interface RecipeConsumer {
                    nested: RecipeTemplate<string>[];
                }
            "#,
        );
        let consumer_group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "RecipeConsumer")
            .expect("consumer type group")
            .id;
        let consumer_template = published_template_type(&state, "RecipeConsumer");
        let (store_references, _) = state.interner.reference_records_for_test();
        let mut pending = vec![(TYPE_DOMAIN, consumer_template.0, false)];
        let mut visited = BTreeSet::new();
        let mut reaches_class_through_recipe = false;
        while let Some((domain, owner, crossed_recipe)) = pending.pop() {
            if !visited.insert((domain, owner, crossed_recipe)) {
                continue;
            }
            for &(owner_domain, target_domain, _, row, target) in &store_references {
                if owner_domain != domain || row != owner {
                    continue;
                }
                if target_domain == CLASS_DOMAIN && crossed_recipe {
                    reaches_class_through_recipe = true;
                    break;
                }
                if matches!(target_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
                    pending.push((
                        target_domain,
                        target,
                        crossed_recipe || target_domain == DECLARED_RECIPE_DOMAIN,
                    ));
                }
            }
            if reaches_class_through_recipe {
                break;
            }
        }
        assert!(
            reaches_class_through_recipe,
            "fixture must expose Type -> Recipe -> Type/Class dependency reachability"
        );

        let trace = ReplayDependencyTrace::default();
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");
        for edge in &replay.reverse_edges {
            if edge.consumer == ReplayOwner::TypeGroup(consumer_group) {
                continue;
            }
            trace.enter(edge.consumer);
            trace.demand(edge.dependency);
            trace.leave(edge.consumer);
        }
        let runtime = state
            .runtime
            .snapshot_parts()
            .expect("runtime snapshot parts");
        validate_terminal_class_dependencies(
            Some(&trace),
            &state.interner,
            ReplayTerminalValidationInputs {
                binder: &state.binder,
                published: &state.published_types,
                decl_types: &state.decl_types,
                namespace_terminals: &runtime.namespace_terminals,
                runtime: &runtime,
                semantic_identities: state.semantic_identities.as_ref(),
            },
        )
        .expect("terminal validation enumerates canonical references");
        let error = trace
            .finish(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0)
            .expect_err("omitting the consumer dependency must fail typed coverage");
        assert!(
            matches!(
                error,
                ReplayIndexGenerationError::TypedReferenceCoverage { .. }
            ),
            "declared recipe template dependency was invisible to terminal validation: {error:?}"
        );
    }

    #[test]
    fn terminal_class_validator_preserves_cycles_duplicates_and_multiple_classes() {
        const TYPE_DOMAIN: u8 = 1;
        const DECLARED_RECIPE_DOMAIN: u8 = 10;

        let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let classes = [ClassId(7), ClassId(11)];
        let semantic_edges = BTreeMap::from([
            ((TYPE_DOMAIN, 0), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
            (
                (DECLARED_RECIPE_DOMAIN, 10),
                vec![(TYPE_DOMAIN, 11), (TYPE_DOMAIN, 11)],
            ),
            ((TYPE_DOMAIN, 11), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
        ]);
        let class_edges = BTreeMap::from([
            ((DECLARED_RECIPE_DOMAIN, 10), vec![classes[0], classes[0]]),
            ((TYPE_DOMAIN, 11), vec![classes[1]]),
        ]);
        let direct = BTreeMap::from([(owner, vec![TypeId(0)])]);

        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        for class in classes {
            trace.demand(ReplayOwner::Class(class));
        }
        trace.leave(owner);
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            None,
        );

        let partition = vec![
            owner,
            ReplayOwner::Class(classes[0]),
            ReplayOwner::Class(classes[1]),
        ];
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, baselines, 0)
            .expect("cycles and duplicate edges retain both terminal classes");
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[0]),
                    consumer: owner,
                },
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[1]),
                    consumer: owner,
                },
            ])
        );
    }

    #[test]
    fn terminal_class_validator_processes_a_shared_semantic_tail_once() {
        const TYPE_DOMAIN: u8 = 1;
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const OWNER_COUNT: usize = 64;
        const SHARED_DEPTH: usize = 128;

        let owners = (0..OWNER_COUNT)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        let classes = [ClassId(7), ClassId(11), ClassId(13)];
        let shared_tail = (0..SHARED_DEPTH)
            .map(|index| {
                let domain = if index % 2 == 0 {
                    DECLARED_RECIPE_DOMAIN
                } else {
                    TYPE_DOMAIN
                };
                (
                    domain,
                    10_000 + u32::try_from(index).expect("semantic id fits u32"),
                )
            })
            .collect::<Vec<_>>();

        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
        for (index, owner) in owners.iter().copied().enumerate() {
            let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
            direct.insert(owner, vec![root]);
            semantic_edges
                .entry((TYPE_DOMAIN, root.0))
                .or_default()
                .push(shared_tail[0]);
        }
        for pair in shared_tail.windows(2) {
            semantic_edges
                .entry(pair[0])
                .or_default()
                .extend([pair[1], pair[1]]);
        }
        semantic_edges
            .entry(*shared_tail.last().expect("non-empty shared tail"))
            .or_default()
            .push(shared_tail[SHARED_DEPTH / 2]);

        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
        class_edges
            .entry(shared_tail[SHARED_DEPTH / 4])
            .or_default()
            .extend([classes[0], classes[0]]);
        class_edges
            .entry(shared_tail[SHARED_DEPTH / 2])
            .or_default()
            .push(classes[1]);
        class_edges
            .entry(*shared_tail.last().expect("non-empty shared tail"))
            .or_default()
            .push(classes[2]);

        let trace = ReplayDependencyTrace::default();
        for owner in &owners {
            trace.enter(*owner);
            for class in classes {
                trace.demand(ReplayOwner::Class(class));
            }
            trace.leave(*owner);
        }
        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let mut partition = owners.clone();
        partition.extend(classes.into_iter().map(ReplayOwner::Class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, baselines, 0)
            .expect("cycles and duplicate semantic edges preserve the exact class oracle");
        let expected_edges = owners
            .iter()
            .flat_map(|owner| {
                classes.iter().map(move |class| ReplayReverseEdge {
                    dependency: ReplayOwner::Class(*class),
                    consumer: *owner,
                })
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges,
            "shared-tail summarization must not change the owner-to-class oracle"
        );

        let unique_semantic_nodes = OWNER_COUNT + SHARED_DEPTH;
        assert_eq!(
            work.owner_root_summaries,
            u64::try_from(OWNER_COUNT).expect("owner count fits u64")
        );
        let linear_input_size = unique_semantic_nodes
            + semantic_edges.values().map(Vec::len).sum::<usize>()
            + class_edges.values().map(Vec::len).sum::<usize>()
            + OWNER_COUNT;
        let measured_work = work
            .owner_root_summaries
            .saturating_add(work.semantic_node_visits)
            .saturating_add(work.semantic_edge_probes)
            .saturating_add(work.class_edge_probes);
        assert!(
            measured_work
                <= 4 * u64::try_from(linear_input_size).expect("validation input fits u64"),
            "terminal dependency validation must stay O(V + E + owners), not owners * depth: \
             input={linear_input_size}, work={work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_deduplicates_repeated_class_across_shared_tail() {
        const TYPE_DOMAIN: u8 = 1;
        const OWNER_COUNT: usize = 64;
        const SHARED_DEPTH: usize = 128;

        let owners = (0..OWNER_COUNT)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        let class = ClassId(7);
        let shared_tail = (0..SHARED_DEPTH)
            .map(|index| {
                (
                    TYPE_DOMAIN,
                    10_000 + u32::try_from(index).expect("semantic id fits u32"),
                )
            })
            .collect::<Vec<_>>();
        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
        for (index, owner) in owners.iter().copied().enumerate() {
            let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
            direct.insert(owner, vec![root]);
            semantic_edges.insert((TYPE_DOMAIN, root.0), vec![shared_tail[0]]);
        }
        for pair in shared_tail.windows(2) {
            semantic_edges.insert(pair[0], vec![pair[1]]);
        }
        let class_edges = shared_tail
            .iter()
            .copied()
            .map(|node| (node, vec![class]))
            .collect::<BTreeMap<_, _>>();

        let trace = ReplayDependencyTrace::default();
        for owner in &owners {
            trace.enter(*owner);
            trace.demand(ReplayOwner::Class(class));
            trace.leave(*owner);
        }
        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let mut partition = owners.clone();
        partition.push(ReplayOwner::Class(class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, baselines, 0)
            .expect("repeated class references retain their exact terminal-class oracle");
        let expected_edges = owners
            .iter()
            .map(|owner| ReplayReverseEdge {
                dependency: ReplayOwner::Class(class),
                consumer: *owner,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges
        );

        let semantic_node_count = OWNER_COUNT + SHARED_DEPTH;
        let semantic_edge_count = OWNER_COUNT + SHARED_DEPTH - 1;
        let class_edge_count = SHARED_DEPTH;
        let output_edge_count = OWNER_COUNT;
        let linear_input_and_output = semantic_node_count
            + semantic_edge_count
            + class_edge_count
            + output_edge_count
            + OWNER_COUNT;
        assert!(
            work.class_summary_items
                <= 6 * u64::try_from(linear_input_and_output)
                    .expect("validation input and output fit u64"),
            "repeated terminal classes must be deduplicated before owner expansion: \
             input+output={linear_input_and_output}, work={work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_summary_work_is_output_sensitive_for_unique_chain() {
        const TYPE_DOMAIN: u8 = 1;
        const ROOTED_DEPTH: usize = 1_024;
        const DISCONNECTED_DEPTH: usize = 2_048;

        let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let rooted_classes = (0..ROOTED_DEPTH)
            .map(|index| ClassId(10_000 + u32::try_from(index).expect("class id fits u32")))
            .collect::<Vec<_>>();
        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();

        for (index, class) in rooted_classes.iter().copied().enumerate() {
            let node = (
                TYPE_DOMAIN,
                u32::try_from(index).expect("semantic id fits u32"),
            );
            class_edges.insert(node, vec![class]);
            if index + 1 < ROOTED_DEPTH {
                semantic_edges.insert(
                    node,
                    vec![(
                        TYPE_DOMAIN,
                        u32::try_from(index + 1).expect("semantic id fits u32"),
                    )],
                );
            }
        }

        for index in 0..DISCONNECTED_DEPTH {
            let id = 100_000 + u32::try_from(index).expect("semantic id fits u32");
            let node = (TYPE_DOMAIN, id);
            class_edges.insert(
                node,
                vec![ClassId(
                    200_000 + u32::try_from(index).expect("class id fits u32"),
                )],
            );
            if index + 1 < DISCONNECTED_DEPTH {
                semantic_edges.insert(node, vec![(TYPE_DOMAIN, id + 1)]);
            }
        }

        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        for class in &rooted_classes {
            trace.demand(ReplayOwner::Class(*class));
        }
        trace.leave(owner);

        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            BTreeMap::from([(owner, vec![TypeId(0)])]),
            Some(&mut work),
        );

        let mut partition = vec![owner];
        partition.extend(rooted_classes.iter().copied().map(ReplayOwner::Class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, baselines, 0)
            .expect("the rooted chain retains its exact terminal-class oracle");
        let expected_edges = rooted_classes
            .iter()
            .map(|class| ReplayReverseEdge {
                dependency: ReplayOwner::Class(*class),
                consumer: owner,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges,
            "the output oracle must contain exactly the rooted chain's classes"
        );

        let semantic_node_count = ROOTED_DEPTH + DISCONNECTED_DEPTH;
        let semantic_edge_count = (ROOTED_DEPTH - 1) + (DISCONNECTED_DEPTH - 1);
        let class_edge_count = semantic_node_count;
        let output_edge_count = ROOTED_DEPTH;
        let linear_input_and_output =
            semantic_node_count + semantic_edge_count + class_edge_count + output_edge_count + 1;
        assert!(
            work.class_summary_items
                <= 6 * u64::try_from(linear_input_and_output)
                    .expect("validation input and output fit u64"),
            "terminal-class summaries must be root-relevant and output-sensitive, not copy every \
             suffix's growing class set: input+output={linear_input_and_output}, work={work:?}"
        );
    }

    /// Shared *owner-expression* spine: a chain of `L` components each merging the
    /// same cross owner, so every spine component mints a fresh two-input
    /// `TerminalOwnerExpression::Union` whose owner set stays tiny (`{seed, cross}`).
    /// `M` sibling components hang off the tail, each with its own owner and its own
    /// terminal class, so each becomes a *separate* query root that is not an
    /// ancestor of any other. No spine union is ever a query root, so a
    /// summarizer that memoises only at roots re-walks the whole spine once per
    /// sibling — Θ(M · L) work for Θ(M) output.
    fn shared_union_spine_probe(
        spine_len: usize,
        siblings: usize,
        demand_first_sibling: bool,
    ) -> (u64, TerminalClassDependencyValidationWork) {
        const TYPE_DOMAIN: u8 = 1;
        let spine = |index: usize| {
            (
                TYPE_DOMAIN,
                1_000_000 + u32::try_from(index).expect("spine id fits u32"),
            )
        };
        let cross = (TYPE_DOMAIN, 2_000_000_u32);
        let sibling = |index: usize| {
            (
                TYPE_DOMAIN,
                3_000_000 + u32::try_from(index).expect("sibling id fits u32"),
            )
        };
        let sibling_root = |index: usize| {
            (
                TYPE_DOMAIN,
                4_000_000 + u32::try_from(index).expect("sibling root id fits u32"),
            )
        };

        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();

        let seed_owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let cross_owner = ReplayOwner::TypeGroup(TypeGroupId(1));
        for index in 0..spine_len - 1 {
            semantic_edges
                .entry(spine(index))
                .or_default()
                .push(spine(index + 1));
        }
        for index in 0..spine_len {
            semantic_edges.entry(cross).or_default().push(spine(index));
        }
        direct.insert(seed_owner, vec![TypeId(spine(0).1)]);
        direct.insert(cross_owner, vec![TypeId(cross.1)]);

        let classes = (0..siblings)
            .map(|index| ClassId(100 + u32::try_from(index).expect("class id fits u32")))
            .collect::<Vec<_>>();
        let sibling_owners = (0..siblings)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    2 + u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        for index in 0..siblings {
            semantic_edges
                .entry(spine(spine_len - 1))
                .or_default()
                .push(sibling(index));
            semantic_edges
                .entry(sibling_root(index))
                .or_default()
                .push(sibling(index));
            direct.insert(sibling_owners[index], vec![TypeId(sibling_root(index).1)]);
            class_edges
                .entry(sibling(index))
                .or_default()
                .push(classes[index]);
        }

        let trace = ReplayDependencyTrace::default();
        if demand_first_sibling {
            // Only the first sibling owner's edge is authenticated; every other
            // emitted pair stays a coverage miss, which the caller counts.
            trace.enter(sibling_owners[0]);
            trace.demand(ReplayOwner::Class(classes[0]));
            trace.leave(sibling_owners[0]);
        }

        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let error = trace
            .finish(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0)
            .expect_err("the probe authenticates at most one emitted pair");
        let ReplayIndexGenerationError::TypedReferenceCoverage { count, .. } = error else {
            panic!("terminal closure must report its unauthenticated pairs: {error:?}");
        };
        (count, work)
    }

    /// Input + output size of the shared-union-spine shape: semantic nodes and
    /// edges, class edges, owner roots, and the emitted owner × class pairs.
    fn shared_union_spine_input_and_output(spine_len: usize, siblings: usize) -> usize {
        let semantic_nodes = spine_len + 1 + 2 * siblings;
        let semantic_edges = (spine_len - 1) + spine_len + 2 * siblings;
        let class_edges = siblings;
        let owner_roots = 2 + siblings;
        let output_pairs = 3 * siblings;
        semantic_nodes + semantic_edges + class_edges + owner_roots + output_pairs
    }

    #[test]
    fn terminal_class_validator_shared_union_spine_stays_complete() {
        const BASE: usize = 256;

        // Completeness: each class must still summarize to exactly its three
        // owners {seed, cross, own}, and an authenticated pair must be seen.
        let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
        let (authenticated_misses, _) = shared_union_spine_probe(BASE, BASE, true);
        assert_eq!(
            base_misses,
            u64::try_from(3 * BASE).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );
        assert_eq!(
            authenticated_misses,
            base_misses - 1,
            "authenticating one owner-class edge must retire exactly one pair"
        );

        // The graph-build / SCC / Kahn counters stay linear on this shape — none
        // of them observes the owner-expression traversal, which the companion
        // `..._summary_is_output_sensitive` spec pins separately.
        let input_and_output = shared_union_spine_input_and_output(BASE, BASE);
        let counted = base_work
            .owner_root_summaries
            .saturating_add(base_work.semantic_node_visits)
            .saturating_add(base_work.semantic_edge_probes)
            .saturating_add(base_work.class_edge_probes)
            .saturating_add(base_work.class_summary_items);
        assert!(
            counted <= 6 * u64::try_from(input_and_output).expect("input fits u64"),
            "the committed counters are blind to the owner-expression traversal: \
             input+output={input_and_output}, work={base_work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_shared_union_spine_summary_is_output_sensitive() {
        const BASE: usize = 256;
        const SCALED: usize = 1_024;

        let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
        let (scaled_misses, scaled_work) = shared_union_spine_probe(SCALED, SCALED, false);
        assert_eq!(
            base_misses,
            u64::try_from(3 * BASE).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );
        assert_eq!(
            scaled_misses,
            u64::try_from(3 * SCALED).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );

        // Two sizes, so a super-linear summary is visible as a ratio and not
        // just as a budget overrun on one arbitrary shape.
        let base_budget = 6 * u64::try_from(shared_union_spine_input_and_output(BASE, BASE))
            .expect("validation input and output fit u64");
        let scaled_budget = 6 * u64::try_from(shared_union_spine_input_and_output(SCALED, SCALED))
            .expect("validation input and output fit u64");
        let base_visits = base_work.owner_expression_visits;
        let scaled_visits = scaled_work.owner_expression_visits;
        assert!(
            base_visits <= base_budget && scaled_visits <= scaled_budget,
            "owner-expression summarization must stay O(inputs + owners), not owners * spine \
             length: {BASE}x{BASE} visited {base_visits} (budget {base_budget}), \
             {SCALED}x{SCALED} visited {scaled_visits} (budget {scaled_budget})"
        );
    }

    #[test]
    fn declared_recipe_domain_uses_independent_base_and_local_ends() {
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const TYPE_DOMAIN: u8 = 1;

        let state = compile_reservation_fixture(
            "declared-recipe-domain-ends.d.ts",
            "interface RecipeOwner { values: string[]; }",
        );
        let base = owned_state_identity_ends(&state);
        let recipe_end = state.interner.store().all_declared_recipes().count();
        assert!(recipe_end > 0, "fixture must plan declaration recipes");

        let mut summary = OwnedBaseReferenceSummary::default();
        classify_interner_live_reference(
            &mut summary,
            &base,
            DECLARED_RECIPE_DOMAIN,
            0,
            TYPE_DOMAIN,
            u32::try_from(base.store).expect("store end fits u32"),
        );
        classify_interner_live_reference(
            &mut summary,
            &base,
            DECLARED_RECIPE_DOMAIN,
            u32::try_from(recipe_end).expect("recipe end fits u32"),
            TYPE_DOMAIN,
            0,
        );

        assert_eq!(
            summary,
            OwnedBaseReferenceSummary {
                base_to_delta: 1,
                delta_to_base: 1,
                delta_to_delta: 0,
            },
            "recipe rows must be classified against their own frozen prefix"
        );
    }

    #[test]
    fn binder_source_unit_domain_cannot_alias_the_declared_recipe_domain() {
        const SOURCE_UNIT_DOMAIN: u8 = 10;
        const SCOPE_DOMAIN: u8 = 4;

        let state = compile_reservation_fixture(
            "source-unit-domain-origin.d.ts",
            "interface RecipeOwner { values: string[]; }",
        );
        let mut base = owned_state_identity_ends(&state);
        assert!(base.source_units > 0);
        assert!(base.scopes > 0);
        base.declared_recipes = base.source_units.saturating_add(100);

        let first_delta_source =
            u32::try_from(base.source_units).expect("source-unit end fits u32");
        let last_base_source = first_delta_source
            .checked_sub(1)
            .expect("fixture retains a base source unit");
        let first_delta_scope = u32::try_from(base.scopes).expect("scope end fits u32");

        let mut summary = OwnedBaseReferenceSummary::default();
        classify_binder_live_reference(
            &mut summary,
            &base,
            SOURCE_UNIT_DOMAIN,
            first_delta_source,
            SCOPE_DOMAIN,
            first_delta_scope,
        );
        classify_binder_live_reference(
            &mut summary,
            &base,
            SOURCE_UNIT_DOMAIN,
            last_base_source,
            SCOPE_DOMAIN,
            first_delta_scope,
        );
        classify_binder_live_reference(
            &mut summary,
            &base,
            SOURCE_UNIT_DOMAIN,
            first_delta_source,
            SCOPE_DOMAIN,
            0,
        );

        assert_eq!(
            summary,
            OwnedBaseReferenceSummary {
                base_to_delta: 1,
                delta_to_base: 1,
                delta_to_delta: 1,
            },
            "binder source units use their source prefix even when recipe ids overlap"
        );
    }

    #[test]
    fn store_prefix_digest_authenticates_unreferenced_planned_recipes() {
        let mut interner = Interner::with_intrinsics();
        let prefix_len = interner.store().len();
        let recipe_prefix_len = interner.store().all_declared_recipes().count();
        let before = store_prefix_digest(interner.store(), prefix_len, recipe_prefix_len)
            .expect("initial prefix digest");
        let string = interner.well_known().string;
        interner.intern_declared_recipe(crate::types::repr::DeclaredRecipeNode::Type(string));
        let planned_recipe_prefix_len = interner.store().all_declared_recipes().count();
        let after = store_prefix_digest(interner.store(), prefix_len, planned_recipe_prefix_len)
            .expect("prefix digest after recipe planning");

        assert_ne!(
            after, before,
            "an unreferenced recipe row is still authenticated prefix state"
        );
    }

    #[test]
    fn replay_index_records_empty_and_constructor_only_parent_edges() {
        let state = compile_reservation_fixture(
            "replay-class-parent-edges.d.ts",
            r#"
                declare class EmptyBase {}
                declare class EmptyChild extends EmptyBase {}
                declare class ConstructorBase { constructor(value: string); }
                declare class ConstructorChild extends ConstructorBase {
                    constructor(value: string);
                }
            "#,
        );
        let class = |name: &str| {
            let group = state
                .binder
                .type_groups
                .iter()
                .find(|group| group.name == name)
                .unwrap_or_else(|| panic!("missing class group {name}"));
            match state.published_types.groups().get(group.id) {
                Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    surface: PublishedTypeGroupSurface::Class(class),
                    ..
                })) => *class,
                terminal => panic!("{name} did not publish a class: {terminal:?}"),
            }
        };
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        for (parent, child) in [
            (class("EmptyBase"), class("EmptyChild")),
            (class("ConstructorBase"), class("ConstructorChild")),
        ] {
            assert!(replay.reverse_edges.contains(&ReplayReverseEdge {
                dependency: ReplayOwner::Class(parent),
                consumer: ReplayOwner::Class(child),
            }));
        }
    }

    fn root_candidate(
        slots: impl IntoIterator<Item = SourceBindingSlot>,
    ) -> crate::binder::declaration::SourceGlobalBindingCandidate {
        crate::binder::declaration::SourceGlobalBindingCandidate {
            slots: slots.into_iter().collect(),
            global_object_contributor: false,
        }
    }

    #[test]
    fn replay_root_census_rejects_a_missing_binder_root() {
        let candidates = BTreeMap::from([(
            "PresentOnlyInSource".to_owned(),
            root_candidate([SourceBindingSlot::Value]),
        )]);
        let error = validate_root_census(&candidates, &[], false).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("PresentOnlyInSource".to_owned())
        );
    }

    #[test]
    fn replay_root_census_rejects_an_inexact_slot_set() {
        let candidates = BTreeMap::from([(
            "Merged".to_owned(),
            root_candidate([SourceBindingSlot::Value, SourceBindingSlot::Type]),
        )]);
        let rows = [RootNameRow {
            name: "Merged".to_owned(),
            value: Some(ValueStorageId(0)),
            ty: None,
            namespace: None,
        }];
        let error = validate_root_census(&candidates, &rows, false).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("Merged".to_owned())
        );
    }

    #[test]
    fn replay_root_census_requires_explicit_global_this_to_publish() {
        let error = validate_root_census(&BTreeMap::new(), &[], true).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("globalThis".to_owned())
        );
    }

    #[test]
    fn instantiated_namespace_expectation_does_not_depend_on_storage_or_root_row() {
        let mut candidate = root_candidate([SourceBindingSlot::Namespace]);
        apply_merge_root_semantics(
            &mut candidate,
            BTreeSet::from([SourceBindingSlot::Namespace]),
            false,
            true,
        );
        assert_eq!(
            candidate.slots,
            BTreeSet::from([SourceBindingSlot::Value, SourceBindingSlot::Namespace])
        );
        assert!(candidate.global_object_contributor);

        let candidates = BTreeMap::from([("RuntimeNamespace".to_owned(), candidate)]);
        let rows = [RootNameRow {
            name: "RuntimeNamespace".to_owned(),
            value: None,
            ty: None,
            namespace: Some(NamespaceId(0)),
        }];
        assert_eq!(
            validate_root_census(&candidates, &rows, false).unwrap_err(),
            ReplayIndexGenerationError::InvalidRootSlot("RuntimeNamespace".to_owned())
        );
    }

    #[test]
    fn replay_root_normalization_covers_pure_and_mixed_namespace_semantics() {
        let source = r#"
            declare namespace PureType { interface Member {} }
            declare namespace PureRuntime { const value: number; }
            interface InterfaceMerge {}
            declare namespace InterfaceMerge { interface Member {} }
            declare class ClassMerge {}
            declare namespace ClassMerge { interface Member {} }
            declare function FunctionMerge(): void;
            declare namespace FunctionMerge { interface Member {} }
        "#;
        let state = compile_reservation_fixture("root-normalization.d.ts", source);
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");
        let root = |name: &str| {
            replay
                .root_slots
                .iter()
                .find(|root| root.name == name)
                .unwrap_or_else(|| panic!("missing replay root {name}"))
        };
        let slots = |name: &str| {
            let root = root(name);
            (
                root.value.is_some(),
                root.ty.is_some(),
                root.namespace.is_some(),
                root.global_object_contributor,
            )
        };
        assert_eq!(slots("PureType"), (false, false, true, false));
        assert_eq!(slots("PureRuntime"), (true, false, true, true));
        assert_eq!(slots("InterfaceMerge"), (false, true, true, false));
        assert_eq!(slots("ClassMerge"), (true, true, true, false));
        assert_eq!(slots("FunctionMerge"), (true, false, true, true));

        let binding_span = |name: &str| {
            let start = u32::try_from(source.find(name).expect("fixture binding")).unwrap();
            Span::new(start, start + u32::try_from(name.len()).unwrap())
        };
        let has_global_site = |span: Span| {
            replay
                .owner_sites
                .iter()
                .any(|site| site.owner == ReplayOwner::GlobalObject && site.span == span)
        };
        assert!(!has_global_site(binding_span("PureType")));
        assert!(has_global_site(binding_span("PureRuntime")));
    }

    #[test]
    fn object_alias_identity_ignores_member_source_order() {
        let state = compile_reservation_fixture(
            "object-alias-member-order.d.ts",
            r#"
                type OrderedFirst = { alpha: string; beta: number };
                type OrderedSecond = { beta: number; alpha: string };
            "#,
        );

        assert_eq!(
            published_template_type(&state, "OrderedFirst"),
            published_template_type(&state, "OrderedSecond"),
            "member source order is not structural identity"
        );
        assert_strict_interner_state(&state, "object alias member order");
    }

    #[test]
    fn object_alias_identity_keeps_identity_bearing_metadata() {
        let state = compile_reservation_fixture(
            "object-alias-metadata.d.ts",
            r#"
                type OptionalValue = { value?: number };
                type RequiredValue = { value: number };
                type ReadonlyValue = { readonly value: number };
                type MutableValue = { value: number };
                type StringIndexed = { [key: string]: number };
                type NumberIndexed = { [key: number]: number };
                type Callable = { (input: string): boolean };
                type Constructable = { new (input: string): { value: boolean } };
            "#,
        );

        for (left, right) in [
            ("OptionalValue", "RequiredValue"),
            ("ReadonlyValue", "MutableValue"),
            ("StringIndexed", "NumberIndexed"),
            ("Callable", "Constructable"),
        ] {
            assert_ne!(
                published_template_type(&state, left),
                published_template_type(&state, right),
                "{left} and {right} differ in identity-bearing metadata"
            );
        }
        assert_strict_interner_state(&state, "object alias identity metadata");
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "large object-alias scaling is a release-only gate"
    )]
    fn acyclic_object_alias_canonicalization_scales_linearly() {
        fn compile_scale(count: usize) -> (Duration, usize, usize) {
            let mut source = String::new();
            for index in 0..count {
                use std::fmt::Write;
                writeln!(
                    source,
                    "type Scale{index:05} = {{ value: number; run(input: string): boolean }};"
                )
                .expect("write scale fixture");
            }
            let (run, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(0),
                name: "object-alias-scale.d.ts",
                source: &source,
            }])
            .expect("object-alias scale fixture compiles");
            let identities = state
                .binder
                .type_groups
                .iter()
                .filter(|group| group.name.starts_with("Scale"))
                .map(|group| match state.published_types.groups().get(group.id) {
                    Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                        surface: PublishedTypeGroupSurface::Template(ty),
                        ..
                    })) => *ty,
                    terminal => panic!("scale alias did not publish: {terminal:?}"),
                })
                .collect::<rustc_hash::FxHashSet<_>>();
            assert_strict_interner_state(&state, "object alias scale");
            eprintln!(
                "typokat-object-alias-scale-v1 aliases={count} store_rows={} reserve_fill_us={} unique_published={}",
                state.interner.store().len(),
                run.phase_timings.reserve_fill.as_micros(),
                identities.len(),
            );
            (
                run.phase_timings.reserve_fill,
                state.interner.store().len(),
                identities.len(),
            )
        }

        let (small_time, small_rows, small_unique) = compile_scale(1_000);
        let (large_time, large_rows, large_unique) = compile_scale(10_000);
        assert_eq!((small_unique, large_unique), (1, 1));
        assert!(large_rows < small_rows.saturating_mul(12));
        assert!(
            large_time <= small_time.saturating_mul(20),
            "10x aliases must not approach quadratic reserve/fill time: {small_time:?} -> {large_time:?}"
        );
    }

    #[test]
    fn focused_profile_selects_complete_native_bridge_identities() {
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "native-bridges.d.ts",
            source: r#"
                interface Array<T> { mapResult: T; }
                interface ReadonlyArray<T> { mapResult: T; }
                interface String { toUpperCaseResult: string; }
                interface Number { toFixedResult: string; }
                interface Boolean { valueOfResult: boolean; }
                interface RegExp { testResult: boolean; }
                interface Object { toStringResult: string; }
                interface Function { legacyCallResult: string; }
                interface CallableFunction extends Function { callResult: number; }
            "#,
        }])
        .expect("focused source-compiled library profile");
        let identities = run.semantic_identities();
        assert!(identities.all_ready());
        assert_eq!(
            identities.callable_function_group(),
            run.global_type_probe("CallableFunction")
                .map(|probe| probe.identity)
        );
        assert_ne!(
            identities.callable_function_group(),
            run.global_type_probe("Function")
                .map(|probe| probe.identity)
        );
        let expected_arities = [1, 1, 0, 0, 0, 0, 0, 0];
        for (terminal, expected_arity) in identities.terminals().into_iter().zip(expected_arities) {
            let super::super::library_identities::LibraryIdentityTerminal::Ready(identity) =
                terminal
            else {
                panic!("focused native bridge identity must be ready")
            };
            assert_eq!(identity.parameters.len(), expected_arity);
        }
    }

    #[test]
    fn owned_runtime_product_parts_restore_a_consumable_base() {
        let parts = compile_semantic_product_parts();
        assert_eq!(parts.source_file_count, 1);
        assert!(parts.semantic_identities.is_some());
        assert!(!parts.runtime.class_application_parameters.is_empty());
        assert!(!parts.runtime.class_new_metadata.is_empty());
        assert!(!parts.runtime.class_value_bindings.is_empty());
        assert!(!parts.runtime.namespace_terminals.is_empty());
        assert!(!parts.runtime.named_function_symbols.is_empty());
        let restored =
            OwnedLibraryRuntimeState::from_product_parts(parts).expect("restore product parts");
        let user = check_caller_certified_collision_free_source_with_owned_library(
            restored,
            r#"
                declare const witness: SeamWitness;
                const witnessValue: number = witness.value;
                declare const nativeValues: number[];
                const nativeArrayItem: number = nativeValues.item;
                const nativeStringMarker: string = "value".stringMarker;
                const instance = new SeamClass<number>(1);
                const classValue: number = instance.value;
                const inheritedValue: string = instance.base;
                const base = new SeamBase();
                const directBaseValue: string = base.base;
                const classTag: string = SeamClass.tag;
                const enabled: boolean = SeamSpace.enabled;
                const called: string = seamNamed(1);
                const functionVersion: number = seamNamed.version;
            "#,
        )
        .expect("consume restored base");

        assert!(user.result.diagnostics.is_empty());
        assert!(user.result.incomplete.is_empty());
        assert_eq!(user.witness.base_max_source_key, 1);
    }

    #[test]
    fn owned_runtime_product_parts_restore_native_bridge_error_behavior() {
        let restored =
            OwnedLibraryRuntimeState::from_product_parts(compile_semantic_product_parts())
                .expect("restore product parts");
        let user = check_caller_certified_collision_free_source_with_owned_library(
            restored,
            r#"
                declare const nativeValues: number[];
                const wrongNativeItem: string = nativeValues.item;
            "#,
        )
        .expect("consume restored native bridge");

        assert_eq!(user.result.diagnostics.len(), 1);
        assert_eq!(
            user.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2322
        );
        assert!(user.result.incomplete.is_empty());
    }

    #[test]
    fn owned_runtime_product_parts_reject_empty_and_gapped_source_prefixes() {
        let mut empty = compile_semantic_product_parts();
        replace_module_sources(&mut empty, Default::default());
        assert_product_restore_error(empty, "product binder has no retained source ownership");

        let mut gapped = compile_semantic_product_parts();
        let mut sources = gapped
            .binder
            .module_sources()
            .iter()
            .map(|(&scope, &source)| (scope, source))
            .collect::<rustc_hash::FxHashMap<_, _>>();
        let library_scope = sources
            .iter()
            .find_map(|(scope, source)| (source.0 == 1).then_some(*scope))
            .expect("library source owner");
        sources.insert(library_scope, exact_key(2));
        replace_module_sources(&mut gapped, sources);
        assert_product_restore_error(
            gapped,
            "product binder source keys are not the contiguous prelude/library prefix",
        );
    }

    #[test]
    fn owned_runtime_product_parts_reject_group_and_counter_collisions() {
        let mut groups = compile_semantic_product_parts();
        groups.published_types.groups.pop();
        assert_product_restore_error(
            groups,
            "product published type-group count does not match the binder",
        );

        let mut type_parameters = compile_semantic_product_parts();
        type_parameters.next_type_param = 0;
        assert_product_restore_error(
            type_parameters,
            "product type parameter id collides with the next parameter counter",
        );

        let mut classes = compile_semantic_product_parts();
        classes.next_class_id = 0;
        assert_product_restore_error(
            classes,
            "product class id collides with the next class counter",
        );

        let mut renamed = compile_semantic_product_parts();
        let template_group = renamed
            .published_types
            .groups
            .iter_mut()
            .find_map(|terminal| match terminal {
                PublishedTypeGroupTerminal::Ready(group)
                    if group.name == "SeamWitness"
                        && matches!(group.surface, PublishedTypeGroupSurface::Template(_)) =>
                {
                    Some(group)
                }
                PublishedTypeGroupTerminal::Ready(_)
                | PublishedTypeGroupTerminal::Unavailable(_) => None,
            })
            .expect("ready template group");
        template_group.name = "RenamedSeamWitness".to_owned();
        assert_product_restore_error(
            renamed,
            "product published type-group name does not match the binder",
        );
    }

    #[test]
    fn owned_runtime_product_parts_reject_missing_required_class_rows() {
        let mut new_metadata = compile_semantic_product_parts();
        let removed_class = new_metadata
            .runtime
            .class_new_metadata
            .pop()
            .expect("new metadata row")
            .0;
        assert!(new_metadata
            .runtime
            .class_value_bindings
            .iter()
            .any(|(_, binding)| binding.class_id == removed_class));
        assert_product_restore_error(
            new_metadata,
            "product new metadata does not exactly cover published classes",
        );

        let mut applications = compile_semantic_product_parts();
        applications
            .runtime
            .class_application_parameters
            .pop()
            .expect("class application row");
        assert_product_restore_error(
            applications,
            "product class application metadata does not exactly cover published classes",
        );

        let mut names = compile_semantic_product_parts();
        names.runtime.class_names.pop().expect("class name row");
        assert_product_restore_error(
            names,
            "product class names do not exactly match published class groups",
        );

        let mut bindings = compile_semantic_product_parts();
        bindings
            .runtime
            .class_value_bindings
            .pop()
            .expect("class value binding row");
        assert_product_restore_error(
            bindings,
            "product class value bindings do not exactly cover published classes",
        );

        let mut parent_chain = compile_semantic_product_parts();
        let inherited = parent_chain
            .runtime
            .class_new_metadata
            .iter()
            .find_map(|(class, metadata)| {
                (*class != metadata.ctor_declaring_class).then_some(*class)
            })
            .expect("class with inherited constructor owner");
        let parent_row = parent_chain
            .runtime
            .class_parents
            .iter()
            .position(|(class, _)| *class == inherited)
            .expect("inherited class parent row");
        parent_chain.runtime.class_parents.remove(parent_row);
        assert_product_restore_error(
            parent_chain,
            "product constructor owner is not on the class parent chain",
        );

        let mut wrong_name = compile_semantic_product_parts();
        wrong_name.runtime.class_names[0].1 = "DifferentClass".to_owned();
        assert_product_restore_error(
            wrong_name,
            "product class names do not exactly match published class groups",
        );

        let mut generic_bit = compile_semantic_product_parts();
        let generic_class = generic_bit
            .runtime
            .class_application_parameters
            .iter()
            .find_map(|(class, parameters)| (!parameters.is_empty()).then_some(*class))
            .expect("generic class application");
        let binding = generic_bit
            .runtime
            .class_value_bindings
            .iter_mut()
            .find_map(|(_, binding)| (binding.class_id == generic_class).then_some(binding))
            .expect("generic class value binding");
        binding.has_header_type_params = !binding.has_header_type_params;
        assert_product_restore_error(
            generic_bit,
            "product class value binding generic bit does not match application metadata",
        );
    }

    #[test]
    fn owned_runtime_product_parts_reject_dangling_semantic_and_type_references() {
        let mut semantic = compile_semantic_product_parts();
        let LibraryIdentityTerminal::Ready(identity) = &mut semantic
            .semantic_identities
            .as_mut()
            .expect("semantic identities")[0]
        else {
            panic!("Array identity is ready")
        };
        identity.group = TypeGroupId(u32::MAX);
        assert_product_restore_error(
            semantic,
            "product semantic identity refers to an unpublished type group",
        );

        let mut types = compile_semantic_product_parts();
        let ready = types
            .runtime
            .namespace_terminals
            .iter_mut()
            .find_map(|row| match &mut row.terminal {
                FrozenNamespaceValueTerminalSnapshot::Ready { ty, .. } => Some(ty),
                FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => None,
            })
            .expect("ready namespace terminal");
        *ready = TypeId(u32::MAX);
        assert_product_restore_error(
            types,
            "product namespace terminal has an invalid ready reference",
        );
    }

    #[test]
    fn owned_runtime_product_parts_reject_dangling_runtime_binder_references() {
        let mut values = compile_semantic_product_parts();
        values
            .runtime
            .class_value_bindings
            .first_mut()
            .expect("class value binding")
            .0 = ValueStorageId(u32::MAX);
        assert_product_restore_error(
            values,
            "product class value binding has an invalid reference",
        );

        let mut symbols = compile_semantic_product_parts();
        symbols.runtime.named_function_symbols[0] = SymbolId(u32::MAX);
        assert_product_restore_error(
            symbols,
            "product named-function metadata refers to a non-function symbol",
        );

        let mut namespaces = compile_semantic_product_parts();
        namespaces.runtime.namespace_terminals[0].namespace =
            crate::binder::namespace::NamespaceId(u32::MAX);
        assert_product_restore_error(
            namespaces,
            "product namespace terminal refers to an unknown namespace",
        );
    }

    #[test]
    fn phase_timings_are_real_nonoverlapping_measurements() {
        let started = Instant::now();
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "timing.d.ts",
            source: "interface TimingWitness { value: number; }",
        }])
        .expect("focused timing profile");
        let external = started.elapsed();
        assert!(run.phase_timings.measured_total() <= external);
        assert_eq!(run.phase_counts.parse_units, 1);
        assert_eq!(run.phase_counts.bind_units, 1);
        assert_eq!(run.phase_counts.statement_check_units, 1);
    }

    #[test]
    fn release_probe_lines_have_the_exact_v1_contract() {
        let timings = LibraryPhaseTimings {
            parse: Duration::from_micros(2),
            bind: Duration::from_micros(3),
            reserve_fill: Duration::from_micros(5),
            publication_validation: Duration::from_micros(7),
            statement_check: Duration::from_micros(11),
        };
        assert_eq!(
            release_phase_line(
                4,
                Duration::from_micros(1),
                &timings,
                Duration::from_micros(31),
            ),
            "typokat-wu0b-phase-v1 process=4 registry_validation_us=1 parse_us=2 bind_us=3 reserve_fill_us=5 publication_validation_us=7 statement_check_us=11 total_us=31"
        );
        assert_eq!(
            ReleaseOutcomeLine {
                process: 4,
                file_count: 82,
                reserved_records: 13,
                filled_records: 13,
                publication_validations: 9,
                library_diagnostics: 2,
                library_incompletes: 3,
                tiny_parse_errors: 0,
                tiny_diagnostics: 0,
                tiny_incompletes: 0,
            }
            .render(),
            "typokat-wu0b-outcome-v1 process=4 file_count=82 reserved_records=13 filled_records=13 publication_validations=9 library_diagnostics=2 library_incompletes=3 tiny_parse_errors=0 tiny_diagnostics=0 tiny_incompletes=0"
        );
    }

    #[test]
    fn recoverable_parser_diagnostic_fails_closed_before_profile_execution() {
        let file_ordinal = LibraryFileOrdinal::new(3);
        let source = "declare namespace Broken { export = Broken; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        assert_eq!(parsed.diagnostics.len(), 1, "witness must diagnose once");
        assert_eq!(parsed.diagnostics[0].code.scope.as_deref(), Some("TS"));
        assert_eq!(parsed.diagnostics[0].code.number.as_deref(), Some("1063"));
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "recoverable.ts",
            source,
        }]);
        let Err(InjectedProfileError::Parse {
            file_ordinal: actual,
            messages,
        }) = result
        else {
            panic!("recoverable parser diagnostics must abort the injected run");
        };
        assert_eq!(actual, file_ordinal);
        assert!(!messages.is_empty());
    }

    #[test]
    fn focused_shared_interface_identity_and_surface() {
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "first.d.ts",
                source: "interface Shared { first: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(2),
                name: "second.d.ts",
                source: "interface Shared { second: string; }",
            },
        ])
        .expect("focused injected profile");
        let shared = run.global_type_probe("Shared").expect("shared type probe");
        assert_eq!(shared.declaration_count, 2);
        assert_eq!(shared.member_names, ["first", "second"]);
        assert_eq!(run.phase_counts.parse_units, 2);
        assert_eq!(run.phase_counts.bind_units, 2);
        assert_eq!(run.phase_counts.statement_check_units, 2);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert_eq!(run.phase_counts.publication_validations, 1);
        assert_eq!(
            run.reserved_file_ordinals,
            [LibraryFileOrdinal::new(1), LibraryFileOrdinal::new(2)]
        );
        assert!(run.reporting_receipts.is_empty());
        assert!(run.library_records.is_empty());
    }

    #[test]
    fn focused_implementation_diagnostic_keeps_exact_library_key() {
        let file_ordinal = LibraryFileOrdinal::new(7);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "broken.ts",
            source: "const broken: number = 'wrong';",
        }])
        .expect("focused injected profile");
        assert_eq!(run.pass_source_units, [library_unit(file_ordinal)]);
        assert!(!run.lexical_source_units.is_empty());
        assert_eq!(run.library_records.len(), 1);
        assert_eq!(run.library_records[0].0.file_ordinal, file_ordinal);
        let CheckerRecord::Diagnostic(diagnostic) = &run.library_records[0].1 else {
            panic!("implementation mismatch must be a diagnostic");
        };
        assert_eq!(run.library_records[0].0.source_start, 23);
        assert_eq!(diagnostic.span.start, 6);
    }

    /// The route to a shared base drops the library's records; only an explicit inspection
    /// caller collects them (ADR-0018). The ledger is still drained on both routes — that drain
    /// is the completeness gate, not the retention.
    #[test]
    fn the_base_route_retains_no_record_while_the_census_route_collects_them() {
        let injected = [InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(7),
            name: "broken.ts",
            source: "const broken: number = 'wrong';",
        }];

        let (base_run, _base_state) =
            compile_owned_injected_base_profile(&injected).expect("base-route injected profile");
        assert!(base_run.library_records.is_empty());
        assert_eq!(
            base_run.phase_counts.reserved_records,
            base_run.phase_counts.filled_records
        );

        let collected =
            compile_owned_injected_records(&injected).expect("census-route injected profile");
        assert_eq!(collected.len(), 1);
        assert!(matches!(collected[0].1, CheckerRecord::Diagnostic(_)));
    }

    #[test]
    fn focused_function_namespace_and_module_private_probes() {
        let script = LibraryFileOrdinal::new(10);
        let module = LibraryFileOrdinal::new(11);
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: script,
                name: "function.d.ts",
                source: "declare function Merged(value: number): string; declare namespace Merged { export function member(value: string): number; } interface Shared { script: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: module,
                name: "module.d.ts",
                source: "export {}; interface Private { local: boolean; } declare global { interface Shared { module: string; } }",
            },
        ])
        .expect("focused injected profile");

        let merged = run.global_value_probe("Merged").expect("merged value");
        assert_eq!(merged.declaration_count, 2);
        assert_eq!(merged.call_signature_count, 1, "{merged:?}");
        assert_eq!(merged.member_names, ["member"], "{merged:?}");
        assert_eq!(merged.callable_members.len(), 1);
        assert_eq!(merged.callable_members[0].signatures.len(), 1);
        assert!(run.global_type_probe("Private").is_none());
        assert!(run.module_type_probe(module, "Private").is_some());
        assert_eq!(
            run.global_type_probe("Shared")
                .expect("augmented global")
                .declaration_count,
            2
        );
    }

    #[test]
    fn focused_typed_ts1319_claim_is_reported_once_by_binder_consumer() {
        let file_ordinal = LibraryFileOrdinal::new(14);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "export-context.d.ts",
            source: "declare namespace Exported { export default function f(): void; }",
        }])
        .expect("typed TS1319 claim must transfer to binder reporting");
        let receipt = run
            .reporting_receipts
            .iter()
            .find(|receipt| receipt.family == LibraryReportingFamily::ExportContext)
            .expect("export-context receipt");
        assert_eq!(receipt.file_ordinal, file_ordinal);
        assert_eq!(receipt.observed_outcomes, 1);
        assert_eq!(receipt.emitted_records, 1);
        assert_eq!(run.library_records.len(), 1);
        let (key, record) = &run.library_records[0];
        assert_eq!(key.file_ordinal, file_ordinal);
        assert_eq!(key.source_start, 29);
        let CheckerRecord::Incomplete(incomplete) = record else {
            panic!("TK1319 reporting must remain an incomplete record");
        };
        assert_eq!(incomplete.id, "library/export-context/future-tk1319");
        assert_eq!(
            incomplete.context,
            "library export-context TK1319 reporting is deferred beyond snapshot feasibility"
        );
        assert_eq!(incomplete.span, Span::new(29, 63));
        assert_eq!(key.source_start, incomplete.span.start);
    }

    #[test]
    fn mixed_ts1319_and_ts1063_diagnostics_fail_closed() {
        let file_ordinal = LibraryFileOrdinal::new(15);
        let source =
            "declare namespace Mixed { export default function f(): void; export = Mixed; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        let codes = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code.scope.as_deref(),
                    diagnostic.code.number.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert!(codes.contains(&(Some("TS"), Some("1319"))), "{codes:?}");
        assert!(codes.contains(&(Some("TS"), Some("1063"))), "{codes:?}");
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "mixed.ts",
            source,
        }]);
        assert!(matches!(
            result,
            Err(InjectedProfileError::Parse {
                file_ordinal: actual,
                ..
            }) if actual == file_ordinal
        ));
    }

    #[test]
    fn parser_export_claim_inventory_rejects_duplicates_and_unmatched_binder_claims() {
        let claim = ParserExportClaim {
            file_ordinal: LibraryFileOrdinal::new(16),
            span: Span::new(4, 12),
        };
        assert!(matches!(
            match_parser_export_claims(vec![claim, claim], vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
        assert!(matches!(
            match_parser_export_claims(Vec::new(), vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
    }

    #[test]
    fn focused_identical_callable_offsets_keep_exact_owners_and_signatures() {
        let first = LibraryFileOrdinal::new(34);
        let second = LibraryFileOrdinal::new(35);
        let sources = [
            InjectedLibrarySource {
                file_ordinal: first,
                name: "first.d.ts",
                source: "declare namespace OffsetCallable { export function alpha(value: number): string; }\ndeclare function OffsetCallable(): void;",
            },
            InjectedLibrarySource {
                file_ordinal: second,
                name: "second.d.ts",
                source: "declare namespace OffsetCallable { export function bravo(value: string): number; }",
            },
        ];
        let reversed = [sources[1], sources[0]];

        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            let merged = run
                .global_value_probe("OffsetCallable")
                .expect("merged callable");
            let mut members = merged.callable_members.clone();
            members.sort_by(|left, right| left.name.cmp(&right.name));
            assert_eq!(members.len(), 2);
            assert_ne!(members[0].identity, members[1].identity);
            for (member, name, file, parameter, result) in [
                (&members[0], "alpha", first, "number", "string"),
                (&members[1], "bravo", second, "string", "number"),
            ] {
                assert_eq!(member.name, name);
                assert_eq!(member.source, library_unit(file));
                assert_eq!(member.reservation_source, library_unit(file));
                assert_eq!(member.source_start, 42);
                assert_eq!(member.call_signature_count, 1);
                assert_eq!(member.signatures.len(), 1);
                assert_eq!(member.signatures[0].parameter_types, [parameter]);
                assert_eq!(member.signatures[0].return_type, result);
            }
        }
    }

    #[test]
    fn focused_function_namespace_merges_are_input_order_independent() {
        let sources = [
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(30),
                name: "function.d.ts",
                source: "declare function FunctionFirst(value: number): string;",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(31),
                name: "function-namespace.d.ts",
                source: "declare namespace FunctionFirst { export const tag: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(32),
                name: "namespace.d.ts",
                source: "declare namespace NamespaceFirst { export const tag: string; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(33),
                name: "namespace-function.d.ts",
                source: "declare function NamespaceFirst(value: string): number;",
            },
        ];
        let reversed = [sources[3], sources[2], sources[1], sources[0]];
        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            for name in ["FunctionFirst", "NamespaceFirst"] {
                let merged = run.global_value_probe(name).expect("merged value");
                assert_eq!(merged.declaration_count, 2);
                assert_eq!(merged.call_signature_count, 1);
                assert_eq!(merged.member_names, ["tag"]);
            }
            assert!(run.library_records.is_empty());
        }
    }

    #[test]
    fn focused_parse_clean_binder_reporting_families_have_real_receipts() {
        for (index, name, source, family) in [
            (
                10,
                "alias.d.ts",
                "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
                LibraryReportingFamily::LocalAmbientExportAliasFailure,
            ),
            (
                11,
                "placement.ts",
                "namespace Late { export const value = 1; } function Late(): void {}",
                LibraryReportingFamily::PlacementIssue,
            ),
            (
                12,
                "global.d.ts",
                "declare global { interface InvalidScriptGlobal {} }",
                LibraryReportingFamily::GlobalAugmentation,
            ),
            (
                13,
                "umd.d.ts",
                "export as namespace ScriptUmd;",
                LibraryReportingFamily::UmdExportContext,
            ),
            (
                15,
                "member.d.ts",
                "declare namespace MemberRoot { const value: number; }",
                LibraryReportingFamily::NamespaceMember,
            ),
            (
                16,
                "standalone.d.ts",
                "declare namespace StandaloneRoot { const value: number; }",
                LibraryReportingFamily::StandaloneNamespaceValueMember,
            ),
        ] {
            let file_ordinal = LibraryFileOrdinal::new(index);
            let run = run_injected_profile(&[InjectedLibrarySource {
                file_ordinal,
                name,
                source,
            }])
            .expect("binder reporting profile");
            let receipt = run
                .reporting_receipts
                .iter()
                .find(|receipt| receipt.family == family)
                .expect("family receipt");
            assert_eq!(receipt.file_ordinal, file_ordinal);
            assert_eq!(receipt.observed_outcomes, 1);
        }
    }

    const OWNED_MINI_LIBRARY: &str = r#"
        interface Object { toString(): string; }
        interface Function {}
        interface CallableFunction extends Function {
            call<T, A extends unknown[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R;
        }
        interface String { toUpperCase(): string; }
        interface Number { toFixed(fractionDigits?: number): string; }
        interface Boolean { valueOf(): boolean; }
        interface Array<T> {
            map<U>(callbackfn: (value: T) => U): U[];
            push(...items: T[]): number;
        }
        interface ReadonlyArray<T> { map<U>(callbackfn: (value: T) => U): U[]; }
        interface RegExp { test(value: string): boolean; }
        interface HTMLElement {}
        interface HTMLDivElement extends HTMLElement { align: string; }
        interface HTMLElementTagNameMap { div: HTMLDivElement; }
        interface ElementCreationOptions {}
        interface Document {
            createElement<K extends keyof HTMLElementTagNameMap>(
                tagName: K,
                options?: ElementCreationOptions,
            ): HTMLElementTagNameMap[K];
        }
        declare var document: Document;
        declare function increment(value: number): number;
        declare namespace increment { const identity: string; }
        declare class LibraryBase<T = string> {
            constructor(value: T);
            value: T;
        }
        declare namespace LibraryBase { const kind: string; }
        declare class PrivateLibraryClass { private constructor(); }
        declare namespace RuntimeBag { const answer: number; }
    "#;

    const OWNED_MINI_USER: &str = r#"
        class UserSuffix {}
        const mappedOk: string[] = [1].map(value => value.toFixed());
        const mappedBad: number[] = [1].map(value => value.toFixed());
        const readonlyValues: readonly number[] = [1, 2];
        const readonlyBad: number[] = readonlyValues.map(value => value.toFixed());
        readonlyValues.push(3);
        const upperBad: number = "x".toUpperCase();
        const fixedBad: number = (1).toFixed();
        const booleanBad: string = true.valueOf();
        const calledBad: number = ((value: string) => value).call(undefined, "x");
        const objectBad: number = ({ value: 1 }).toString();
        const regexpBad: string = /x/.test("x");
        const domOk: HTMLDivElement = document.createElement("div");
        const domBad: number = document.createElement("div");
        const libraryClass = new LibraryBase<string>("x");
        const libraryValue: string = libraryClass.value;
        const libraryKind: string = LibraryBase.kind;
        const runtimeAnswer: number = RuntimeBag.answer;
        const RuntimeAlias = RuntimeBag;
        const aliasAnswer: number = RuntimeAlias.answer;
        increment.notAFunctionMember;
    "#;

    fn object_overlap_scale_source(width: usize) -> String {
        let source_properties = (0..width)
            .map(|index| format!("  p{index:03}: [{index}];"))
            .collect::<Vec<_>>()
            .join("\n");
        let target_properties = (0..width)
            .map(|index| format!("  p{index:03}: Object;"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "interface ScaleSource {{\n{source_properties}\n}}\n\
             interface ScaleTarget {{\n{target_properties}\n}}\n\
             declare const source: ScaleSource;\n\
             const target: ScaleTarget = source;\n"
        )
    }

    fn object_overlap_relation_frames(width: usize) -> Result<u64, String> {
        let library = format!(
            "{OWNED_MINI_LIBRARY}\n\
             interface Array<T> {{ readonly length: number; }}\n\
             interface ReadonlyArray<T> {{ readonly length: number; }}\n"
        );
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "object-overlap-scale.d.ts",
            source: &library,
        }])
        .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            crate::relate::relation::relation_source_cold_measure(),
            None
        );
        let guard = crate::relate::relation::start_relation_source_cold_measure();
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            &object_overlap_scale_source(width),
        )?;
        let work = crate::relate::relation::relation_source_cold_measure().unwrap_or_default();
        drop(guard);
        assert!(
            run.result.diagnostics.is_empty(),
            "{:?}",
            run.result.diagnostics
        );
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        Ok(work.uncached_relation_frames)
    }

    #[test]
    fn object_overlap_demand_retries_do_not_rescan_named_property_prefixes() -> Result<(), String> {
        const SMALL: usize = 16;
        const LARGE: usize = 32;

        let small = object_overlap_relation_frames(SMALL)?;
        let large = object_overlap_relation_frames(LARGE)?;
        assert!(small > 0, "the scale guard must observe relation work");
        assert!(
            large <= small.saturating_mul(3),
            "doubling Object-overlap width must stay sub-quadratic: small={small}, large={large}"
        );
        Ok(())
    }

    #[test]
    fn owned_generic_promise_identity_preserves_resolve_argument() {
        let library = r#"
            type SeamAwaited<T> = T;
            interface SeamPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SeamPromise<Result>;
            }
            interface SeamPromiseConstructor {
                resolve<T>(value: T): SeamPromise<SeamAwaited<T>>;
            }
            declare var SeamPromise: SeamPromiseConstructor;
        "#;
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "promise-identity.d.ts",
            source: library,
        }])
        .expect("focused Promise identity library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                declare const awaited: SeamAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SeamPromise.resolve(1);
                const numberControl: SeamPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SeamPromise<string> = resolved;
            "#,
        )
        .expect("focused Promise suffix checks");
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        assert_eq!(
            run.result.diagnostics.len(),
            3,
            "{:?}",
            run.result.diagnostics
        );
        assert_eq!(
            run.result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
            ],
        );
    }

    #[test]
    fn owned_conditional_awaited_preserves_non_thenable_argument() {
        let library = r#"
            type SeamAwaited<T> = T extends null | undefined ? T :
                T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
                    F extends ((value: infer V, ...args: infer _) => any) ?
                        SeamAwaited<V> :
                    never :
                T;
            interface SeamPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SeamPromise<Result>;
            }
            interface SeamPromiseConstructor {
                resolve<T>(value: T): SeamPromise<SeamAwaited<T>>;
            }
            declare var SeamPromise: SeamPromiseConstructor;
        "#;
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "promise-awaited.d.ts",
            source: library,
        }])
        .expect("focused conditional Awaited library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                declare const awaited: SeamAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SeamPromise.resolve(1);
                const numberControl: SeamPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SeamPromise<string> = resolved;
            "#,
        )
        .expect("focused conditional Awaited suffix checks");
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        assert_eq!(
            run.result.diagnostics.len(),
            3,
            "{:?}",
            run.result.diagnostics
        );
        assert_eq!(
            run.result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
            ],
        );
    }

    fn owned_state_identity_ends(state: &OwnedLibraryRuntimeState) -> OwnedBaseFinalIdentityEnds {
        OwnedBaseFinalIdentityEnds {
            store: state.interner.store().len(),
            declared_recipes: state.interner.store().all_declared_recipes().count(),
            type_params: usize::try_from(state.next_type_param)
                .expect("base type parameter end fits usize"),
            classes: usize::try_from(state.next_class_id).expect("base class end fits usize"),
            scopes: state.binder.graph.len(),
            symbols: state.binder.symbols.len(),
            declarations: state.binder.declarations.len(),
            type_groups: state.binder.type_groups.len(),
            namespaces: state.binder.namespaces.len(),
            value_storages: state.decl_types.len(),
            source_units: state.binder.checkpoint_ends().next_source,
        }
    }

    #[test]
    fn owned_runtime_forks_share_checker_prefixes_and_keep_empty_private_suffixes() {
        let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-sharing.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned sharing library compiles");
        base.freeze_as_library_base().expect("owned base seals");
        let first = base
            .fork_user_delta_for_test()
            .expect("first owned checker suffix");
        let second = base
            .fork_user_delta_for_test()
            .expect("second owned checker suffix");

        assert!(first.shares_checker_base_with(&second));
        assert_eq!(first.decl_types.local_len(), 0);
        assert_eq!(second.decl_types.local_len(), 0);
        assert_eq!(first.decl_types.len(), base.decl_types.len());
        assert_eq!(
            second.published_types.groups().len(),
            base.published_types.groups().len()
        );
    }

    #[test]
    fn owned_base_final_identity_witness_uses_real_aliases_dense_suffixes_and_base_shapes() {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-final-identities.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned final-identity library compiles");
        let base = owned_state_identity_ends(&state);
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                export type FirstLibraryAlias = LibraryBase<string>;
                export type SecondLibraryAlias = LibraryBase<string>;
                export declare namespace HonestLocalSpace {
                    export interface HonestLocalInterface<T> { value: T; }
                    export class HonestLocalClass<U> {
                        value: U;
                    }
                    export const localValue: HonestLocalClass<number>;
                }
                export const honestReusedFunction = increment;
            "#,
        )
        .expect("owned base captures final identity state");
        assert!(
            run.result.diagnostics.is_empty(),
            "{:?}",
            run.result.diagnostics
        );
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );

        let aliases = &run.final_identity.named_alias_types;
        assert_eq!(
            aliases.get("FirstLibraryAlias"),
            aliases.get("SecondLibraryAlias")
        );
        assert!(aliases.get("FirstLibraryAlias").is_some());
        for (base_end, final_end) in [
            (base.store, run.final_identity.ends.store),
            (base.type_params, run.final_identity.ends.type_params),
            (base.classes, run.final_identity.ends.classes),
            (base.scopes, run.final_identity.ends.scopes),
            (base.symbols, run.final_identity.ends.symbols),
            (base.declarations, run.final_identity.ends.declarations),
            (base.type_groups, run.final_identity.ends.type_groups),
            (base.namespaces, run.final_identity.ends.namespaces),
            (base.value_storages, run.final_identity.ends.value_storages),
        ] {
            assert!(final_end > base_end, "{base_end}..{final_end}");
        }
        let reused = run
            .final_identity
            .reused_base_shape
            .expect("user lowering reuses a non-intrinsic base shape");
        assert!(reused.type_id.index() < base.store);
        assert!(matches!(
            reused.tag,
            TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
        ));
    }

    #[test]
    fn production_check_program_does_not_enter_final_identity_inspection_route() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "type ProductionAlias = { value: number }; const value: ProductionAlias = { value: 1 };",
            SourceType::ts(),
        )
        .parse();
        assert!(!parsed.panicked);
        assert!(parsed.diagnostics.is_empty());
        let inspections_before = super::super::final_identity_inspector_calls_for_test();
        let mut interner = Interner::with_intrinsics();
        let result = super::super::check_program(&mut interner, &parsed.program);
        assert!(result.diagnostics.is_empty());
        assert!(result.incomplete.is_empty());
        assert_eq!(
            super::super::final_identity_inspector_calls_for_test(),
            inspections_before
        );
    }

    #[test]
    fn owned_library_state_checks_caller_certified_collision_free_suffix_without_prefix_rebinding()
    {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let run =
            check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
                state,
                OWNED_MINI_USER,
            )
            .expect("owned mini base accepts a caller-certified collision-free suffix");
        let codes = run
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2339,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2339,
            ]
        );
        assert_eq!(
            run.result
                .diagnostics
                .last()
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("Property 'notAFunctionMember' does not exist on type 'typeof increment'")
        );
        assert!(run.result.incomplete.is_empty());
        assert!(run.witness.array_group_stable);
        assert!(run.witness.document_value_stable);
        assert_eq!(run.witness.store_prefix_stable, Some(true));
        assert!(run.witness.final_store_len >= run.witness.base_store_len);
        assert!(run.witness.final_type_group_count > run.witness.base_type_group_count);
        assert!(run.witness.final_decl_count > run.witness.base_decl_count);
        assert!(run.witness.source_key > run.witness.base_max_source_key);
    }

    #[test]
    fn incomplete_collision_identity_selection_emits_no_user_diagnostics() {
        let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        base.freeze_as_library_base().expect("owned base seals");
        let collision = base
            .fork_sparse_collision_epoch()
            .expect("sparse collision epoch forks");
        let run = super::super::library_identities::
            with_forced_collision_identity_selection_pending(|| {
                check_caller_certified_collision_free_source_with_owned_library(
                    collision,
                    "interface Array<T> { replacementMarker(): T }\nconst wrong: string = [1].replacementMarker();\n",
                )
            })
            .expect("collision publication returns a typed incomplete outcome");

        assert!(
            run.result.diagnostics.is_empty(),
            "{:?}",
            run.result.diagnostics
        );
        assert_eq!(
            run.result.incomplete.len(),
            1,
            "{:?}",
            run.result.incomplete
        );
        assert_eq!(
            run.result.incomplete[0].id,
            "library/publication/semantic-identity-selection-pending"
        );
    }

    #[test]
    fn owned_frozen_interface_heritage_projects_into_local_interface() {
        let (_, state) = compile_owned_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(0),
                name: "owned-mini.d.ts",
                source: OWNED_MINI_LIBRARY,
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "html-element.d.ts",
                source: "interface HTMLElement { elementMarker: string; }",
            },
        ])
        .expect("owned HTMLElement library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"interface Local extends HTMLElement { localMarker: number; }
declare const local: Local;
const inheritedGood: string = local.elementMarker;
const inheritedBad: number = local.elementMarker;
"#,
        )
        .expect("local interface composes its frozen heritage endpoint");

        assert_eq!(run.result.diagnostics.len(), 1);
        assert_eq!(
            run.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2322,
        );
        assert!(
            run.result.incomplete.is_empty(),
            "{:#?}",
            run.result.incomplete
        );
    }

    #[test]
    fn owned_library_preserves_class_and_namespace_runtime_metadata_for_caller_certified_collision_free_suffix(
    ) {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let run =
            check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
                state,
                r#"
                class UserFirst extends LibraryBase<string> { constructor() { super("x"); } }
                const explicit = new LibraryBase<string>("x");
                const value: string = explicit.value;
                const kind: string = LibraryBase.kind;
                const answer: number = RuntimeBag.answer;
                const RuntimeAlias = RuntimeBag;
                const aliasAnswer: number = RuntimeAlias.answer;
                new PrivateLibraryClass();
            "#,
            )
            .expect("owned class and namespace metadata installs");
        assert_eq!(run.result.diagnostics.len(), 1);
        assert_eq!(
            run.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2673
        );
        assert!(run.result.incomplete.is_empty());
        assert_eq!(run.witness.store_prefix_stable, Some(true));
    }

    #[test]
    fn owned_library_continuation_admits_declare_global_syntax() {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let checks_before = super::super::bound_user_check_calls_for_test();
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            "export {}; declare global { interface UserOwnedGlobal { value: string; } }",
        )
        .expect("declare-global continuation binds and checks");
        assert!(run.result.diagnostics.is_empty());
        assert!(run.result.incomplete.is_empty());
        assert_eq!(
            super::super::bound_user_check_calls_for_test(),
            checks_before + 1
        );
    }
}
