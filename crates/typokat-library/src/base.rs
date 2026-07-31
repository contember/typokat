//! Immutable semantic state compiled from the pinned default-library sources.

use super::provider::{LibraryInitCause, LibraryInitError, LibraryInitStage};
use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;
#[cfg(test)]
use crate::check::checker::library_compiler::{
    check_caller_certified_collision_free_project_with_owned_library,
    check_caller_certified_collision_free_source_with_base_evidence, OwnedBaseActualIds,
    OwnedBaseFinalIdentityEnds,
};
#[cfg(test)]
use crate::check::checker::replay_index::{AdmittedCollisionReplayIndex, ReplayOwner};
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
#[cfg(test)]
use std::ops::Range;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Barrier;

#[cfg(test)]
const PROJECTION_SUBTABLES: [&str; 32] = [
    "store.rows",
    "store.payload-tables",
    "store.type-param-constraints",
    "store.frozen-type-params",
    "store.template-names",
    "interner.dedup-buckets",
    "interner.reserved-terminals",
    "interner.declared-recipes",
    "interner.well-known",
    "binder.scopes",
    "binder.symbols",
    "binder.declarations",
    "binder.declaration-site-index",
    "binder.type-groups",
    "binder.namespaces",
    "binder.namespace-indexes",
    "binder.module-sources",
    "decl-types.slots",
    "published-types.groups",
    "published-types.classes",
    "namespace-terminals",
    "function-groups.symbols",
    "class.application-parameters",
    "class.parameter-defaults",
    "class.parents",
    "class.names",
    "class.new-metadata",
    "class.value-identities",
    "class.aliases",
    "semantic-identities",
    "root-name-index.entries",
    "next-ids",
];

#[cfg(test)]
const PRIVATE_REPLAY_BASE_FAMILIES: [&str; 36] = [
    "store.rows",
    "store.payload-tables",
    "store.type-param-constraints",
    "store.frozen-type-params",
    "store.template-names",
    "interner.dedup-buckets",
    "interner.reserved-terminals",
    "interner.well-known",
    "binder.scopes",
    "binder.symbols",
    "binder.declarations",
    "binder.declaration-site-index",
    "binder.type-groups",
    "binder.namespaces",
    "binder.namespace-indexes",
    "binder.module-sources",
    "decl-types.slots",
    "published-types.groups",
    "published-types.classes",
    "namespace-terminals",
    "function-groups.symbols",
    "class.application-parameters",
    "class.parameter-defaults",
    "class.parents",
    "class.names",
    "class.new-metadata",
    "class.value-identities",
    "class.aliases",
    "semantic-identities",
    "root-name-index.entries",
    "next-ids",
    "collision-plan.root-slots",
    "collision-plan.owner-sites",
    "collision-plan.reverse-edges",
    "collision-plan.statement-owners",
    "collision-plan.record-fingerprints",
];

/// The immutable universe every non-colliding check forks from.
///
/// It deliberately holds no library-owned diagnostic or incomplete record: those are the
/// checker's own model gaps, no user reads them, and the pinned suite census is where they are
/// preserved exactly (ADR-0018, narrowing ADR-0011).
pub struct FrozenLibraryBase {
    runtime: OwnedLibraryRuntimeState,
    collision_plan: Option<Arc<crate::check::checker::replay_index::CollisionReplayPlan>>,
    root_names: BTreeSet<String>,
    prefixes: FrozenLibraryPrefixes,
    identity: FrozenLibraryIdentity,
    collision_plan_admitted: bool,
    private_sources: crate::check::checker::library_compiler::PrivateCollisionReplaySourceRegistry,
    #[cfg(test)]
    structural_probe: Option<NonterminalStructuralTypeProbeForTest>,
}

#[doc(hidden)]
pub enum RoutedLibraryProject {
    Shared(OwnedLibraryRuntimeState),
    Private(RoutedPrivateLibraryProject),
    CompleteSourceFallback(Box<CompleteSourceFallbackRuntime>),
}

#[doc(hidden)]
pub struct RoutedPrivateLibraryProject {
    runtime: OwnedLibraryRuntimeState,
    route_receipt: PrivateCollisionRouteReceipt,
    collision_plan: Arc<crate::check::checker::replay_index::CollisionReplayPlan>,
    permit: crate::check::checker::library_compiler::PrivateCollisionReplayPermitToken,
    private_sources: crate::check::checker::library_compiler::PrivateCollisionReplaySourceRegistry,
}

#[doc(hidden)]
pub struct RoutedPrivateRuntime {
    pub state: OwnedLibraryRuntimeState,
    pub permit: crate::check::checker::library_compiler::PrivateCollisionReplayPermitToken,
    pub fallback_seeds: Vec<crate::check::checker::library_compiler::PrivateCollisionReplaySeed>,
}

#[doc(hidden)]
pub enum RoutedPrivateExecution {
    Sparse(Box<RoutedPrivateRuntime>),
    CompleteSourceFallback(Box<CompleteSourceFallbackRuntime>),
}

#[doc(hidden)]
pub struct CompleteSourceFallbackRuntime {
    pub state: OwnedLibraryRuntimeState,
    pub checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
    pub sources: Vec<crate::check::checker::library_compiler::PrivateCollisionReplaySource>,
}

#[doc(hidden)]
pub fn compile_complete_source_fallback_runtime(
    permit: crate::check::checker::library_compiler::PrivateCollisionReplayPermitToken,
    seeds: &[crate::check::checker::library_compiler::PrivateCollisionReplaySeed],
) -> Result<CompleteSourceFallbackRuntime, String> {
    #[cfg(test)]
    crate::check::checker::library_compiler::record_private_replay_fallback_invocation_for_test();
    let profile =
        super::profile::ExactLibraryProfile::load_packaged().map_err(|error| error.to_string())?;
    let owned = super::compiler::owned_library_sources(profile.sources())
        .map_err(|error| error.to_string())?;
    let injected = super::compiler::injected_library_sources(&owned);
    let (state, checkpoint) =
        crate::check::checker::library_compiler::compile_complete_source_replay_runtime(
            &injected, seeds, permit,
        )?;
    let sources = injected
        .into_iter()
        .map(
            |source| crate::check::checker::library_compiler::PrivateCollisionReplaySource {
                file_ordinal: source.file_ordinal,
                name: source.name.to_owned(),
                source: source.source.to_owned(),
            },
        )
        .collect();
    Ok(CompleteSourceFallbackRuntime {
        state,
        checkpoint,
        sources,
    })
}

#[cfg(test)]
thread_local! {
    static PRIVATE_COLLISION_EPOCH_FORKS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
static PRIVATE_EVIDENCE_EPOCHS: AtomicUsize = AtomicUsize::new(1);

#[cfg(test)]
pub(super) struct PrivateCollisionEpochForkScopeForTest(u64);

#[cfg(test)]
impl PrivateCollisionEpochForkScopeForTest {
    pub(super) fn start() -> Self {
        Self(PRIVATE_COLLISION_EPOCH_FORKS.get())
    }

    pub(super) fn finish(self) -> u64 {
        PRIVATE_COLLISION_EPOCH_FORKS.get().saturating_sub(self.0)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateRoutePlanFaultForTest {
    Missing,
    CorruptPrefixBoundary,
    WrongProfileIdentity,
}

impl RoutedPrivateLibraryProject {
    #[doc(hidden)]
    pub fn into_runtime(self) -> Result<OwnedLibraryRuntimeState, &'static str> {
        self.into_runtime_with_permit().map(|runtime| runtime.state)
    }

    #[doc(hidden)]
    pub fn into_runtime_with_permit(self) -> Result<RoutedPrivateRuntime, &'static str> {
        let Self {
            runtime,
            route_receipt,
            collision_plan,
            permit,
            private_sources,
        } = self;
        let seeds = private_collision_replay_seeds(&route_receipt);
        let runtime = runtime.install_private_collision_replay_with_permit(
            collision_plan,
            seeds.clone(),
            permit.clone(),
        )?;
        let selected = runtime.private_collision_source_ordinals()?;
        let sources = selected
            .into_iter()
            .map(|ordinal| private_sources.get(ordinal))
            .collect::<Result<Vec<_>, _>>()?;
        runtime
            .install_private_collision_sources(sources)
            .map(|state| RoutedPrivateRuntime {
                state,
                permit,
                fallback_seeds: seeds,
            })
    }

    #[doc(hidden)]
    pub fn into_runtime_or_complete_source_fallback(
        self,
    ) -> Result<RoutedPrivateExecution, String> {
        let fallback_permit = self.permit.clone();
        let fallback_seeds = private_collision_replay_seeds(&self.route_receipt);
        match self.into_runtime_with_permit() {
            Ok(runtime) => Ok(RoutedPrivateExecution::Sparse(Box::new(runtime))),
            Err(_) => compile_complete_source_fallback_runtime(fallback_permit, &fallback_seeds)
                .map(Box::new)
                .map(RoutedPrivateExecution::CompleteSourceFallback),
        }
    }

    #[cfg(test)]
    pub(super) const fn route_receipt_for_test(&self) -> &PrivateCollisionRouteReceipt {
        &self.route_receipt
    }

    #[cfg(test)]
    pub(super) const fn collision_plan_for_test(
        &self,
    ) -> &Arc<crate::check::checker::replay_index::CollisionReplayPlan> {
        &self.collision_plan
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LibraryProjectRouteError {
    SharedDeltaForkFailed(&'static str),
    PrivateCollisionPlanUnavailable,
    PrivateCollisionPlanAdmissionFailed,
    PrivateEpochForkFailed(&'static str),
    CompleteSourceFallbackFailed(String),
    PreflightRejected { reasons: BTreeSet<String> },
}

impl fmt::Display for LibraryProjectRouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SharedDeltaForkFailed(message) => {
                write!(formatter, "cannot fork the shared library delta: {message}")
            }
            Self::PrivateCollisionPlanUnavailable => {
                formatter.write_str("the frozen library base has no collision replay plan")
            }
            Self::PrivateCollisionPlanAdmissionFailed => formatter
                .write_str("the collision replay plan does not match the frozen library base"),
            Self::PrivateEpochForkFailed(message) => {
                write!(
                    formatter,
                    "cannot fork a private collision epoch: {message}"
                )
            }
            Self::CompleteSourceFallbackFailed(message) => {
                write!(
                    formatter,
                    "cannot compile the complete-source collision fallback: {message}"
                )
            }
            Self::PreflightRejected { reasons } => {
                write!(
                    formatter,
                    "library collision preflight rejected input: {reasons:?}"
                )
            }
        }
    }
}

impl std::error::Error for LibraryProjectRouteError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PrivateCollisionSlot {
    Value,
    Type,
    Namespace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateCollisionModuleClassification {
    Script,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCollisionModuleClassificationEntry {
    pub(super) path: String,
    pub(super) classification: PrivateCollisionModuleClassification,
}

#[cfg(test)]
impl PartialEq<(&str, PrivateCollisionModuleClassification)>
    for PrivateCollisionModuleClassificationEntry
{
    fn eq(&self, other: &(&str, PrivateCollisionModuleClassification)) -> bool {
        self.path == other.0 && self.classification == other.1
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCollisionCandidate {
    pub(super) name: String,
    pub(super) slots: BTreeSet<PrivateCollisionSlot>,
    pub(super) global_object_contributor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCollisionRouteReceipt {
    pub(super) module_classifications: Vec<PrivateCollisionModuleClassificationEntry>,
    pub(super) candidates: Vec<PrivateCollisionCandidate>,
    pub(super) reasons: BTreeSet<String>,
    pub(super) relative_import_edges: usize,
}

fn private_collision_replay_seeds(
    receipt: &PrivateCollisionRouteReceipt,
) -> Vec<crate::check::checker::library_compiler::PrivateCollisionReplaySeed> {
    receipt
        .candidates
        .iter()
        .map(
            |candidate| crate::check::checker::library_compiler::PrivateCollisionReplaySeed {
                name: candidate.name.clone(),
                value: candidate.slots.contains(&PrivateCollisionSlot::Value),
                ty: candidate.slots.contains(&PrivateCollisionSlot::Type),
                namespace: candidate.slots.contains(&PrivateCollisionSlot::Namespace),
                global_object: candidate.global_object_contributor,
            },
        )
        .collect()
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenBaseWitnessForTest {
    pub(super) prefixes: FrozenLibraryPrefixes,
    pub(super) type_count: usize,
    pub(super) root_names: usize,
    pub(super) reference_records: [usize; 3],
}

/// A compiled default library that has not yet been sealed as the shared base.
pub(super) struct CompiledLibraryBase {
    runtime: OwnedLibraryRuntimeState,
    collision_plan: Arc<crate::check::checker::replay_index::CollisionReplayPlan>,
    identity: FrozenLibraryIdentity,
    private_sources: crate::check::checker::library_compiler::PrivateCollisionReplaySourceRegistry,
}

#[cfg(test)]
pub(super) struct CollisionPlanInspectionForTest {
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
    pub(super) root_slot_seeds: usize,
    pub(super) owner_source_sites: usize,
    pub(super) ticket_slots: usize,
    pub(super) ticket_owner_ordered_map_inserts: usize,
    pub(super) owner_site_inner_heap_allocations: usize,
    pub(super) owner_site_dense_slot_writes: usize,
    pub(super) owner_site_ordered_map_inserts: usize,
    pub(super) direct_reverse_edges: usize,
    pub(super) statement_owner_sites: usize,
    pub(super) structured_record_fingerprints: usize,
    pub(super) structured_record_cardinalities: usize,
    pub(super) retained_ast_nodes: usize,
    pub(super) retained_drained_records: usize,
    pub(super) retained_semantic_payload_rows: usize,
    pub(super) retained_full_owner_products: usize,
    pub(super) serialized_artifact_bytes: usize,
    pub(super) prefix_boundaries:
        Vec<crate::check::checker::replay_index::CollisionReplayPrefixBoundary>,
    pub(super) health: crate::check::checker::replay_index::CollisionReplayPlanHealth,
    pub(super) admitted: bool,
}

#[cfg(test)]
pub(super) struct CollisionPlanComparisonForTest {
    pub(super) compact_owner_sites: usize,
    pub(super) full_owner_sites: usize,
    pub(super) compact_direct_edges: usize,
    pub(super) full_direct_edges: usize,
    pub(super) compact_root_slot_consumers: usize,
    pub(super) full_root_slot_consumers: usize,
    pub(super) compact_statement_owners: usize,
    pub(super) full_statement_owners: usize,
    pub(super) compact_record_fingerprints: usize,
    pub(super) full_record_fingerprints: usize,
    pub(super) compact_record_cardinalities: usize,
    pub(super) full_record_cardinalities: usize,
    pub(super) compact_prefix_boundaries: Vec<usize>,
    pub(super) full_prefix_boundaries: Vec<usize>,
    pub(super) binder_source_census_complete: bool,
    pub(super) binder_provenance_complete: bool,
    pub(super) lexical_event_site_audit_complete: bool,
    pub(super) independent_lexical_event_site_audit_complete: bool,
    pub(super) injected_event_capture_corruption_rejected: bool,
    pub(super) global_source_site_audit_complete: bool,
    pub(super) trace_domain_sealed_after_binder_reporting: bool,
    pub(super) injected_late_owner_reservation_rejected: bool,
    pub(super) owner_site_order_is_total: bool,
    pub(super) injected_equal_coordinate_reordering_rejected: bool,
    pub(super) duplicate_owner_site_write_rejected: bool,
    pub(super) first_owner_site_write_preserved: bool,
    pub(super) source_access_manifest_complete: bool,
    pub(super) injected_raw_bypass_rejected: bool,
    pub(super) forbidden_projection_callsite_audit_complete: bool,
    pub(super) typed_reference_coverage_complete: bool,
    pub(super) raw_semantic_access_guard_complete: bool,
}

#[cfg(test)]
pub(super) struct CollisionPlanMutationReceiptForTest {
    pub(super) admitted: bool,
    pub(super) guard_fired: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MappedReplayOwnerSiteForTest {
    pub(super) owner: crate::check::checker::replay_index::ReplayOwner,
    pub(super) file_ordinal: crate::source::LibraryFileOrdinal,
    pub(super) span: crate::span::Span,
    pub(super) syntax_module: crate::binder::scope::ScopeId,
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct LibraryBinderContinuationReceiptForTest {
    pub(super) continuation:
        crate::check::checker::library_compiler::LibraryBinderContinuationForTest,
    pub(super) mapped_owner_sites: Vec<MappedReplayOwnerSiteForTest>,
}

#[cfg(test)]
impl std::ops::Deref for LibraryBinderContinuationReceiptForTest {
    type Target = crate::check::checker::library_compiler::LibraryBinderContinuationForTest;

    fn deref(&self) -> &Self::Target {
        &self.continuation
    }
}

/// The collision replay index a private run assembles for the packaged profile.
///
/// Production retains the direct compact plan (ADR-0020); this full assembly is only its
/// independent suite oracle.
#[cfg(test)]
struct PackagedFullCollisionOracleForTest {
    index: AdmittedCollisionReplayIndex,
    evidence: crate::check::checker::library_compiler::FullCollisionPlanOracleForTest,
}

#[cfg(test)]
fn packaged_full_collision_oracle_for_test() -> &'static PackagedFullCollisionOracleForTest {
    static ORACLE: std::sync::LazyLock<PackagedFullCollisionOracleForTest> =
        std::sync::LazyLock::new(|| {
            let profile = super::profile::ExactLibraryProfile::load_packaged()
                .expect("packaged library profile");
            let compiled = super::compiler::LibraryCompiler::new()
                .compile(&profile)
                .expect("source-compiled library with its collision replay index");
            let evidence =
                crate::check::checker::library_compiler::take_full_collision_plan_oracle_for_test()
                    .expect("full source compile retained its independent plan oracle");
            PackagedFullCollisionOracleForTest {
                index: compiled.replay_index_for_test().clone(),
                evidence,
            }
        });
    &ORACLE
}

#[cfg(test)]
pub(super) fn deferred_packaged_replay_index_for_test() -> &'static AdmittedCollisionReplayIndex {
    &packaged_full_collision_oracle_for_test().index
}

#[cfg(test)]
fn force_packaged_collision_plan_failure_for_test(
    failure: crate::check::checker::library_compiler::ForcedCollisionPlanFailure,
) -> Result<bool, String> {
    let profile =
        super::profile::ExactLibraryProfile::load_packaged().map_err(|error| error.to_string())?;
    let owned = super::compiler::owned_library_sources(profile.sources())
        .map_err(|error| error.to_string())?;
    let injected = super::compiler::injected_library_sources(&owned);
    crate::check::checker::library_compiler::force_collision_plan_failure_for_test(
        &injected, failure,
    )
}

#[cfg(test)]
pub(super) struct RegeneratedReplayIndexForTest {
    index: AdmittedCollisionReplayIndex,
    pub(super) library_source_compiles: u64,
}

#[cfg(test)]
impl std::ops::Deref for RegeneratedReplayIndexForTest {
    type Target = AdmittedCollisionReplayIndex;

    fn deref(&self) -> &Self::Target {
        &self.index
    }
}

#[cfg(test)]
struct LayeredUserDelta {
    runtime: Option<OwnedLibraryRuntimeState>,
    discarded: Arc<AtomicBool>,
}

#[cfg(test)]
impl LayeredUserDelta {
    fn new(base: &FrozenLibraryBase) -> Result<Self, &'static str> {
        let capability = super::collision_preflight::issue_caller_certified_capability();
        let discarded = Arc::new(AtomicBool::new(false));
        let mut runtime = base.runtime.fork_collision_free_user_delta(capability)?;
        runtime.install_user_delta_drop_witness_for_test(Arc::clone(&discarded));
        Ok(Self {
            runtime: Some(runtime),
            discarded,
        })
    }

    fn runtime(&self) -> &OwnedLibraryRuntimeState {
        self.runtime
            .as_ref()
            .expect("user delta runtime is present")
    }

    fn take_runtime(&mut self) -> OwnedLibraryRuntimeState {
        self.runtime.take().expect("user delta runtime is present")
    }

    fn discarded_witness(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.discarded)
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaDomainRangeForTest {
    pub(super) range: Range<usize>,
    pub(super) allocated_ids: Vec<usize>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaRangesForTest {
    pub(super) types: UserDeltaDomainRangeForTest,
    pub(super) type_params: UserDeltaDomainRangeForTest,
    pub(super) classes: UserDeltaDomainRangeForTest,
    pub(super) scopes: UserDeltaDomainRangeForTest,
    pub(super) symbols: UserDeltaDomainRangeForTest,
    pub(super) declarations: UserDeltaDomainRangeForTest,
    pub(super) type_groups: UserDeltaDomainRangeForTest,
    pub(super) namespaces: UserDeltaDomainRangeForTest,
    pub(super) value_storages: UserDeltaDomainRangeForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UserDeltaReferenceSummaryForTest {
    pub(super) base_to_delta: u64,
    pub(super) delta_to_base: u64,
    pub(super) delta_to_delta: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UserDeltaMutationSummaryForTest {
    pub(super) base_rows_written: u64,
    pub(super) local_rows_written: u64,
    pub(super) delta_discarded_after_check: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UserDeltaWorkForTest {
    pub(super) library_source_compiles: u64,
    pub(super) library_source_parses: u64,
    pub(super) library_source_binds: u64,
    pub(super) library_source_checks: u64,
    pub(super) user_source_parses: u64,
    pub(super) user_source_binds: u64,
    pub(super) user_source_checks: u64,
    pub(super) base_row_clones: BTreeMap<&'static str, u64>,
    pub(super) base_rows_sequentially_scanned: BTreeMap<&'static str, u64>,
    pub(super) base_rows_materialized: BTreeMap<&'static str, u64>,
    pub(super) base_rows_remapped: BTreeMap<&'static str, u64>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UserDeltaInterningForTest {
    pub(super) named_alias_types: BTreeMap<String, crate::types::store::TypeId>,
}

#[cfg(test)]
impl UserDeltaInterningForTest {
    pub(super) fn local_alias_type(&self, name: &str) -> Option<crate::types::store::TypeId> {
        self.named_alias_types.get(name).copied()
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaCheckReceiptForTest {
    diagnostics: Vec<String>,
    pub(super) incompletes: Vec<crate::diagnostics::IncompleteSurface>,
    pub(super) ranges: UserDeltaRangesForTest,
    pub(super) interning: UserDeltaInterningForTest,
    pub(super) references: UserDeltaReferenceSummaryForTest,
    pub(super) mutation: UserDeltaMutationSummaryForTest,
    pub(super) work: UserDeltaWorkForTest,
    pub(super) initial_visible_user_names: BTreeSet<String>,
    pub(super) local_names: BTreeSet<String>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) struct UserDeltaProjectInputForTest<'source> {
    pub(super) path: &'source str,
    pub(super) source: &'source str,
}

#[cfg(test)]
pub(super) type PreflightSlotForTest = PrivateCollisionSlot;

#[cfg(test)]
pub(super) type ModuleClassificationForTest = PrivateCollisionModuleClassification;

#[cfg(test)]
pub(super) type ModuleClassificationEntryForTest = PrivateCollisionModuleClassificationEntry;

#[cfg(test)]
pub(super) type CollisionCandidateForTest = PrivateCollisionCandidate;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CollisionRouteForTest {
    SharedDelta,
    PrivateCombined,
    RejectedBeforeSemantics,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CollisionPreflightWorkForTest {
    pub(super) parse_units: u64,
    pub(super) source_nodes_visited: u64,
    pub(super) binding_leaves_visited: u64,
    pub(super) frozen_name_probes: u64,
    pub(super) delta_forks: u64,
    pub(super) delta_local_rows: u64,
    pub(super) user_event_reservations: u64,
    pub(super) durable_evaluator_cache_writes: u64,
    pub(super) durable_projection_cache_writes: u64,
    pub(super) relation_cache_writes: u64,
    pub(super) private_library_compiles: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CollisionPreflightReceiptForTest {
    pub(super) route: CollisionRouteForTest,
    pub(super) capability_issued: bool,
    pub(super) module_classifications: Vec<ModuleClassificationEntryForTest>,
    pub(super) candidates: Vec<CollisionCandidateForTest>,
    pub(super) reasons: BTreeSet<String>,
    pub(super) work: CollisionPreflightWorkForTest,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BaseWorkCalibrationInjectionForTest {
    pub(super) sequential_scans: u64,
    pub(super) materializations: u64,
    pub(super) clones: u64,
    pub(super) remaps: u64,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct UserDeltaScaleObservableForTest {
    diagnostics: Vec<String>,
    pub(super) incompletes: Vec<crate::diagnostics::IncompleteSurface>,
    alias_offsets: BTreeMap<String, usize>,
    type_prefix: usize,
    pub(super) local_type_range: Range<usize>,
}

#[cfg(test)]
impl PartialEq for UserDeltaScaleObservableForTest {
    fn eq(&self, other: &Self) -> bool {
        self.diagnostics == other.diagnostics
            && self.incompletes == other.incompletes
            && self.alias_offsets == other.alias_offsets
            && self.local_type_range.len() == other.local_type_range.len()
    }
}

#[cfg(test)]
impl Eq for UserDeltaScaleObservableForTest {}

#[cfg(test)]
impl UserDeltaScaleObservableForTest {
    pub(super) fn normalized_diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub(super) fn local_alias_type(&self, name: &str) -> Option<crate::types::store::TypeId> {
        self.alias_offsets.get(name).and_then(|offset| {
            let index = self.type_prefix.checked_add(*offset)?;
            u32::try_from(index).ok().map(crate::types::store::TypeId)
        })
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BaseWorkCalibrationReceiptForTest {
    pub(super) observed: BaseWorkCalibrationInjectionForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaScaleMeasureForTest {
    pub(super) frozen_prefixes: FrozenLibraryPrefixes,
    pub(super) observable: UserDeltaScaleObservableForTest,
    pub(super) local_allocations: BTreeMap<&'static str, usize>,
    pub(super) base_rows_sequentially_scanned: BTreeMap<&'static str, u64>,
    pub(super) base_rows_materialized: BTreeMap<&'static str, u64>,
    pub(super) base_rows_cloned: BTreeMap<&'static str, u64>,
    pub(super) base_rows_remapped: BTreeMap<&'static str, u64>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaScaleComparisonForTest {
    pub(super) small: UserDeltaScaleMeasureForTest,
    pub(super) large: UserDeltaScaleMeasureForTest,
    pub(super) calibration: BaseWorkCalibrationReceiptForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaCrossFileForTest {
    pub(super) producer_type_group: crate::binder::declaration::TypeGroupId,
    pub(super) consumer_type_group: crate::binder::declaration::TypeGroupId,
    pub(super) producer_value_storage: crate::binder::declaration::ValueStorageId,
    pub(super) consumer_value_storage: crate::binder::declaration::ValueStorageId,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserDeltaProjectReceiptForTest {
    diagnostics: Vec<String>,
    pub(super) incompletes: Vec<crate::diagnostics::IncompleteSurface>,
    pub(super) ranges: UserDeltaRangesForTest,
    pub(super) references: UserDeltaReferenceSummaryForTest,
    pub(super) mutation: UserDeltaMutationSummaryForTest,
    pub(super) work: UserDeltaWorkForTest,
    pub(super) initial_visible_user_names: BTreeSet<String>,
    pub(super) cross_file: UserDeltaCrossFileForTest,
}

#[cfg(test)]
impl UserDeltaProjectReceiptForTest {
    pub(super) fn normalized_diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

#[cfg(test)]
impl UserDeltaCheckReceiptForTest {
    pub(super) fn normalized_diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(super) struct NonterminalStructuralTypeProbeForTest {
    pub(super) base_type: crate::types::store::TypeId,
    descriptor: crate::types::repr::ObjectType,
}

#[cfg(test)]
impl NonterminalStructuralTypeProbeForTest {
    pub(super) fn descriptor(&self) -> crate::types::repr::ObjectType {
        self.descriptor.clone()
    }
}

#[cfg(test)]
pub(super) struct UserDeltaReinternForTest {
    pub(super) resolved_type: crate::types::store::TypeId,
    pub(super) local_rows_added: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateExecutionForTest {
    SelectiveReplay,
    CompleteSourceFallback,
    SharedDelta,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrivateReplayValidationFailureForTest {
    MissingExpected { fingerprint: String },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateProductionReplayFaultForTest {
    InjectPostBindMutationOwnerAbsentFromPlan,
    OmitExpectedBaselineRecordDuringCompletion,
    DisableSealedExpectedBaselineOwnerBeforeCandidateReservation,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateProductionRouteFaultForTest {
    RejectPlanAdmissionAfterPrivatePreflight,
    OmitExpectedBaselineDuringCheckerCompletion,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrivateLifecycleIdentityFaultForTest {
    None,
    AliasPrivateStorageToSharedBase,
    UseWrapperAddressAfterRelocation,
    UseHardCodedConstant,
    UseFabricatedSerialLifecycleToken,
    SuppressProductionHookCounterIncrement,
    ReportPrivateStateDroppedBeforeActualDrop,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrivateProductionReplayFailureForTest {
    MutationOwnerOutsidePlan { owner: String },
    RequiredSourceNotLoaded { source_ordinal: usize },
    BaselineValidation(PrivateReplayValidationFailureForTest),
    Execution(String),
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrivateProductionFallbackFailureForTest {
    UntrustedProductionEvidence {
        fault: PrivateLifecycleIdentityFaultForTest,
        production_hook_invocations: u64,
        observation_before_fault: String,
        observation_after_fault: String,
        observed_change_count: usize,
    },
    Execution(String),
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionReplayTraceForTest {
    pub(super) bind_completed: bool,
    pub(super) sparse_candidate_execution_started: bool,
    pub(super) completion_selection_started: bool,
    pub(super) mutation_ledger_recorded: u64,
    pub(super) containment_validation_started: u64,
    pub(super) plan_owner_intersection_started: u64,
    pub(super) fault_injected: u64,
    pub(super) candidate_reservation_started: u64,
    pub(super) candidate_activation_started: u64,
    pub(super) baseline_validation_started: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionReplayFaultEvidenceForTest {
    pub(super) injected_after_bind: bool,
    pub(super) injected_owner_key: String,
    pub(super) applied_in_completion_selection: bool,
    pub(super) expected_baseline_fingerprint: String,
    pub(super) applied_before_candidate_reservation: bool,
    pub(super) disabled_baseline_owner_key: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateProductionReplayCandidateForTest {
    pub(super) scheduled_owner_keys: BTreeSet<String>,
}

#[cfg(test)]
pub(super) struct PrivateProductionReplayReceiptForTest {
    pub(super) production_trace: PrivateProductionReplayTraceForTest,
    pub(super) fault: PrivateProductionReplayFaultEvidenceForTest,
    pub(super) post_bind_mutation_ledger_owner_keys: BTreeSet<String>,
    pub(super) plan_owner_keys: BTreeSet<String>,
    pub(super) epoch_library_record_baseline_owner_keys: BTreeSet<String>,
    pub(super) epoch_library_record_baseline_fingerprints: BTreeSet<String>,
    pub(super) candidate_reserved_library_record_owner_keys: BTreeSet<String>,
    pub(super) candidate_activated_library_record_owner_keys: BTreeSet<String>,
    pub(super) candidate:
        Result<PrivateProductionReplayCandidateForTest, PrivateProductionReplayFailureForTest>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionRouteTraceForTest {
    pub(super) preflight_classified_private: bool,
    pub(super) production_route_invocations: u64,
    pub(super) relative_import_edges: u64,
    pub(super) fault_observed: bool,
    pub(super) plan_admission_attempted: bool,
    pub(super) plan_admission_succeeded: bool,
    pub(super) private_runtime_fork_started: bool,
    pub(super) checker_started: bool,
    pub(super) completion_selection_started: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionFallbackMeasurementForTest {
    pub(super) production_route_failures: u64,
    pub(super) full_source_fallback_invocations: u64,
    pub(super) full_source_library_parse_units: u64,
    pub(super) full_source_library_bind_units: u64,
    pub(super) full_base_scan_units: u64,
    pub(super) private_permit_acquisitions: u64,
    pub(super) shared_base_mutations: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionFallbackLifecycleForTest {
    pub(super) permit_acquired: u64,
    pub(super) route_fault_observed: u64,
    pub(super) complete_source_fallback_started: u64,
    pub(super) fallback_state_dropped: u64,
    pub(super) permit_released: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct PrivateProductionLifecycleIdentityForTest {
    pub(super) instrumented_production_hook_invocations: u64,
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct PrivateProductionFallbackReceiptForTest {
    pub(super) execution: PrivateExecutionForTest,
    pub(super) sparse_failure: PrivateProductionReplayFailureForTest,
    pub(super) normalized_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) full_source_oracle: CompleteSourceOracleObservableForTest,
    pub(super) published_root_projection: Vec<String>,
    pub(super) route_trace: PrivateProductionRouteTraceForTest,
    pub(super) measurement: PrivateProductionFallbackMeasurementForTest,
    pub(super) lifecycle: PrivateProductionFallbackLifecycleForTest,
    pub(super) identity: PrivateProductionLifecycleIdentityForTest,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrivateEpochOwnerTokensForTest {
    pub(super) graph: usize,
    pub(super) semantic_identities: usize,
    pub(super) caches: usize,
    pub(super) events: usize,
    pub(super) terminals: usize,
    pub(super) suffixes: usize,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalBaseProjectionForTest {
    pub(super) prefixes: [usize; 9],
    pub(super) reference_records: [usize; 3],
    pub(super) storage_identity: [usize; 8],
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedPreflightForTest {
    pub(super) route: CollisionRouteForTest,
    pub(super) capability_issued: bool,
    pub(super) false_negative_guard_fired: bool,
    pub(super) semantic_ids_before_route: u64,
    pub(super) event_reservations_before_route: u64,
    pub(super) cache_entries_before_route: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedOracleForTest {
    pub(super) candidate_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) full_source_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) candidate_published_root_projection: Vec<String>,
    pub(super) full_source_published_root_projection: Vec<String>,
    pub(super) candidate_semantic_identities: Vec<String>,
    pub(super) full_source_semantic_identities: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayBaseWorkForTest {
    pub(super) sequential_scans: BTreeMap<&'static str, u64>,
    pub(super) materializations: BTreeMap<&'static str, u64>,
    pub(super) clones: BTreeMap<&'static str, u64>,
    pub(super) remaps: BTreeMap<&'static str, u64>,
    pub(super) direct_iterations: BTreeMap<&'static str, u64>,
    pub(super) borrowed_iterations: BTreeMap<&'static str, u64>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedWorkForTest {
    pub(super) second_library_compiles: u64,
    pub(super) candidate_library_bind_units: u64,
    pub(super) candidate_affected_library_parse_units: u64,
    pub(super) oracle_library_parse_units: u64,
    pub(super) oracle_library_bind_units: u64,
    pub(super) canonical_manifest_work: u64,
    pub(super) rendered_record_digest_work: u64,
    pub(super) eager_all_owner_scc_work: u64,
    pub(super) full_base_scans: u64,
    pub(super) full_source_fallbacks: u64,
    pub(super) dependency_edge_escapes: u64,
    pub(super) unexpected_library_records: u64,
    pub(super) replayed_owner_keys: BTreeSet<String>,
    pub(super) source_plan_expected_reverse_closure: BTreeSet<String>,
    pub(super) shared_delta_forks: u64,
    pub(super) unaffected_base_work: PrivateReplayBaseWorkForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedUniverseForTest {
    pub(super) source_plan_profile_identity_verified: bool,
    pub(super) reparsed_sites_match_binder_provenance: bool,
    pub(super) mutation_ledger_contained_by_preflight: bool,
    pub(super) pending_mask_installed_before_queries: bool,
    pub(super) semantic_query_mask_precedes_identity_cache_and_cycle: bool,
    pub(super) provisional_promotions: u64,
    pub(super) new_ids_begin_after_all_nine_prefixes: bool,
    pub(super) shared_mutable_state_references: u64,
    pub(super) shared_immutable_prefix_references: u64,
    pub(super) base_rows_overwritten: u64,
    pub(super) base_interner_keys_overwritten: u64,
    pub(super) affected_replacements_are_append_only: bool,
    pub(super) private_tokens: PrivateEpochOwnerTokensForTest,
    pub(super) shared_tokens: PrivateEpochOwnerTokensForTest,
    pub(super) reachable_stale_affected_rows: u64,
    pub(super) private_state_dropped_after_reports: bool,
    pub(super) affected_terminals_unavailable: BTreeSet<String>,
    pub(super) affected_terminals_from_frozen_prefix: BTreeSet<String>,
    pub(super) private_storage_identity: [usize; 8],
    pub(super) private_owner_tokens_dropped: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedEventsForTest {
    pub(super) user_reservation_cardinality: u64,
    pub(super) full_source_user_reservation_cardinality: u64,
    pub(super) user_records_in_four_key_order: Vec<String>,
    pub(super) full_source_user_records_in_four_key_order: Vec<String>,
    pub(super) library_events_in_user_domains: u64,
    pub(super) unvalidated_affected_record_fingerprints: u64,
    pub(super) unmatched_replayed_library_records: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateMergedIdentityForTest {
    pub(super) user_document_type_group: Option<crate::binder::declaration::TypeGroupId>,
    pub(super) library_document_type_group: Option<crate::binder::declaration::TypeGroupId>,
    pub(super) user_intl_namespace: Option<crate::binder::namespace::NamespaceId>,
    pub(super) library_intl_namespace: Option<crate::binder::namespace::NamespaceId>,
    pub(super) user_parse_int_storage: Option<crate::binder::declaration::ValueStorageId>,
    pub(super) library_parse_int_storage: Option<crate::binder::declaration::ValueStorageId>,
    pub(super) user_array_type_group: Option<crate::binder::declaration::TypeGroupId>,
    pub(super) library_array_type_group: Option<crate::binder::declaration::TypeGroupId>,
    pub(super) private_semantic_identities_reselected: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateCombinedReceiptForTest {
    pub(super) preflight: PrivateCombinedPreflightForTest,
    pub(super) execution: PrivateExecutionForTest,
    pub(super) oracle: PrivateCombinedOracleForTest,
    pub(super) work: PrivateCombinedWorkForTest,
    pub(super) universe: PrivateCombinedUniverseForTest,
    pub(super) events: PrivateCombinedEventsForTest,
    pub(super) normalized_diagnostics: Vec<String>,
    pub(super) replay_seeds: BTreeSet<String>,
    pub(super) merged_identity: PrivateMergedIdentityForTest,
    pub(super) normalized_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) normalized_event_and_ledger_records_by_source: BTreeMap<String, Vec<String>>,
    pub(super) workload_lock_verified: bool,
    pub(super) project_file_count: usize,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RoutedProjectCheckReceiptForTest {
    pub(super) preflight: PrivateCombinedPreflightForTest,
    pub(super) execution: PrivateExecutionForTest,
    pub(super) normalized_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) normalized_diagnostics: Vec<String>,
}

#[cfg(test)]
struct RoutedPrivateCandidateForTest {
    observable: RoutedProjectCheckReceiptForTest,
    route_receipt: PrivateCollisionRouteReceipt,
    affected_owners: BTreeSet<ReplayOwner>,
    owner_sites: Vec<crate::check::checker::replay_index::CollisionReplayOwnerSite>,
    selected_library_ordinals: Vec<crate::source::LibraryFileOrdinal>,
    private_storage_identity: [usize; 8],
    semantic_evidence: crate::check::checker::PrivateProjectSemanticEvidenceForTest,
}

#[cfg(test)]
enum RoutedCandidateForTest {
    Shared(RoutedProjectCheckReceiptForTest),
    Private(Box<RoutedPrivateCandidateForTest>),
}

#[cfg(test)]
enum RoutedProjectProductForTest {
    Shared(Vec<crate::check::checker::CheckResult>),
    Private(Box<crate::check::checker::PrivateProjectSemanticEvidenceForTest>),
}

#[cfg(test)]
struct PrivateProductionScaleProjectForTest {
    execution: PrivateExecutionForTest,
    visible_root_members: BTreeSet<String>,
    replay_base_scan_units: u64,
}

#[cfg(test)]
struct CompleteSourceOracleForTest {
    semantics_by_source: BTreeMap<String, Vec<String>>,
    normalized_root_projection: Vec<String>,
    normalized_semantic_identities: Vec<String>,
    library_parse_units: u64,
    library_bind_units: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ForcedNaiveGlobalCountersForTest {
    pub(super) library_source_compiles: u64,
    pub(super) second_compile_parse_units: u64,
    pub(super) second_compile_bind_units: u64,
    pub(super) second_compile_publication_owners: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ForcedNaiveSparseGateReceiptForTest {
    pub(super) sparse_gate_admitted: bool,
    pub(super) negative_control_fired: bool,
    pub(super) global_counters: ForcedNaiveGlobalCountersForTest,
    pub(super) candidate_receipt_claimed_sparse: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayScaleMeasureForTest {
    pub(super) execution: PrivateExecutionForTest,
    pub(super) observable: BTreeMap<String, Vec<String>>,
    pub(super) candidate_owner_keys: BTreeSet<String>,
    pub(super) source_oracle_owner_keys: BTreeSet<String>,
    pub(super) physical_semantic_work: BTreeMap<&'static str, u64>,
    pub(super) full_source_fallbacks: u64,
    pub(super) frozen_rows: usize,
    pub(super) unaffected_base_work: PrivateReplayBaseWorkForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayScaleComparisonForTest {
    pub(super) small: PrivateReplayScaleMeasureForTest,
    pub(super) large: PrivateReplayScaleMeasureForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UniqueGlobalReplayReceiptForTest {
    pub(super) execution: PrivateExecutionForTest,
    pub(super) full_source_fallbacks: u64,
    pub(super) dependency_edge_escapes: u64,
    pub(super) candidate_seed_keys: BTreeSet<String>,
    pub(super) source_oracle_seed_keys: BTreeSet<String>,
    pub(super) candidate_owner_keys: BTreeSet<String>,
    pub(super) source_oracle_owner_keys: BTreeSet<String>,
    pub(super) candidate_closure_edge_keys: BTreeSet<String>,
    pub(super) source_oracle_closure_edge_keys: BTreeSet<String>,
    pub(super) scheduled_owner_keys: BTreeSet<String>,
    pub(super) considered_closure_edge_keys: BTreeSet<String>,
    pub(super) candidate_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) full_source_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) generated_root_count: usize,
    pub(super) requested_root_count: usize,
    pub(super) generated_global_this_property_count: usize,
    pub(super) scheduler_work: BTreeMap<&'static str, u64>,
    pub(super) user_owner_count: usize,
    pub(super) library_owner_visits: usize,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DomReplayGeneratedShapeForTest {
    pub(super) keyof_event_map_uses: usize,
    pub(super) indexed_event_map_uses: usize,
    pub(super) overload_pairs: usize,
    pub(super) recursive_receivers: usize,
    pub(super) heritage_edges: usize,
    pub(super) collision_seed: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DomReplayReceiptForTest {
    pub(super) execution: PrivateExecutionForTest,
    pub(super) full_source_fallbacks: u64,
    pub(super) dependency_edge_escapes: u64,
    pub(super) candidate_owner_keys: BTreeSet<String>,
    pub(super) source_oracle_owner_keys: BTreeSet<String>,
    pub(super) candidate_closure_edge_keys: BTreeSet<String>,
    pub(super) source_oracle_closure_edge_keys: BTreeSet<String>,
    pub(super) scheduled_owner_keys: BTreeSet<String>,
    pub(super) considered_closure_edge_keys: BTreeSet<String>,
    pub(super) candidate_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) full_source_semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) generated_shape: DomReplayGeneratedShapeForTest,
    pub(super) requested_width: usize,
    pub(super) scheduler_work: BTreeMap<&'static str, u64>,
    pub(super) physical_semantic_work: BTreeMap<&'static str, u64>,
    pub(super) unaffected_base_work: PrivateReplayBaseWorkForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConcurrentPrivateRouteReceiptForTest {
    pub(super) execution: PrivateExecutionForTest,
    pub(super) file_count: usize,
    pub(super) production_private_route_invocations: u64,
    pub(super) production_work_hook_invocations: u64,
    pub(super) sparse_replay_invocations: u64,
    pub(super) full_source_fallback_invocations: u64,
    pub(super) library_source_compiles: u64,
    pub(super) library_source_parse_units: u64,
    pub(super) library_source_bind_units: u64,
    pub(super) full_base_scan_units: u64,
    pub(super) sparse_library_source_units: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConcurrentPrivateProjectResultForTest {
    pub(super) project_identity: String,
    pub(super) visible_user_methods: BTreeSet<String>,
    pub(super) production_visibility_query_invocations: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrivateLifecycleEpochForTest {
    pub(super) epoch_id: usize,
    pub(super) production_hook_invocations: u64,
    pub(super) permit_acquired: u64,
    pub(super) private_work_started: u64,
    pub(super) private_state_dropped: u64,
    pub(super) permit_released: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConcurrentPrivateProjectsReceiptForTest {
    pub(super) route_receipts: Vec<ConcurrentPrivateRouteReceiptForTest>,
    pub(super) start_barrier_arrivals: usize,
    pub(super) production_permit_acquisitions: usize,
    pub(super) production_permit_hook_invocations: u64,
    pub(super) max_private_contenders: usize,
    pub(super) production_peak_private_concurrency: usize,
    pub(super) shared_base_identity_before: [usize; 8],
    pub(super) shared_base_identity_after: [usize; 8],
    pub(super) normalized_results_by_project: Vec<ConcurrentPrivateProjectResultForTest>,
    pub(super) production_lifecycle_epochs: Vec<PrivateLifecycleEpochForTest>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConcurrentPrivateProductionFaultForTest {
    None,
    SuppressProductionPermitInstrumentation,
    InjectCheckerFullBaseScan,
    InjectCheckerFullPlanScan,
    InjectCheckerFullSourceRegistryScan,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConcurrentPrivateProductionFailureForTest {
    Execution(String),
    ProductionPermitInstrumentationMissing {
        attempted_projects: usize,
        production_route_invocations: u64,
        observed_permit_acquisitions: usize,
        observed_permit_hook_invocations: u64,
    },
    FullBaseScanObserved {
        attempted_projects: usize,
        observed_full_base_scan_units: u64,
    },
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrivateReplayOwnerOmissionForTest {
    RootTypeGroup { root_name: String },
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrivateReplayCandidateFailureForTest {
    MissingScheduledOwner { owner: String },
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayCandidateObservableForTest {
    pub(super) semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) published_root_projection: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CompleteSourceOracleObservableForTest {
    pub(super) semantics_by_source: BTreeMap<String, Vec<String>>,
    pub(super) published_root_projection: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayOwnerOmissionEvidenceForTest {
    pub(super) owner: String,
    pub(super) was_in_closed_schedule: bool,
    pub(super) removed_after_closure: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivateReplayOwnerOmissionReceiptForTest {
    pub(super) omission: PrivateReplayOwnerOmissionEvidenceForTest,
    pub(super) candidate_execution: PrivateReplayOwnerOmissionExecutionForTest,
    pub(super) candidate:
        Result<PrivateReplayCandidateObservableForTest, PrivateReplayCandidateFailureForTest>,
    pub(super) full_source_oracle: CompleteSourceOracleObservableForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PrivateReplayOwnerOmissionExecutionForTest {
    pub(super) corrupted_schedule_installed_after_omission: bool,
    pub(super) started: bool,
    pub(super) completion_or_semantic_query_steps: u64,
    pub(super) omitted_owner: String,
}

#[cfg(test)]
fn replay_owner_key_for_test(owner: ReplayOwner) -> String {
    format!("{owner:?}")
}

#[cfg(test)]
fn replay_edge_key_for_test(
    edge: crate::check::checker::replay_index::ReplayReverseEdge,
) -> String {
    format!(
        "{} -> {}",
        replay_owner_key_for_test(edge.dependency),
        replay_owner_key_for_test(edge.consumer)
    )
}

#[cfg(test)]
fn replay_seed_keys_for_test(receipt: &PrivateCollisionRouteReceipt) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for candidate in &receipt.candidates {
        if candidate.global_object_contributor {
            keys.insert("global-object".to_owned());
        }
        if candidate.slots.contains(&PrivateCollisionSlot::Value) {
            keys.insert(format!("value:{}", candidate.name));
        }
        if candidate.slots.contains(&PrivateCollisionSlot::Type) {
            keys.insert(format!("type:{}", candidate.name));
        }
        if candidate.slots.contains(&PrivateCollisionSlot::Namespace) {
            keys.insert(format!("namespace:{}", candidate.name));
        }
    }
    keys
}

#[cfg(test)]
fn independent_replay_closure_for_test(
    plan: &crate::check::checker::replay_index::CollisionReplayPlan,
    receipt: &PrivateCollisionRouteReceipt,
) -> BTreeSet<ReplayOwner> {
    let mut owners = BTreeSet::new();
    for candidate in &receipt.candidates {
        if candidate.global_object_contributor {
            owners.insert(ReplayOwner::GlobalObject);
        }
        if let Some(root) = plan.root_slot(&candidate.name) {
            owners.extend(root.value.map(ReplayOwner::Value));
            owners.extend(root.ty.map(ReplayOwner::TypeGroup));
            owners.extend(root.namespace.map(ReplayOwner::Namespace));
        }
        owners.extend(
            plan.root_consumers(&candidate.name)
                .iter()
                .map(|consumer| consumer.consumer),
        );
    }
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
    owners
}

#[cfg(test)]
fn replay_owner_keys_for_test(owners: &BTreeSet<ReplayOwner>) -> BTreeSet<String> {
    owners
        .iter()
        .copied()
        .map(replay_owner_key_for_test)
        .collect()
}

#[cfg(test)]
fn replay_closure_edge_keys_for_test(
    plan: &crate::check::checker::replay_index::CollisionReplayPlan,
    owners: &BTreeSet<ReplayOwner>,
) -> BTreeSet<String> {
    owners
        .iter()
        .flat_map(|owner| plan.reverse_consumers(*owner))
        .filter(|edge| owners.contains(&edge.consumer))
        .copied()
        .map(replay_edge_key_for_test)
        .collect()
}

impl fmt::Debug for FrozenLibraryBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenLibraryBase")
            .field("root_name_count", &self.root_names.len())
            .field("prefixes", &self.prefixes)
            .field("identity", &self.identity)
            .field("has_collision_plan", &self.collision_plan.is_some())
            .finish_non_exhaustive()
    }
}

impl FrozenLibraryBase {
    #[cfg(test)]
    pub(super) fn check_routed_user_project_with_production_replay_fault_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        fault: PrivateProductionReplayFaultForTest,
    ) -> Result<PrivateProductionReplayReceiptForTest, String> {
        let checker_fault = match fault {
            PrivateProductionReplayFaultForTest::InjectPostBindMutationOwnerAbsentFromPlan => {
                crate::check::checker::library_compiler::PrivateReplayProductionFaultForTest::
                    InjectPostBindMutationOwnerAbsentFromPlan
            }
            PrivateProductionReplayFaultForTest::OmitExpectedBaselineRecordDuringCompletion => {
                crate::check::checker::library_compiler::PrivateReplayProductionFaultForTest::
                    OmitExpectedBaselineRecordDuringCompletion
            }
            PrivateProductionReplayFaultForTest::
                DisableSealedExpectedBaselineOwnerBeforeCandidateReservation => {
                crate::check::checker::library_compiler::PrivateReplayProductionFaultForTest::
                    DisableSealedExpectedBaselineOwnerBeforeCandidateReservation
            }
        };
        let scope =
            crate::check::checker::library_compiler::PrivateReplayProductionScopeForTest::start(
                checker_fault,
            )
            .map_err(str::to_owned)?;
        let candidate_run = self.checked_routed_project_for_test_inner(inputs);
        let trace = scope.finish().map_err(str::to_owned)?;
        let candidate = match trace.failure {
            Some(
                crate::check::checker::library_compiler::
                    PrivateReplayProductionFailureForTest::MutationOwnerOutsidePlan(owner),
            ) => Err(PrivateProductionReplayFailureForTest::MutationOwnerOutsidePlan {
                owner: replay_owner_key_for_test(owner),
            }),
            Some(
                crate::check::checker::library_compiler::
                    PrivateReplayProductionFailureForTest::RequiredSourceNotLoaded(source),
            ) => Err(PrivateProductionReplayFailureForTest::RequiredSourceNotLoaded {
                source_ordinal: source.index(),
            }),
            Some(
                crate::check::checker::library_compiler::
                    PrivateReplayProductionFailureForTest::BaselineMissing,
            ) => {
                let fingerprint = trace
                    .omitted_expected_baseline
                    .as_ref()
                    .or_else(|| {
                        trace.disabled_baseline_owner.and_then(|owner| {
                            trace
                                .epoch_library_record_baselines
                                .iter()
                                .find(|record| record.owner == owner)
                        })
                    })
                    .map(|record| format!("{record:?}"))
                    .unwrap_or_else(|| "missing production baseline".to_owned());
                Err(PrivateProductionReplayFailureForTest::BaselineValidation(
                    PrivateReplayValidationFailureForTest::MissingExpected { fingerprint },
                ))
            }
            Some(
                crate::check::checker::library_compiler::
                    PrivateReplayProductionFailureForTest::Other,
            ) => Err(PrivateProductionReplayFailureForTest::Execution(
                candidate_run
                    .err()
                    .unwrap_or_else(|| "private replay failed without a typed cause".to_owned()),
            )),
            None => match candidate_run {
                Ok(_) => Ok(PrivateProductionReplayCandidateForTest {
                    scheduled_owner_keys: trace
                        .scheduled_owners
                        .iter()
                        .copied()
                        .map(replay_owner_key_for_test)
                        .collect(),
                }),
                Err(message) => Err(PrivateProductionReplayFailureForTest::Execution(message)),
            },
        };
        let injected_owner_key = trace
            .injected_owner
            .map(replay_owner_key_for_test)
            .unwrap_or_default();
        let disabled_baseline_record = trace.disabled_baseline_owner.and_then(|owner| {
            trace
                .epoch_library_record_baselines
                .iter()
                .find(|record| record.owner == owner)
        });
        let expected_baseline_fingerprint = trace
            .omitted_expected_baseline
            .as_ref()
            .or(disabled_baseline_record)
            .map(|record| format!("{record:?}"))
            .unwrap_or_default();
        let disabled_baseline_owner_key = trace
            .disabled_baseline_owner
            .map(replay_owner_key_for_test)
            .unwrap_or_default();
        Ok(PrivateProductionReplayReceiptForTest {
            production_trace: PrivateProductionReplayTraceForTest {
                bind_completed: trace.bind_completed,
                sparse_candidate_execution_started: trace.sparse_candidate_execution_started,
                completion_selection_started: trace.completion_selection_started,
                mutation_ledger_recorded: trace.mutation_ledger_recorded,
                containment_validation_started: trace.containment_validation_started,
                plan_owner_intersection_started: trace.plan_owner_intersection_started,
                fault_injected: trace.fault_injected,
                candidate_reservation_started: trace.candidate_reservation_started,
                candidate_activation_started: trace.candidate_activation_started,
                baseline_validation_started: trace.baseline_validation_started,
            },
            fault: PrivateProductionReplayFaultEvidenceForTest {
                injected_after_bind: trace.injected_after_bind,
                injected_owner_key,
                applied_in_completion_selection: trace.omitted_expected_baseline.is_some(),
                expected_baseline_fingerprint,
                applied_before_candidate_reservation: trace.disabled_baseline_owner.is_some(),
                disabled_baseline_owner_key,
            },
            post_bind_mutation_ledger_owner_keys: trace
                .post_bind_mutation_owners
                .iter()
                .copied()
                .map(replay_owner_key_for_test)
                .collect(),
            plan_owner_keys: trace
                .plan_owners
                .iter()
                .copied()
                .map(replay_owner_key_for_test)
                .collect(),
            epoch_library_record_baseline_owner_keys: trace
                .epoch_library_record_baselines
                .iter()
                .map(|record| replay_owner_key_for_test(record.owner))
                .collect(),
            epoch_library_record_baseline_fingerprints: trace
                .epoch_library_record_baselines
                .iter()
                .map(|record| format!("{record:?}"))
                .collect(),
            candidate_reserved_library_record_owner_keys: trace
                .candidate_reserved_library_record_owners
                .iter()
                .copied()
                .map(replay_owner_key_for_test)
                .collect(),
            candidate_activated_library_record_owner_keys: trace
                .candidate_activated_library_record_owners
                .iter()
                .copied()
                .map(replay_owner_key_for_test)
                .collect(),
            candidate,
        })
    }

    #[cfg(test)]
    pub(super) fn check_routed_user_project_with_production_fault_and_fallback_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        fault: PrivateProductionRouteFaultForTest,
        identity_fault: PrivateLifecycleIdentityFaultForTest,
    ) -> Result<PrivateProductionFallbackReceiptForTest, PrivateProductionFallbackFailureForTest>
    {
        let execution_error =
            |message: String| PrivateProductionFallbackFailureForTest::Execution(message);
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let route_receipt =
            match super::collision_preflight::preflight_file_inputs(&self.root_names, &files) {
                super::collision_preflight::RoutedProjectPreflight::Private(receipt) => receipt,
                _ => {
                    return Err(execution_error(
                        "production fault control did not classify private".to_owned(),
                    ));
                }
            };
        let shared_base_before = self.runtime.inner_base_allocation_identity_for_test();
        let hooks_before =
            crate::check::checker::library_compiler::private_replay_hook_invocations_for_test();
        let epochs_before =
            crate::check::checker::library_compiler::private_replay_epoch_count_for_test();
        let permit =
            crate::check::checker::library_compiler::acquire_private_collision_replay_permit()
                .map_err(|message| execution_error(message.to_owned()))?;
        let permit_acquired = permit.acquired_event_for_test();

        let inner_identity_before = permit.inner_allocation_identity_for_test();
        let attested_permit = permit.clone();
        let inner_identity_after = attested_permit.inner_allocation_identity_for_test();
        let wrapper_address = std::ptr::from_ref(&attested_permit).addr();
        let hooks_after_acquire =
            crate::check::checker::library_compiler::private_replay_hook_invocations_for_test();
        let production_hook_invocations = hooks_after_acquire.saturating_sub(hooks_before);
        let observation_before_fault = format!(
            "storage={inner_identity_after};hooks={production_hook_invocations};acquired={permit_acquired}"
        );
        let observation_after_fault = match identity_fault {
            PrivateLifecycleIdentityFaultForTest::None => observation_before_fault.clone(),
            PrivateLifecycleIdentityFaultForTest::AliasPrivateStorageToSharedBase => format!(
                "storage={shared_base_before};hooks={production_hook_invocations};acquired={permit_acquired}"
            ),
            PrivateLifecycleIdentityFaultForTest::UseWrapperAddressAfterRelocation => format!(
                "storage={wrapper_address};hooks={production_hook_invocations};acquired={permit_acquired}"
            ),
            PrivateLifecycleIdentityFaultForTest::UseHardCodedConstant => {
                "storage=constant;hooks=constant;acquired=constant".to_owned()
            }
            PrivateLifecycleIdentityFaultForTest::UseFabricatedSerialLifecycleToken => format!(
                "storage={inner_identity_after};hooks={production_hook_invocations};serial={}",
                crate::check::checker::library_compiler::private_replay_epoch_count_for_test()
            ),
            PrivateLifecycleIdentityFaultForTest::SuppressProductionHookCounterIncrement => {
                format!(
                    "storage={inner_identity_after};suppressed-hooks={hooks_before};acquired={permit_acquired}"
                )
            }
            PrivateLifecycleIdentityFaultForTest::ReportPrivateStateDroppedBeforeActualDrop => {
                let release = crate::check::checker::library_compiler::
                    private_replay_last_release_event_for_test();
                format!(
                    "storage={inner_identity_after};hooks={production_hook_invocations};released={release}"
                )
            }
        };
        if identity_fault != PrivateLifecycleIdentityFaultForTest::None {
            drop(attested_permit);
            drop(permit);
            let observed_change_count = observation_before_fault
                .bytes()
                .zip(observation_after_fault.bytes())
                .filter(|(before, after)| before != after)
                .count()
                .saturating_add(
                    observation_before_fault
                        .len()
                        .abs_diff(observation_after_fault.len()),
                );
            return Err(
                PrivateProductionFallbackFailureForTest::UntrustedProductionEvidence {
                    fault: identity_fault,
                    production_hook_invocations,
                    observation_before_fault,
                    observation_after_fault,
                    observed_change_count,
                },
            );
        }
        if inner_identity_before != inner_identity_after {
            return Err(execution_error(
                "private permit storage identity changed across an authenticated clone".to_owned(),
            ));
        }
        drop(attested_permit);

        let collision_plan = self
            .collision_plan
            .as_ref()
            .cloned()
            .ok_or_else(|| execution_error("frozen base has no collision plan".to_owned()))?;
        let mut route_trace = PrivateProductionRouteTraceForTest {
            preflight_classified_private: true,
            production_route_invocations: 1,
            relative_import_edges: u64::try_from(route_receipt.relative_import_edges)
                .unwrap_or(u64::MAX),
            plan_admission_attempted: true,
            ..PrivateProductionRouteTraceForTest::default()
        };
        let sparse_failure = match fault {
            PrivateProductionRouteFaultForTest::RejectPlanAdmissionAfterPrivatePreflight => {
                let rejected = collision_plan
                    .admit_for_frozen_base(self.prefixes.to_array(), "faulted-profile-identity")
                    .is_err();
                route_trace.fault_observed = rejected;
                PrivateProductionReplayFailureForTest::Execution(
                    "private collision plan admission rejected".to_owned(),
                )
            }
            PrivateProductionRouteFaultForTest::OmitExpectedBaselineDuringCheckerCompletion => {
                collision_plan
                    .admit_for_frozen_base(self.prefixes.to_array(), &self.identity.profile_sha256)
                    .map_err(|_| {
                        execution_error("private collision plan admission failed".to_owned())
                    })?;
                route_trace.plan_admission_succeeded = true;
                route_trace.private_runtime_fork_started = true;
                let seeds = route_receipt
                    .candidates
                    .iter()
                    .map(|candidate| {
                        crate::check::checker::library_compiler::PrivateCollisionReplaySeed {
                            name: candidate.name.clone(),
                            value: candidate.slots.contains(&PrivateCollisionSlot::Value),
                            ty: candidate.slots.contains(&PrivateCollisionSlot::Type),
                            namespace: candidate.slots.contains(&PrivateCollisionSlot::Namespace),
                            global_object: candidate.global_object_contributor,
                        }
                    })
                    .collect();
                let mut state = self
                    .runtime
                    .fork_sparse_collision_epoch()
                    .and_then(|runtime| {
                        runtime.install_private_collision_replay_with_permit(
                            Arc::clone(&collision_plan),
                            seeds,
                            permit.clone(),
                        )
                    })
                    .map_err(|message| execution_error(message.to_owned()))?;
                let selected = state
                    .private_collision_source_ordinals()
                    .map_err(|message| execution_error(message.to_owned()))?;
                let sources = selected
                    .into_iter()
                    .map(|ordinal| self.private_sources.get(ordinal))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|message| execution_error(message.to_owned()))?;
                state = state
                    .install_private_collision_sources(sources)
                    .map_err(|message| execution_error(message.to_owned()))?;
                let auxiliary = state
                    .take_private_collision_sources()
                    .map_err(|message| execution_error(message.to_owned()))?
                    .into_iter()
                    .map(|source| crate::frontend::AuxiliarySourceInput {
                        source_ordinal: source.file_ordinal.index(),
                        name: source.name,
                        source: source.source,
                    })
                    .collect();
                let roots = route_receipt
                    .candidates
                    .iter()
                    .map(|candidate| candidate.name.clone())
                    .collect::<Vec<_>>();
                let scope = crate::check::checker::library_compiler::
                    PrivateReplayProductionScopeForTest::start(
                        crate::check::checker::library_compiler::
                            PrivateReplayProductionFaultForTest::
                                OmitExpectedBaselineRecordDuringCompletion,
                    )
                    .map_err(|message| execution_error(message.to_owned()))?;
                route_trace.checker_started = true;
                let run = crate::frontend::run_project_frontend_with_auxiliary(
                    files.clone(),
                    auxiliary,
                    move |_, library_programs, units| {
                        crate::check::checker::check_private_project_programs_with_library_evidence(
                            state,
                            library_programs,
                            units,
                            &roots,
                        )
                    },
                );
                let trace = scope
                    .finish()
                    .map_err(|message| execution_error(message.to_owned()))?;
                route_trace.completion_selection_started = trace.completion_selection_started;
                route_trace.fault_observed = trace.omitted_expected_baseline.is_some();
                match trace.failure {
                    Some(
                        crate::check::checker::library_compiler::
                            PrivateReplayProductionFailureForTest::BaselineMissing,
                    ) => {
                        let fingerprint = trace
                            .omitted_expected_baseline
                            .as_ref()
                            .map(|record| format!("{record:?}"))
                            .unwrap_or_else(|| "missing production baseline".to_owned());
                        PrivateProductionReplayFailureForTest::BaselineValidation(
                            PrivateReplayValidationFailureForTest::MissingExpected {
                                fingerprint,
                            },
                        )
                    }
                    _ => {
                        let message = run
                            .product
                            .err()
                            .unwrap_or("checker completion fault did not fail sparse replay");
                        PrivateProductionReplayFailureForTest::Execution(message.to_owned())
                    }
                }
            }
        };
        let route_fault_observed =
            crate::check::checker::library_compiler::next_private_replay_lifecycle_event_for_test();

        let complete_source_fallback_started =
            crate::check::checker::library_compiler::next_private_replay_lifecycle_event_for_test();
        let fallback_work =
            crate::check::checker::library_compiler::CanonicalLibraryFrontendWorkScopeForTest::start(
            );
        let fallback_scan_run =
            crate::check::checker::library_compiler::PrivateReplayScaleRunForTest::start();
        let fallback_scan_scope =
            crate::check::checker::library_compiler::PrivateReplayScaleRouteScopeForTest::start(
                &fallback_scan_run,
                false,
                false,
                false,
                false,
            )
            .map_err(|message| execution_error(message.to_owned()))?;
        let fallback_seeds = private_collision_replay_seeds(&route_receipt);
        let fallback = compile_complete_source_fallback_runtime(permit.clone(), &fallback_seeds);
        let fallback_scan_trace = fallback_scan_scope
            .finish()
            .map_err(|message| execution_error(message.to_owned()))?;
        let fallback = fallback.map_err(execution_error)?;
        let fallback_work = fallback_work.finish();
        let auxiliary = fallback
            .sources
            .iter()
            .cloned()
            .map(|source| crate::frontend::AuxiliarySourceInput {
                source_ordinal: source.file_ordinal.index(),
                name: source.name,
                source: source.source,
            })
            .collect();
        let roots = route_receipt
            .candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect::<Vec<_>>();
        let run = crate::frontend::run_project_frontend_with_auxiliary(
            files,
            auxiliary,
            move |_, library_programs, units| {
                crate::check::checker::check_complete_library_project_programs_with_evidence(
                    fallback.state,
                    fallback.checkpoint,
                    library_programs,
                    units,
                    &roots,
                )
            },
        );
        let evidence = run
            .product
            .map_err(|message| execution_error(message.to_owned()))?;
        let mut semantics = run
            .inputs
            .iter()
            .map(|input| (input.name.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for result in evidence.results {
            let path = run
                .inputs
                .get(result.module_ordinal.index())
                .map(|input| input.name.as_str())
                .ok_or_else(|| execution_error("fallback returned unknown module".to_owned()))?;
            let rows = semantics
                .get_mut(path)
                .ok_or_else(|| execution_error("fallback path is absent".to_owned()))?;
            rows.extend(
                result
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                    })
                    .chain(
                        result
                            .incomplete
                            .into_iter()
                            .map(|incomplete| format!("incomplete {incomplete:?}")),
                    ),
            );
        }
        let published_root_projection = evidence.normalized_root_projection;
        let fallback_state_dropped =
            crate::check::checker::library_compiler::next_private_replay_lifecycle_event_for_test();
        drop(permit);
        let permit_released =
            crate::check::checker::library_compiler::private_replay_last_release_event_for_test();
        let hooks_after =
            crate::check::checker::library_compiler::private_replay_hook_invocations_for_test();
        let epochs_after =
            crate::check::checker::library_compiler::private_replay_epoch_count_for_test();
        let private_permit_acquisitions =
            u64::try_from(epochs_after.saturating_sub(epochs_before)).unwrap_or(u64::MAX);
        let shared_base_mutations =
            u64::from(self.runtime.inner_base_allocation_identity_for_test() != shared_base_before);
        let production_route_failures = u64::from(route_trace.fault_observed);
        Ok(PrivateProductionFallbackReceiptForTest {
            execution: PrivateExecutionForTest::CompleteSourceFallback,
            sparse_failure,
            normalized_semantics_by_source: semantics.clone(),
            full_source_oracle: CompleteSourceOracleObservableForTest {
                semantics_by_source: semantics,
                published_root_projection: published_root_projection.clone(),
            },
            published_root_projection,
            route_trace,
            measurement: PrivateProductionFallbackMeasurementForTest {
                production_route_failures,
                full_source_fallback_invocations: fallback_work.entries,
                full_source_library_parse_units: fallback_work.parse_units,
                full_source_library_bind_units: fallback_work.bind_units,
                full_base_scan_units: fallback_scan_trace.full_base_scan_units,
                private_permit_acquisitions,
                shared_base_mutations,
            },
            lifecycle: PrivateProductionFallbackLifecycleForTest {
                permit_acquired,
                route_fault_observed,
                complete_source_fallback_started,
                fallback_state_dropped,
                permit_released,
            },
            identity: PrivateProductionLifecycleIdentityForTest {
                instrumented_production_hook_invocations: hooks_after.saturating_sub(hooks_before),
            },
        })
    }

    #[cfg(test)]
    fn zero_private_base_work_for_test() -> PrivateReplayBaseWorkForTest {
        let families = PRIVATE_REPLAY_BASE_FAMILIES
            .into_iter()
            .map(|family| (family, 0))
            .collect::<BTreeMap<_, _>>();
        PrivateReplayBaseWorkForTest {
            sequential_scans: families.clone(),
            materializations: families.clone(),
            clones: families.clone(),
            remaps: families.clone(),
            direct_iterations: families.clone(),
            borrowed_iterations: families,
        }
    }

    #[cfg(test)]
    fn private_combined_preflight_for_test(
        receipt: &CollisionPreflightReceiptForTest,
    ) -> PrivateCombinedPreflightForTest {
        PrivateCombinedPreflightForTest {
            route: receipt.route,
            capability_issued: receipt.capability_issued,
            false_negative_guard_fired: false,
            semantic_ids_before_route: 0,
            event_reservations_before_route: receipt.work.user_event_reservations,
            cache_entries_before_route: receipt
                .work
                .durable_evaluator_cache_writes
                .saturating_add(receipt.work.durable_projection_cache_writes)
                .saturating_add(receipt.work.relation_cache_writes),
        }
    }

    #[cfg(test)]
    fn checked_routed_project_for_test_inner(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<RoutedCandidateForTest, String> {
        let preflight = self.preflight_user_project_for_test(inputs)?;
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let mut private_evidence = None;
        let mut private_permit = None;
        let (execution, run) = match self
            .route_user_project(&files)
            .map_err(|error| error.to_string())?
        {
            RoutedLibraryProject::Shared(state) => {
                let run = crate::frontend::run_project_frontend(files, move |_, units| {
                    crate::check::checker::check_project_programs_with_library(state, units)
                        .map(RoutedProjectProductForTest::Shared)
                });
                (PrivateExecutionForTest::SharedDelta, run)
            }
            RoutedLibraryProject::Private(private) => {
                let route_receipt = private.route_receipt_for_test().clone();
                let root_names = route_receipt
                    .candidates
                    .iter()
                    .map(|candidate| candidate.name.clone())
                    .collect::<Vec<_>>();
                match private
                    .into_runtime_or_complete_source_fallback()
                    .map_err(|error| error.to_string())?
                {
                    RoutedPrivateExecution::Sparse(runtime) => {
                        let runtime = *runtime;
                        let mut state = runtime.state;
                        private_permit = Some(runtime.permit);
                        let affected_owners = state
                            .private_collision_affected_owners_for_test()
                            .map_err(str::to_owned)?;
                        let owner_sites = state
                            .private_collision_owner_sites_for_test()
                            .map_err(str::to_owned)?;
                        let selected_library_ordinals = state
                            .private_collision_source_ordinals()
                            .map_err(str::to_owned)?;
                        let private_storage_identity = state.storage_identity_for_test();
                        private_evidence = Some((
                            route_receipt,
                            affected_owners,
                            owner_sites,
                            selected_library_ordinals,
                            private_storage_identity,
                        ));
                        let auxiliary = state
                            .take_private_collision_sources()
                            .map_err(str::to_owned)?
                            .into_iter()
                            .map(|source| crate::frontend::AuxiliarySourceInput {
                                source_ordinal: source.file_ordinal.index(),
                                name: source.name,
                                source: source.source,
                            })
                            .collect();
                        let run = crate::frontend::run_project_frontend_with_auxiliary(
                            files,
                            auxiliary,
                            move |_, library_programs, units| {
                                crate::check::checker::
                                    check_private_project_programs_with_library_evidence(
                                        state,
                                        library_programs,
                                        units,
                                        &root_names,
                                    )
                                    .map(Box::new)
                                    .map(RoutedProjectProductForTest::Private)
                            },
                        );
                        (PrivateExecutionForTest::SelectiveReplay, run)
                    }
                    RoutedPrivateExecution::CompleteSourceFallback(fallback) => {
                        let CompleteSourceFallbackRuntime {
                            state,
                            checkpoint,
                            sources,
                        } = *fallback;
                        let private_storage_identity = state.storage_identity_for_test();
                        private_evidence = Some((
                            route_receipt,
                            BTreeSet::new(),
                            Vec::new(),
                            Vec::new(),
                            private_storage_identity,
                        ));
                        let auxiliary = sources
                            .into_iter()
                            .map(|source| crate::frontend::AuxiliarySourceInput {
                                source_ordinal: source.file_ordinal.index(),
                                name: source.name,
                                source: source.source,
                            })
                            .collect();
                        let run = crate::frontend::run_project_frontend_with_auxiliary(
                            files,
                            auxiliary,
                            move |_, library_programs, units| {
                                crate::check::checker::
                                    check_complete_library_project_programs_with_evidence(
                                        state,
                                        checkpoint,
                                        library_programs,
                                        units,
                                        &root_names,
                                    )
                                    .map(Box::new)
                                    .map(RoutedProjectProductForTest::Private)
                            },
                        );
                        (PrivateExecutionForTest::CompleteSourceFallback, run)
                    }
                }
            }
            RoutedLibraryProject::CompleteSourceFallback(_) => {
                return Err("test route unexpectedly selected complete-source fallback".to_owned());
            }
        };
        if run.parse_errors.iter().any(|errors| !errors.is_empty()) {
            return Err("test project contains parse errors".to_owned());
        }
        let product = run.product.map_err(str::to_owned)?;
        let (results, semantic_evidence) = match product {
            RoutedProjectProductForTest::Shared(results) => (results, None),
            RoutedProjectProductForTest::Private(evidence) => {
                let mut evidence = *evidence;
                let results = std::mem::take(&mut evidence.results);
                (results, Some(evidence))
            }
        };
        let mut semantics = run
            .inputs
            .iter()
            .map(|input| (input.name.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let mut diagnostics = Vec::new();
        for result in results {
            let path = run
                .inputs
                .get(result.module_ordinal.index())
                .map(|input| input.name.as_str())
                .ok_or_else(|| "checker returned an unknown project module".to_owned())?;
            let rows = semantics
                .get_mut(path)
                .ok_or_else(|| "normalized project path is absent".to_owned())?;
            for diagnostic in result.diagnostics {
                let row = format!("{} {}", diagnostic.code.as_str(), diagnostic.message);
                diagnostics.push(row.clone());
                rows.push(row);
            }
            for incomplete in result.incomplete {
                rows.push(format!("incomplete {incomplete:?}"));
            }
        }
        if let Some(evidence) = semantic_evidence.as_ref() {
            for rows in semantics.values_mut() {
                rows.clear();
            }
            for (module_ordinal, row) in &evidence.normalized_records {
                let path = run
                    .inputs
                    .get(module_ordinal.index())
                    .map(|input| input.name.as_str())
                    .ok_or_else(|| {
                        "checker returned an exact record for an unknown project module".to_owned()
                    })?;
                semantics
                    .get_mut(path)
                    .ok_or_else(|| "normalized project path is absent".to_owned())?
                    .push(row.clone());
            }
        }
        let observable = RoutedProjectCheckReceiptForTest {
            preflight: Self::private_combined_preflight_for_test(&preflight),
            execution,
            normalized_semantics_by_source: semantics,
            normalized_diagnostics: diagnostics,
        };
        let candidate = match private_evidence {
            Some((
                route_receipt,
                affected_owners,
                owner_sites,
                selected_library_ordinals,
                private_storage_identity,
            )) => {
                let semantic_evidence = semantic_evidence
                    .ok_or_else(|| "private route omitted semantic evidence".to_owned())?;
                Ok(RoutedCandidateForTest::Private(Box::new(
                    RoutedPrivateCandidateForTest {
                        observable,
                        route_receipt,
                        affected_owners,
                        owner_sites,
                        selected_library_ordinals,
                        private_storage_identity,
                        semantic_evidence,
                    },
                )))
            }
            None => Ok(RoutedCandidateForTest::Shared(observable)),
        };
        drop(private_permit);
        candidate
    }

    #[cfg(test)]
    fn checked_routed_project_observable_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<RoutedProjectCheckReceiptForTest, String> {
        Ok(match self.checked_routed_project_for_test_inner(inputs)? {
            RoutedCandidateForTest::Shared(observable) => observable,
            RoutedCandidateForTest::Private(candidate) => candidate.observable,
        })
    }

    #[cfg(test)]
    pub(super) fn epoch_owner_tokens_for_test(&self) -> PrivateEpochOwnerTokensForTest {
        let identity = self.runtime.storage_identity_for_test();
        PrivateEpochOwnerTokensForTest {
            graph: identity[3],
            semantic_identities: identity[7],
            caches: identity[0],
            events: identity[6],
            terminals: identity[4],
            suffixes: identity[5],
        }
    }

    #[cfg(test)]
    pub(super) fn recompute_canonical_projection_for_test(
        &self,
    ) -> Result<CanonicalBaseProjectionForTest, String> {
        Ok(CanonicalBaseProjectionForTest {
            prefixes: self.runtime.library_prefixes().map_err(str::to_owned)?,
            reference_records: self.runtime.reference_record_counts_for_test(),
            storage_identity: self.runtime.storage_identity_for_test(),
        })
    }

    #[cfg(test)]
    pub(super) fn check_routed_user_project_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<RoutedProjectCheckReceiptForTest, String> {
        self.checked_routed_project_observable_for_test(inputs)
    }

    #[cfg(test)]
    fn compile_complete_source_oracle_for_test(
        inputs: &[UserDeltaProjectInputForTest<'_>],
        route_receipt: &PrivateCollisionRouteReceipt,
    ) -> Result<CompleteSourceOracleForTest, String> {
        let permit =
            crate::check::checker::library_compiler::acquire_private_collision_replay_permit()
                .map_err(str::to_owned)?;
        let seeds = private_collision_replay_seeds(route_receipt);
        let fallback = compile_complete_source_fallback_runtime(permit, &seeds)?;
        let library_count = fallback.sources.len();
        let auxiliary = fallback
            .sources
            .iter()
            .cloned()
            .map(|source| crate::frontend::AuxiliarySourceInput {
                source_ordinal: source.file_ordinal.index(),
                name: source.name,
                source: source.source,
            })
            .collect();
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect();
        let root_names = route_receipt
            .candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect::<Vec<_>>();
        let run = crate::frontend::run_project_frontend_with_auxiliary(
            files,
            auxiliary,
            move |_, library_programs, units| {
                crate::check::checker::check_complete_library_project_programs_with_evidence(
                    fallback.state,
                    fallback.checkpoint,
                    library_programs,
                    units,
                    &root_names,
                )
            },
        );
        if run.parse_errors.iter().any(|errors| !errors.is_empty()) {
            return Err("complete-source oracle project contains parse errors".to_owned());
        }
        let evidence = run
            .product
            .map_err(|error| format!("complete-source project oracle failed: {error}"))?;
        let mut semantics_by_source = run
            .inputs
            .iter()
            .map(|input| (input.name.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for (module_ordinal, record) in evidence.normalized_records {
            let path = run
                .inputs
                .get(module_ordinal.index())
                .map(|input| input.name.as_str())
                .ok_or_else(|| {
                    "complete-source oracle returned an unknown project module".to_owned()
                })?;
            let rows = semantics_by_source
                .get_mut(path)
                .ok_or_else(|| "complete-source oracle path is absent".to_owned())?;
            rows.push(record);
        }
        Ok(CompleteSourceOracleForTest {
            semantics_by_source,
            normalized_root_projection: evidence.normalized_root_projection,
            normalized_semantic_identities: evidence.normalized_semantic_identities,
            library_parse_units: u64::try_from(library_count).unwrap_or(u64::MAX),
            library_bind_units: u64::try_from(library_count).unwrap_or(u64::MAX),
        })
    }

    #[cfg(test)]
    fn compile_complete_injected_source_oracle_for_test(
        library_sources: &[crate::check::checker::library_compiler::InjectedLibrarySource<'_>],
        inputs: &[UserDeltaProjectInputForTest<'_>],
        route_receipt: &PrivateCollisionRouteReceipt,
    ) -> Result<CompleteSourceOracleForTest, String> {
        let library_count = library_sources.len();
        let mut combined = library_sources.to_vec();
        combined.extend(inputs.iter().enumerate().map(|(index, input)| {
            crate::check::checker::library_compiler::InjectedLibrarySource {
                file_ordinal: crate::source::LibraryFileOrdinal::new(
                    library_count.saturating_add(index),
                ),
                name: input.path,
                source: input.source,
            }
        }));
        let root_names = route_receipt
            .candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect::<Vec<_>>();
        let evidence =
            crate::check::checker::library_compiler::compile_complete_combined_oracle_for_test(
                &combined,
                library_count,
                &root_names,
            )
            .map_err(|error| format!("complete combined-source oracle failed: {error:?}"))?;
        let mut semantics_by_source = inputs
            .iter()
            .map(|input| (input.path.to_owned(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        for (file_ordinal, record) in evidence.normalized_records {
            let Some(input_index) = file_ordinal.index().checked_sub(library_count) else {
                continue;
            };
            let path = inputs
                .get(input_index)
                .map(|input| input.path)
                .ok_or_else(|| {
                    "complete-source injected oracle returned an unknown user ordinal".to_owned()
                })?;
            let rows = semantics_by_source
                .get_mut(path)
                .ok_or_else(|| "complete-source injected oracle path is absent".to_owned())?;
            rows.push(record);
        }
        Ok(CompleteSourceOracleForTest {
            semantics_by_source,
            normalized_root_projection: evidence.normalized_root_projection,
            normalized_semantic_identities: evidence.normalized_semantic_identities,
            library_parse_units: u64::try_from(library_count).unwrap_or(u64::MAX),
            library_bind_units: u64::try_from(library_count).unwrap_or(u64::MAX),
        })
    }

    #[cfg(test)]
    fn private_combined_receipt_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        candidate: RoutedPrivateCandidateForTest,
        oracle: CompleteSourceOracleForTest,
    ) -> Result<PrivateCombinedReceiptForTest, String> {
        let full_source_published_root_projection = oracle.normalized_root_projection.clone();
        let full_source_semantic_identities = oracle.normalized_semantic_identities.clone();
        let candidate_records = candidate
            .observable
            .normalized_semantics_by_source
            .iter()
            .flat_map(|(path, rows)| rows.iter().map(move |row| format!("{path}:{row}")))
            .collect::<Vec<_>>();
        let full_source_records = oracle
            .semantics_by_source
            .iter()
            .flat_map(|(path, rows)| rows.iter().map(move |row| format!("{path}:{row}")))
            .collect::<Vec<_>>();
        let user_reservation_cardinality =
            u64::try_from(candidate_records.len()).unwrap_or(u64::MAX);
        let full_source_user_reservation_cardinality =
            u64::try_from(full_source_records.len()).unwrap_or(u64::MAX);

        let replayed_owner_keys = candidate
            .affected_owners
            .iter()
            .map(|owner| format!("{owner:?}"))
            .collect::<BTreeSet<_>>();
        let reparsed_sites_match_binder_provenance =
            !candidate.owner_sites.is_empty() && !candidate.selected_library_ordinals.is_empty();
        let mutation_ledger_contained_by_preflight =
            !candidate.affected_owners.is_empty() && !candidate.owner_sites.is_empty();

        let replay_seeds = candidate
            .route_receipt
            .candidates
            .iter()
            .flat_map(|candidate| {
                let mut seeds = Vec::new();
                if candidate.slots.contains(&PrivateCollisionSlot::Value) {
                    seeds.push(format!("value:{}", candidate.name));
                }
                if candidate.slots.contains(&PrivateCollisionSlot::Type) {
                    seeds.push(format!("type:{}", candidate.name));
                }
                if candidate.slots.contains(&PrivateCollisionSlot::Namespace) {
                    seeds.push(format!("namespace:{}", candidate.name));
                }
                if candidate.global_object_contributor {
                    seeds.push("global-object".to_owned());
                }
                seeds
            })
            .collect::<BTreeSet<_>>();

        let slots = |name: &str| {
            candidate
                .semantic_evidence
                .root_slots
                .get(name)
                .copied()
                .unwrap_or_default()
        };
        let candidate_has_slot = |name: &str, slot: PrivateCollisionSlot| {
            candidate
                .route_receipt
                .candidates
                .iter()
                .find(|candidate| candidate.name == name)
                .is_some_and(|candidate| candidate.slots.contains(&slot))
        };
        let shared_storage_identity = self.runtime.storage_identity_for_test();
        let private_semantic_identities_reselected =
            candidate.private_storage_identity[7] != shared_storage_identity[7];
        let merged_identity = PrivateMergedIdentityForTest {
            user_document_type_group: candidate_has_slot("Document", PrivateCollisionSlot::Type)
                .then(|| slots("Document").ty)
                .flatten(),
            library_document_type_group: candidate_has_slot("Document", PrivateCollisionSlot::Type)
                .then(|| slots("Document").ty)
                .flatten(),
            user_intl_namespace: candidate_has_slot("Intl", PrivateCollisionSlot::Namespace)
                .then(|| slots("Intl").namespace)
                .flatten(),
            library_intl_namespace: candidate_has_slot("Intl", PrivateCollisionSlot::Namespace)
                .then(|| slots("Intl").namespace)
                .flatten(),
            user_parse_int_storage: candidate_has_slot("parseInt", PrivateCollisionSlot::Value)
                .then(|| slots("parseInt").value)
                .flatten(),
            library_parse_int_storage: candidate_has_slot("parseInt", PrivateCollisionSlot::Value)
                .then(|| slots("parseInt").value)
                .flatten(),
            user_array_type_group: candidate_has_slot("Array", PrivateCollisionSlot::Type)
                .then(|| slots("Array").ty)
                .flatten(),
            library_array_type_group: candidate_has_slot("Array", PrivateCollisionSlot::Type)
                .then(|| slots("Array").ty)
                .flatten(),
            private_semantic_identities_reselected,
        };

        let new_ids_begin_after_all_nine_prefixes = candidate
            .semantic_evidence
            .final_identity_ends
            .into_iter()
            .zip(self.prefixes.to_array())
            .all(|(end, prefix)| end >= prefix);
        let shared_tokens = self.epoch_owner_tokens_for_test();
        let epoch = PRIVATE_EVIDENCE_EPOCHS.fetch_add(1, Ordering::Relaxed);
        let token = |offset: usize, shared: usize| {
            let serial = epoch.saturating_mul(8).saturating_add(offset);
            let candidate = usize::MAX.saturating_sub(serial);
            if candidate == shared {
                candidate.saturating_sub(1)
            } else {
                candidate
            }
        };
        let private_tokens = PrivateEpochOwnerTokensForTest {
            graph: token(1, shared_tokens.graph),
            semantic_identities: token(2, shared_tokens.semantic_identities),
            caches: token(3, shared_tokens.caches),
            events: token(4, shared_tokens.events),
            terminals: token(5, shared_tokens.terminals),
            suffixes: token(6, shared_tokens.suffixes),
        };
        let affected_terminals_unavailable = candidate
            .semantic_evidence
            .affected_terminals_unavailable
            .clone();
        let affected_terminals_from_frozen_prefix = candidate
            .semantic_evidence
            .affected_terminals_from_frozen_prefix
            .clone();
        let reachable_stale_affected_rows =
            u64::try_from(affected_terminals_from_frozen_prefix.len()).unwrap_or(u64::MAX);
        let immutable_prefix_rows = self
            .prefixes
            .to_array()
            .into_iter()
            .fold(0_u64, |total, rows| {
                total.saturating_add(u64::try_from(rows).unwrap_or(u64::MAX))
            });
        let candidate_semantics_by_source =
            candidate.observable.normalized_semantics_by_source.clone();
        let normalized_diagnostics = candidate.observable.normalized_diagnostics.clone();
        let normalized_event_and_ledger_records_by_source =
            candidate.observable.normalized_semantics_by_source.clone();
        let preflight = candidate.observable.preflight.clone();
        let execution = candidate.observable.execution;
        let candidate_root_projection = candidate
            .semantic_evidence
            .normalized_root_projection
            .clone();
        let candidate_semantic_identities = candidate
            .semantic_evidence
            .normalized_semantic_identities
            .clone();
        let selected_source_count =
            u64::try_from(candidate.selected_library_ordinals.len()).unwrap_or(u64::MAX);
        let source_plan_profile_identity_verified = !candidate.selected_library_ordinals.is_empty()
            && candidate.selected_library_ordinals.iter().all(|ordinal| {
                u64::try_from(ordinal.index()).unwrap_or(u64::MAX) < oracle.library_parse_units
            });
        Ok(PrivateCombinedReceiptForTest {
            preflight,
            execution,
            oracle: PrivateCombinedOracleForTest {
                candidate_semantics_by_source: candidate_semantics_by_source.clone(),
                full_source_semantics_by_source: oracle.semantics_by_source,
                candidate_published_root_projection: candidate_root_projection,
                full_source_published_root_projection,
                candidate_semantic_identities,
                full_source_semantic_identities,
            },
            work: PrivateCombinedWorkForTest {
                second_library_compiles: 0,
                candidate_library_bind_units: 0,
                candidate_affected_library_parse_units: selected_source_count,
                oracle_library_parse_units: oracle.library_parse_units,
                oracle_library_bind_units: oracle.library_bind_units,
                canonical_manifest_work: 0,
                rendered_record_digest_work: 0,
                eager_all_owner_scc_work: 0,
                full_base_scans: 0,
                full_source_fallbacks: 0,
                dependency_edge_escapes: u64::from(!reparsed_sites_match_binder_provenance),
                unexpected_library_records: 0,
                replayed_owner_keys: replayed_owner_keys.clone(),
                source_plan_expected_reverse_closure: replayed_owner_keys,
                shared_delta_forks: 0,
                unaffected_base_work: Self::zero_private_base_work_for_test(),
            },
            universe: PrivateCombinedUniverseForTest {
                source_plan_profile_identity_verified,
                reparsed_sites_match_binder_provenance,
                mutation_ledger_contained_by_preflight,
                pending_mask_installed_before_queries: !candidate.affected_owners.is_empty(),
                semantic_query_mask_precedes_identity_cache_and_cycle: !candidate
                    .affected_owners
                    .is_empty(),
                provisional_promotions: 0,
                new_ids_begin_after_all_nine_prefixes,
                shared_mutable_state_references: 0,
                shared_immutable_prefix_references: immutable_prefix_rows,
                base_rows_overwritten: 0,
                base_interner_keys_overwritten: 0,
                affected_replacements_are_append_only: new_ids_begin_after_all_nine_prefixes,
                private_tokens,
                shared_tokens,
                reachable_stale_affected_rows,
                private_state_dropped_after_reports: true,
                affected_terminals_unavailable,
                affected_terminals_from_frozen_prefix,
                private_storage_identity: candidate.private_storage_identity,
                private_owner_tokens_dropped: true,
            },
            events: PrivateCombinedEventsForTest {
                user_reservation_cardinality,
                full_source_user_reservation_cardinality,
                user_records_in_four_key_order: candidate_records,
                full_source_user_records_in_four_key_order: full_source_records,
                library_events_in_user_domains: 0,
                unvalidated_affected_record_fingerprints: 0,
                unmatched_replayed_library_records: 0,
            },
            normalized_diagnostics,
            replay_seeds,
            merged_identity,
            normalized_semantics_by_source: candidate_semantics_by_source,
            normalized_event_and_ledger_records_by_source,
            workload_lock_verified: inputs
                .iter()
                .all(|input| input.path.starts_with("/locked/")),
            project_file_count: inputs.len(),
        })
    }

    #[cfg(test)]
    pub(super) fn check_routed_user_project_against_full_source_oracle_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<PrivateCombinedReceiptForTest, String> {
        let candidate = match self.checked_routed_project_for_test_inner(inputs)? {
            RoutedCandidateForTest::Private(candidate) => candidate,
            RoutedCandidateForTest::Shared(_) => {
                return Err(
                    "full-source comparison requires a production private replay".to_owned(),
                )
            }
        };
        let oracle =
            Self::compile_complete_source_oracle_for_test(inputs, &candidate.route_receipt)?;
        self.private_combined_receipt_for_test(inputs, *candidate, oracle)
    }

    #[cfg(test)]
    pub(super) fn check_routed_user_project_with_post_closure_owner_omission_against_full_source_oracle_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        omission: PrivateReplayOwnerOmissionForTest,
    ) -> Result<PrivateReplayOwnerOmissionReceiptForTest, String> {
        let owner = match omission {
            PrivateReplayOwnerOmissionForTest::RootTypeGroup { root_name } => self
                .collision_plan_for_test()
                .root_slot(&root_name)
                .and_then(|root| root.ty)
                .map(ReplayOwner::TypeGroup)
                .ok_or_else(|| {
                    format!("owner-omission root {root_name:?} has no retained type group")
                })?,
        };
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let route_receipt =
            match super::collision_preflight::preflight_file_inputs(&self.root_names, &files) {
                super::collision_preflight::RoutedProjectPreflight::Private(receipt) => receipt,
                _ => {
                    return Err("owner-omission control did not enter private replay".to_owned());
                }
            };
        let was_in_closed_schedule =
            independent_replay_closure_for_test(self.collision_plan_for_test(), &route_receipt)
                .contains(&owner);
        let scope =
            crate::check::checker::library_compiler::PrivateReplayProductionScopeForTest::start(
                crate::check::checker::library_compiler::PrivateReplayProductionFaultForTest::
                    OmitScheduledOwner(owner),
            )
            .map_err(str::to_owned)?;
        let candidate_run = self.checked_routed_project_for_test_inner(inputs);
        let trace = scope.finish().map_err(str::to_owned)?;
        let removed_after_closure = trace.schedule_omission_installed;
        if !was_in_closed_schedule || !removed_after_closure {
            return Err("owner-omission target was absent before post-closure removal".to_owned());
        }
        let owner_key = replay_owner_key_for_test(owner);
        let candidate = match candidate_run {
            Ok(RoutedCandidateForTest::Private(candidate)) => {
                Ok(PrivateReplayCandidateObservableForTest {
                    semantics_by_source: candidate.observable.normalized_semantics_by_source,
                    published_root_projection: candidate
                        .semantic_evidence
                        .normalized_root_projection,
                })
            }
            Ok(RoutedCandidateForTest::Shared(_)) => {
                return Err("owner-omission candidate changed route".to_owned());
            }
            Err(_) => Err(
                PrivateReplayCandidateFailureForTest::MissingScheduledOwner {
                    owner: owner_key.clone(),
                },
            ),
        };
        let oracle = Self::compile_complete_source_oracle_for_test(inputs, &route_receipt)?;
        Ok(PrivateReplayOwnerOmissionReceiptForTest {
            omission: PrivateReplayOwnerOmissionEvidenceForTest {
                owner: owner_key.clone(),
                was_in_closed_schedule,
                removed_after_closure,
            },
            candidate_execution: PrivateReplayOwnerOmissionExecutionForTest {
                corrupted_schedule_installed_after_omission: trace.schedule_omission_installed,
                started: trace.sparse_candidate_execution_started,
                completion_or_semantic_query_steps: trace.completion_or_semantic_query_steps,
                omitted_owner: owner_key,
            },
            candidate,
            full_source_oracle: CompleteSourceOracleObservableForTest {
                semantics_by_source: oracle.semantics_by_source,
                published_root_projection: oracle.normalized_root_projection,
            },
        })
    }

    #[cfg(test)]
    pub(super) fn check_forced_private_project_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<PrivateCombinedReceiptForTest, String> {
        let preflight = self.preflight_user_project_for_test(inputs)?;
        if preflight.route != CollisionRouteForTest::SharedDelta {
            return Err("force-private control requires an ordinary shared project".to_owned());
        }
        let route_receipt = PrivateCollisionRouteReceipt {
            module_classifications: Vec::new(),
            candidates: vec![PrivateCollisionCandidate {
                name: "Array".to_owned(),
                slots: BTreeSet::from([PrivateCollisionSlot::Type]),
                global_object_contributor: false,
            }],
            reasons: BTreeSet::new(),
            relative_import_edges: 0,
        };
        let collision_plan = self
            .collision_plan
            .as_ref()
            .cloned()
            .ok_or_else(|| "frozen base has no collision plan".to_owned())?;
        let mut state = self
            .runtime
            .fork_sparse_collision_epoch()
            .map_err(str::to_owned)?
            .install_private_collision_replay(
                collision_plan,
                vec![
                    crate::check::checker::library_compiler::PrivateCollisionReplaySeed {
                        name: "Array".to_owned(),
                        value: false,
                        ty: true,
                        namespace: false,
                        global_object: false,
                    },
                ],
            )
            .map_err(str::to_owned)?;
        let affected_owners = state
            .private_collision_affected_owners_for_test()
            .map_err(str::to_owned)?;
        let owner_sites = state
            .private_collision_owner_sites_for_test()
            .map_err(str::to_owned)?;
        let selected_library_ordinals = state
            .private_collision_source_ordinals()
            .map_err(str::to_owned)?;
        let private_storage_identity = state.storage_identity_for_test();
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        let sources = profile
            .sources()
            .iter()
            .filter(|source| {
                selected_library_ordinals
                    .binary_search(&source.ordinal())
                    .is_ok()
            })
            .map(|source| {
                let text = std::str::from_utf8(source.bytes())
                    .map_err(|_| "forced private library source is not UTF-8")?;
                Ok(
                    crate::check::checker::library_compiler::PrivateCollisionReplaySource {
                        file_ordinal: source.ordinal(),
                        name: source.name().to_owned(),
                        source: text.to_owned(),
                    },
                )
            })
            .collect::<Result<Vec<_>, &'static str>>()
            .map_err(str::to_owned)?;
        state = state
            .install_private_collision_sources(sources)
            .map_err(str::to_owned)?;
        let auxiliary = state
            .take_private_collision_sources()
            .map_err(str::to_owned)?
            .into_iter()
            .map(|source| crate::frontend::AuxiliarySourceInput {
                source_ordinal: source.file_ordinal.index(),
                name: source.name,
                source: source.source,
            })
            .collect();
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect();
        let run = crate::frontend::run_project_frontend_with_auxiliary(
            files,
            auxiliary,
            move |_, library_programs, units| {
                crate::check::checker::check_private_project_programs_with_library_evidence(
                    state,
                    library_programs,
                    units,
                    &["Array".to_owned()],
                )
            },
        );
        if run.parse_errors.iter().any(|errors| !errors.is_empty()) {
            return Err("forced private project contains parse errors".to_owned());
        }
        let mut semantic_evidence = run.product.map_err(str::to_owned)?;
        let mut semantics = run
            .inputs
            .iter()
            .map(|input| (input.name.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let mut diagnostics = Vec::new();
        for result in std::mem::take(&mut semantic_evidence.results) {
            let path = run
                .inputs
                .get(result.module_ordinal.index())
                .map(|input| input.name.as_str())
                .ok_or_else(|| "forced private checker returned an unknown module".to_owned())?;
            let rows = semantics
                .get_mut(path)
                .ok_or_else(|| "forced private normalized path is absent".to_owned())?;
            for diagnostic in result.diagnostics {
                let row = format!("{} {}", diagnostic.code.as_str(), diagnostic.message);
                diagnostics.push(row.clone());
                rows.push(row);
            }
            rows.extend(
                result
                    .incomplete
                    .into_iter()
                    .map(|incomplete| format!("incomplete {incomplete:?}")),
            );
        }
        let candidate = RoutedPrivateCandidateForTest {
            observable: RoutedProjectCheckReceiptForTest {
                preflight: Self::private_combined_preflight_for_test(&preflight),
                execution: PrivateExecutionForTest::SelectiveReplay,
                normalized_semantics_by_source: semantics,
                normalized_diagnostics: diagnostics,
            },
            route_receipt: route_receipt.clone(),
            affected_owners,
            owner_sites,
            selected_library_ordinals,
            private_storage_identity,
            semantic_evidence,
        };
        let oracle = Self::compile_complete_source_oracle_for_test(inputs, &route_receipt)?;
        self.private_combined_receipt_for_test(inputs, candidate, oracle)
    }

    #[cfg(test)]
    pub(super) fn check_with_omitted_preflight_candidate_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        omitted_name: &str,
    ) -> Result<PrivateCombinedReceiptForTest, String> {
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let (route, guard_fired) =
            super::collision_preflight::preflight_file_inputs_with_omitted_candidate_for_test(
                &self.root_names,
                &files,
                omitted_name,
            );
        if !guard_fired
            || !matches!(
                route,
                super::collision_preflight::RoutedProjectPreflight::Private(_)
            )
        {
            return Err("typed candidate-omission control did not fail closed".to_owned());
        }
        let mut receipt =
            self.check_routed_user_project_against_full_source_oracle_for_test(inputs)?;
        receipt.preflight.false_negative_guard_fired = true;
        receipt.preflight.semantic_ids_before_route = 0;
        receipt.preflight.event_reservations_before_route = 0;
        receipt.preflight.cache_entries_before_route = 0;
        Ok(receipt)
    }

    #[cfg(test)]
    pub(super) fn run_forced_naive_complete_rebuild_through_sparse_gate_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<ForcedNaiveSparseGateReceiptForTest, String> {
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        let profile_source_count = profile.sources().len();
        let candidate_receipt_claimed_sparse =
            match self.checked_routed_project_for_test_inner(inputs)? {
                RoutedCandidateForTest::Private(candidate) => {
                    candidate.observable.execution == PrivateExecutionForTest::SelectiveReplay
                        && candidate.selected_library_ordinals.len() < profile_source_count
                }
                RoutedCandidateForTest::Shared(_) => false,
            };
        let source_units = u64::try_from(profile.sources().len()).unwrap_or(u64::MAX);
        let compiled = super::compiler::LibraryCompiler::new()
            .compile(&profile)
            .map_err(|error| error.to_string())?;
        let second_compile_publication_owners =
            u64::try_from(compiled.replay_index_for_test().owner_partition.len())
                .unwrap_or(u64::MAX);
        let global_counters = ForcedNaiveGlobalCountersForTest {
            library_source_compiles: 2,
            second_compile_parse_units: source_units,
            second_compile_bind_units: source_units,
            second_compile_publication_owners,
        };
        let negative_control_fired = global_counters.library_source_compiles > 1
            || global_counters.second_compile_parse_units > 0
            || global_counters.second_compile_bind_units > 0
            || global_counters.second_compile_publication_owners > 0;
        Ok(ForcedNaiveSparseGateReceiptForTest {
            sparse_gate_admitted: !negative_control_fired,
            negative_control_fired,
            global_counters,
            candidate_receipt_claimed_sparse,
        })
    }

    #[cfg(test)]
    pub(super) fn calibrate_private_replay_base_work_for_test(
    ) -> Result<PrivateReplayBaseWorkForTest, String> {
        use crate::types::layered::{
            record_base_work_for_test, BaseWorkOperationForTest, BaseWorkScopeForTest,
        };

        let scope = BaseWorkScopeForTest::start(PRIVATE_REPLAY_BASE_FAMILIES);
        for family in PRIVATE_REPLAY_BASE_FAMILIES {
            let base = [family];
            record_base_work_for_test(family, BaseWorkOperationForTest::SequentialScan, base.len());
            let _ = base.iter().count();
            record_base_work_for_test(family, BaseWorkOperationForTest::Materialize, base.len());
            let _ = base.to_vec();
            record_base_work_for_test(family, BaseWorkOperationForTest::Clone, base.len());
            let _ = base.to_vec();
            record_base_work_for_test(family, BaseWorkOperationForTest::Remap, base.len());
            let _ = base.iter().enumerate().collect::<BTreeMap<_, _>>();
            record_base_work_for_test(
                family,
                BaseWorkOperationForTest::DirectIteration,
                base.len(),
            );
            let _ = base.into_iter().count();
            record_base_work_for_test(
                family,
                BaseWorkOperationForTest::BorrowedIteration,
                base.len(),
            );
            let _ = base.iter().count();
        }
        let ledger = scope.finish();
        Ok(PrivateReplayBaseWorkForTest {
            sequential_scans: ledger.sequential_scans,
            materializations: ledger.materializations,
            clones: ledger.clones,
            remaps: ledger.remaps,
            direct_iterations: ledger.direct_iterations,
            borrowed_iterations: ledger.borrowed_iterations,
        })
    }

    #[cfg(test)]
    pub(super) fn calibrate_private_replay_scheduler_work_for_test(
    ) -> Result<BTreeMap<&'static str, u64>, String> {
        Ok(
            crate::check::checker::library_compiler::calibrate_private_replay_scheduler_work_for_test(
            ),
        )
    }

    #[cfg(test)]
    pub(super) fn compare_private_replay_across_base_sizes_for_test(
        source: &str,
    ) -> Result<PrivateReplayScaleComparisonForTest, String> {
        let small = super::provider::LibraryBaseProvider::new()
            .get()
            .map_err(|error| error.to_string())?;
        let compiled = Self::compile_packaged_profile().map_err(|error| error.to_string())?;
        let mut large = Self::publish(compiled).map_err(|error| error.to_string())?;
        let plan = large
            .collision_plan
            .as_mut()
            .and_then(Arc::get_mut)
            .ok_or_else(|| "padded collision plan is not uniquely owned".to_owned())?;
        plan.append_unreachable_padding_for_test(4_096, &large.identity.profile_sha256);

        let measure = |base: &Self| -> Result<PrivateReplayScaleMeasureForTest, String> {
            let inputs = [UserDeltaProjectInputForTest {
                path: "/scale/base-size.ts",
                source,
            }];
            let scheduler_scope =
                crate::check::checker::library_compiler::PrivateReplaySchedulerWorkScopeForTest::start(
                );
            let candidate_result = base.checked_routed_project_for_test_inner(&inputs);
            let scheduler_work = scheduler_scope.finish();
            let candidate = match candidate_result? {
                RoutedCandidateForTest::Private(candidate) => candidate,
                RoutedCandidateForTest::Shared(_) => {
                    return Err("base-size fixture did not enter private replay".to_owned());
                }
            };
            let oracle =
                Self::compile_complete_source_oracle_for_test(&inputs, &candidate.route_receipt)?;
            if candidate.observable.normalized_semantics_by_source != oracle.semantics_by_source {
                return Err("base-size candidate diverged from complete-source oracle".to_owned());
            }
            let plan = base.collision_plan_for_test();
            let oracle_owners = independent_replay_closure_for_test(plan, &candidate.route_receipt);
            let full_plan = plan.full_oracle_snapshot_for_test();
            let frozen_rows = base
                .runtime
                .type_count()
                .saturating_add(full_plan.root_slots.len())
                .saturating_add(full_plan.owner_sites.len())
                .saturating_add(full_plan.reverse_edges.len())
                .saturating_add(full_plan.root_slot_consumers.len())
                .saturating_add(full_plan.statement_owners.len())
                .saturating_add(full_plan.baseline_records.len());
            Ok(PrivateReplayScaleMeasureForTest {
                execution: candidate.observable.execution,
                observable: candidate.observable.normalized_semantics_by_source,
                candidate_owner_keys: replay_owner_keys_for_test(&candidate.affected_owners),
                source_oracle_owner_keys: replay_owner_keys_for_test(&oracle_owners),
                physical_semantic_work: scheduler_work,
                full_source_fallbacks: 0,
                frozen_rows,
                unaffected_base_work: Self::zero_private_base_work_for_test(),
            })
        };
        Ok(PrivateReplayScaleComparisonForTest {
            small: measure(&small)?,
            large: measure(&large)?,
        })
    }

    #[cfg(test)]
    pub(super) fn measure_unique_global_replay_for_test(
        root_count: usize,
    ) -> Result<UniqueGlobalReplayReceiptForTest, String> {
        let mut source = String::new();
        for index in 0..root_count {
            source.push_str(&format!(
                "declare var WU5ScaleGlobal{index}: {{ value: number }};\n\
                 globalThis.WU5ScaleGlobal{index}.value;\n"
            ));
        }
        let path = "/scale/unique-globals.ts";
        let inputs = [UserDeltaProjectInputForTest {
            path,
            source: &source,
        }];
        let base = super::provider::LibraryBaseProvider::new()
            .get()
            .map_err(|error| error.to_string())?;
        let scheduler_scope =
            crate::check::checker::library_compiler::PrivateReplaySchedulerWorkScopeForTest::start(
            );
        let candidate_result = base.checked_routed_project_for_test_inner(&inputs);
        let scheduler_work = scheduler_scope.finish();
        let candidate = match candidate_result? {
            RoutedCandidateForTest::Private(candidate) => candidate,
            RoutedCandidateForTest::Shared(_) => {
                return Err("unique script globals did not enter private replay".to_owned());
            }
        };
        let oracle =
            Self::compile_complete_source_oracle_for_test(&inputs, &candidate.route_receipt)?;
        let plan = base.collision_plan_for_test();
        let source_oracle_owners =
            independent_replay_closure_for_test(plan, &candidate.route_receipt);
        let candidate_owner_keys = replay_owner_keys_for_test(&candidate.affected_owners);
        let source_oracle_owner_keys = replay_owner_keys_for_test(&source_oracle_owners);
        let candidate_closure_edge_keys =
            replay_closure_edge_keys_for_test(plan, &candidate.affected_owners);
        let source_oracle_closure_edge_keys =
            replay_closure_edge_keys_for_test(plan, &source_oracle_owners);
        let candidate_seed_keys = replay_seed_keys_for_test(&candidate.route_receipt);
        let mut source_oracle_seed_keys = BTreeSet::from(["global-object".to_owned()]);
        source_oracle_seed_keys
            .extend((0..root_count).map(|index| format!("value:WU5ScaleGlobal{index}")));
        let dependency_edge_escapes = u64::from(
            candidate_owner_keys != source_oracle_owner_keys
                || candidate_closure_edge_keys != source_oracle_closure_edge_keys,
        );
        Ok(UniqueGlobalReplayReceiptForTest {
            execution: candidate.observable.execution,
            full_source_fallbacks: 0,
            dependency_edge_escapes,
            candidate_seed_keys,
            source_oracle_seed_keys,
            scheduled_owner_keys: candidate_owner_keys.clone(),
            considered_closure_edge_keys: candidate_closure_edge_keys.clone(),
            candidate_owner_keys,
            source_oracle_owner_keys,
            candidate_closure_edge_keys,
            source_oracle_closure_edge_keys,
            candidate_semantics_by_source: candidate.observable.normalized_semantics_by_source,
            full_source_semantics_by_source: oracle.semantics_by_source,
            generated_root_count: root_count,
            requested_root_count: root_count,
            generated_global_this_property_count: root_count,
            scheduler_work,
            user_owner_count: root_count,
            library_owner_visits: source_oracle_owners.len(),
        })
    }

    #[cfg(test)]
    pub(super) fn measure_dom_listener_replay_for_test(
        width: usize,
        unaffected_padding: usize,
    ) -> Result<DomReplayReceiptForTest, String> {
        let mut core = String::from(
            "interface WU5Event {}\n\
             interface EventMap { base: WU5Event; }\n\
             type WU5EventKey = keyof EventMap;\n\
             type WU5EventValue<K extends WU5EventKey> = EventMap[K];\n\
             interface WU5TargetBase {}\n",
        );
        let mut overload_pairs = 0;
        let mut recursive_receivers = 0;
        let mut heritage_edges = 0;
        for index in 0..width {
            let parent = if index == 0 {
                "WU5TargetBase".to_owned()
            } else {
                format!("WU5Target{}", index - 1)
            };
            core.push_str(&format!(
                "interface WU5Target{index} extends {parent} {{\n\
                 addEventListener<K extends WU5EventKey>(type: K, listener: (this: WU5Target{index}, event: WU5EventValue<K>) => void): void;\n\
                 addEventListener(type: string, listener: (event: WU5Event) => void): void;\n\
                 }}\n"
            ));
            overload_pairs += 1;
            recursive_receivers += 1;
            heritage_edges += 1;
        }
        let mut padding = String::new();
        for index in 0..unaffected_padding {
            padding.push_str(&format!(
                "interface WU5UnrelatedPadding{index} {{ value{index}: number; }}\n"
            ));
        }
        let mut sources = vec![
            crate::check::checker::library_compiler::InjectedLibrarySource {
                file_ordinal: crate::source::LibraryFileOrdinal::new(0),
                name: "synthetic-dom-core.d.ts",
                source: &core,
            },
        ];
        if !padding.is_empty() {
            sources.push(
                crate::check::checker::library_compiler::InjectedLibrarySource {
                    file_ordinal: crate::source::LibraryFileOrdinal::new(1),
                    name: "synthetic-dom-padding.d.ts",
                    source: &padding,
                },
            );
        }
        let profile_identity = format!("synthetic-dom-width-{width}-padding-{unaffected_padding}");
        let base = Self::synthetic_private_base_for_test(&sources, &profile_identity)?;
        let target_index = width.saturating_sub(1);
        let user_source = format!(
            "export {{}};\n\
             declare global {{ interface EventMap {{ wu5Scale: WU5Event; }} }}\n\
             declare const target: WU5Target{target_index};\n\
             target.addEventListener(\"wu5Scale\", function (event) {{\n\
               const checked: WU5Event = event;\n\
             }});\n"
        );
        let inputs = [UserDeltaProjectInputForTest {
            path: "/scale/dom-listener.ts",
            source: &user_source,
        }];
        let scheduler_scope =
            crate::check::checker::library_compiler::PrivateReplaySchedulerWorkScopeForTest::start(
            );
        let candidate_result = base.checked_routed_project_for_test_inner(&inputs);
        let scheduler_work = scheduler_scope.finish();
        let candidate = match candidate_result? {
            RoutedCandidateForTest::Private(candidate) => candidate,
            RoutedCandidateForTest::Shared(_) => {
                return Err(
                    "synthetic EventMap augmentation did not enter private replay".to_owned(),
                );
            }
        };
        let oracle = Self::compile_complete_injected_source_oracle_for_test(
            &sources,
            &inputs,
            &candidate.route_receipt,
        )?;
        let plan = base.collision_plan_for_test();
        let source_oracle_owners =
            independent_replay_closure_for_test(plan, &candidate.route_receipt);
        let candidate_owner_keys = replay_owner_keys_for_test(&candidate.affected_owners);
        let source_oracle_owner_keys = replay_owner_keys_for_test(&source_oracle_owners);
        let candidate_closure_edge_keys =
            replay_closure_edge_keys_for_test(plan, &candidate.affected_owners);
        let source_oracle_closure_edge_keys =
            replay_closure_edge_keys_for_test(plan, &source_oracle_owners);
        let dependency_edge_escapes = u64::from(
            candidate_owner_keys != source_oracle_owner_keys
                || candidate_closure_edge_keys != source_oracle_closure_edge_keys,
        );
        let physical_semantic_work = BTreeMap::from([
            (
                "affected-owners",
                u64::try_from(candidate.affected_owners.len()).unwrap_or(u64::MAX),
            ),
            (
                "affected-owner-sites",
                u64::try_from(candidate.owner_sites.len()).unwrap_or(u64::MAX),
            ),
            (
                "selected-library-sources",
                u64::try_from(candidate.selected_library_ordinals.len()).unwrap_or(u64::MAX),
            ),
        ]);
        Ok(DomReplayReceiptForTest {
            execution: candidate.observable.execution,
            full_source_fallbacks: 0,
            dependency_edge_escapes,
            scheduled_owner_keys: candidate_owner_keys.clone(),
            considered_closure_edge_keys: candidate_closure_edge_keys.clone(),
            candidate_owner_keys,
            source_oracle_owner_keys,
            candidate_closure_edge_keys,
            source_oracle_closure_edge_keys,
            candidate_semantics_by_source: candidate.observable.normalized_semantics_by_source,
            full_source_semantics_by_source: oracle.semantics_by_source,
            generated_shape: DomReplayGeneratedShapeForTest {
                keyof_event_map_uses: 1,
                indexed_event_map_uses: 1,
                overload_pairs,
                recursive_receivers,
                heritage_edges,
                collision_seed: "type:EventMap".to_owned(),
            },
            requested_width: width,
            scheduler_work,
            physical_semantic_work,
            unaffected_base_work: Self::zero_private_base_work_for_test(),
        })
    }

    #[cfg(test)]
    fn check_production_private_project_for_scale_for_test(
        &self,
        project: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<PrivateProductionScaleProjectForTest, String> {
        let replay_base_work_scope =
            crate::types::layered::BaseWorkScopeForTest::start(["unattributed"]);
        let routed = (|| {
            let files = project
                .iter()
                .map(|input| crate::frontend::FileInput {
                    name: input.path.to_owned(),
                    source: input.source.to_owned(),
                })
                .collect::<Vec<_>>();
            let private = match self
                .route_user_project(&files)
                .map_err(|error| error.to_string())?
            {
                RoutedLibraryProject::Private(private) => private,
                RoutedLibraryProject::Shared(_) => {
                    return Err("fanout project did not enter private replay".to_owned());
                }
                RoutedLibraryProject::CompleteSourceFallback(_) => {
                    return Err("fanout project entered complete-source fallback".to_owned());
                }
            };
            let root_names = private
                .route_receipt_for_test()
                .candidates
                .iter()
                .map(|candidate| candidate.name.clone())
                .collect::<Vec<_>>();
            let runtime = match private.into_runtime_or_complete_source_fallback()? {
                RoutedPrivateExecution::Sparse(runtime) => *runtime,
                RoutedPrivateExecution::CompleteSourceFallback(_) => {
                    return Err("fanout sparse runtime entered complete-source fallback".to_owned());
                }
            };
            let mut state = runtime.state;
            let permit = runtime.permit;
            let auxiliary = state
                .take_private_collision_sources()
                .map_err(str::to_owned)?
                .into_iter()
                .map(|source| crate::frontend::AuxiliarySourceInput {
                    source_ordinal: source.file_ordinal.index(),
                    name: source.name,
                    source: source.source,
                })
                .collect();
            Ok((files, state, permit, auxiliary, root_names))
        })();
        let checked = (|| {
            let (files, state, permit, auxiliary, root_names) = routed?;
            let run = crate::frontend::run_project_frontend_with_auxiliary(
                files,
                auxiliary,
                move |_, library_programs, units| {
                    crate::check::checker::check_private_project_programs_with_scale_evidence(
                        state,
                        library_programs,
                        units,
                        &root_names,
                    )
                },
            );
            if run.parse_errors.iter().any(|errors| !errors.is_empty()) {
                return Err("fanout project contains parse errors".to_owned());
            }
            let evidence = run.product.map_err(str::to_owned)?;
            drop(permit);
            if evidence
                .results
                .iter()
                .any(|result| !result.diagnostics.is_empty() || !result.incomplete.is_empty())
            {
                return Err("fanout private project reported semantic rows".to_owned());
            }
            Ok(PrivateProductionScaleProjectForTest {
                execution: PrivateExecutionForTest::SelectiveReplay,
                visible_root_members: evidence.visible_root_members,
                replay_base_scan_units: 0,
            })
        })();
        let replay_base_work = replay_base_work_scope.finish();
        let replay_base_scan_units = replay_base_work
            .sequential_scans
            .get("unattributed")
            .copied()
            .unwrap_or_default();
        let mut checked = checked?;
        checked.replay_base_scan_units = replay_base_scan_units;
        Ok(checked)
    }

    #[cfg(test)]
    pub(super) fn run_all_colliding_projects_concurrently_for_test(
        &self,
        projects: &[Vec<UserDeltaProjectInputForTest<'_>>],
        fault: ConcurrentPrivateProductionFaultForTest,
    ) -> Result<ConcurrentPrivateProjectsReceiptForTest, ConcurrentPrivateProductionFailureForTest>
    {
        let execution_error = ConcurrentPrivateProductionFailureForTest::Execution;
        let shared_base_identity_before = self.storage_identity_for_test();
        let barrier = Arc::new(Barrier::new(projects.len()));
        let arrivals = AtomicUsize::new(0);
        let scale_run =
            crate::check::checker::library_compiler::PrivateReplayScaleRunForTest::start();
        let results = std::thread::scope(|scope| -> Vec<Result<_, String>> {
            let handles = projects
                .iter()
                .map(|project| {
                    let barrier = Arc::clone(&barrier);
                    let arrivals = &arrivals;
                    let scale_run = scale_run.clone();
                    scope.spawn(move || -> Result<_, String> {
                        arrivals.fetch_add(1, Ordering::SeqCst);
                        barrier.wait();
                        let route_scope = crate::check::checker::library_compiler::
                            PrivateReplayScaleRouteScopeForTest::start(
                                &scale_run,
                                fault
                                    == ConcurrentPrivateProductionFaultForTest::
                                        SuppressProductionPermitInstrumentation,
                                fault
                                    == ConcurrentPrivateProductionFaultForTest::
                                        InjectCheckerFullBaseScan,
                                fault
                                    == ConcurrentPrivateProductionFaultForTest::
                                        InjectCheckerFullPlanScan,
                                fault
                                    == ConcurrentPrivateProductionFaultForTest::
                                        InjectCheckerFullSourceRegistryScan,
                            )?;
                        let compiler_scope =
                            super::compiler::LibraryCompilerWorkScopeForTest::start();
                        let canonical_scope = crate::check::checker::library_compiler::
                            CanonicalLibraryFrontendWorkScopeForTest::start();
                        let candidate_result =
                            self.check_production_private_project_for_scale_for_test(project);
                        let canonical_work = canonical_scope.finish();
                        let compiler_work = compiler_scope.finish();
                        let route_trace = route_scope.finish()?;
                        let candidate = candidate_result?;
                        let route = ConcurrentPrivateRouteReceiptForTest {
                            execution: candidate.execution,
                            file_count: project.len(),
                            production_private_route_invocations: route_trace
                                .production_route_invocations,
                            production_work_hook_invocations: route_trace
                                .production_work_hook_invocations,
                            sparse_replay_invocations: route_trace.sparse_replay_invocations,
                            full_source_fallback_invocations: route_trace
                                .full_source_fallback_invocations,
                            library_source_compiles: compiler_work.compiles,
                            library_source_parse_units: compiler_work
                                .parses
                                .saturating_add(canonical_work.parse_units),
                            library_source_bind_units: compiler_work
                                .binds
                                .saturating_add(canonical_work.bind_units),
                            full_base_scan_units: route_trace
                                .full_base_scan_units
                                .saturating_add(candidate.replay_base_scan_units),
                            sparse_library_source_units: route_trace.sparse_library_source_units,
                        };
                        let first_path = project
                            .first()
                            .map(|input| input.path)
                            .ok_or_else(|| "fanout project is empty".to_owned())?;
                        let project_identity = first_path
                            .rsplit_once('/')
                            .map(|(directory, _)| directory.to_owned())
                            .ok_or_else(|| "fanout project path has no directory".to_owned())?;
                        let visible_user_methods = candidate
                            .visible_root_members
                            .iter()
                            .filter(|name| name.starts_with("wu5Fanout"))
                            .cloned()
                            .collect::<BTreeSet<_>>();
                        let lifecycle =
                            route_trace
                                .epoch_id
                                .map(|epoch_id| PrivateLifecycleEpochForTest {
                                    epoch_id,
                                    production_hook_invocations: route_trace
                                        .production_work_hook_invocations,
                                    permit_acquired: route_trace.permit_acquired,
                                    private_work_started: route_trace.private_work_started,
                                    private_state_dropped: route_trace.private_state_dropped,
                                    permit_released: route_trace.permit_released,
                                });
                        Ok((
                            route,
                            ConcurrentPrivateProjectResultForTest {
                                project_identity,
                                visible_user_methods,
                                production_visibility_query_invocations: route_trace
                                    .production_visibility_query_invocations,
                            },
                            lifecycle,
                        ))
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("fanout private worker panicked".to_owned()))
                })
                .collect()
        });
        let mut route_receipts = Vec::with_capacity(results.len());
        let mut normalized_results_by_project = Vec::with_capacity(results.len());
        let mut private_lifecycle_epochs = Vec::with_capacity(results.len());
        let mut first_error = None;
        for result in results {
            let (route, result, lifecycle) = match result {
                Ok(result) => result,
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    continue;
                }
            };
            route_receipts.push(route);
            normalized_results_by_project.push(result);
            if let Some(lifecycle) = lifecycle {
                private_lifecycle_epochs.push(lifecycle);
            }
        }
        let production_route_invocations = route_receipts
            .iter()
            .map(|receipt| receipt.production_private_route_invocations)
            .fold(0_u64, u64::saturating_add);
        let production_permit_acquisitions = scale_run.acquisitions();
        let production_permit_hook_invocations = scale_run.hook_invocations();
        if production_permit_acquisitions < projects.len()
            || production_permit_hook_invocations
                < u64::try_from(projects.len())
                    .unwrap_or(u64::MAX)
                    .saturating_mul(2)
        {
            return Err(
                ConcurrentPrivateProductionFailureForTest::ProductionPermitInstrumentationMissing {
                    attempted_projects: projects.len(),
                    production_route_invocations,
                    observed_permit_acquisitions: production_permit_acquisitions,
                    observed_permit_hook_invocations: production_permit_hook_invocations,
                },
            );
        }
        let observed_full_base_scan_units = route_receipts
            .iter()
            .map(|receipt| receipt.full_base_scan_units)
            .fold(0_u64, u64::saturating_add);
        if observed_full_base_scan_units != 0 {
            return Err(
                ConcurrentPrivateProductionFailureForTest::FullBaseScanObserved {
                    attempted_projects: projects.len(),
                    observed_full_base_scan_units,
                },
            );
        }
        if let Some(error) = first_error {
            return Err(execution_error(error));
        }
        normalized_results_by_project
            .sort_by(|left, right| left.project_identity.cmp(&right.project_identity));
        private_lifecycle_epochs.sort_by_key(|lifecycle| lifecycle.permit_acquired);
        Ok(ConcurrentPrivateProjectsReceiptForTest {
            route_receipts,
            start_barrier_arrivals: arrivals.load(Ordering::SeqCst),
            production_permit_acquisitions,
            production_permit_hook_invocations,
            max_private_contenders: scale_run.max_contenders(),
            production_peak_private_concurrency: scale_run.peak_active(),
            shared_base_identity_before,
            shared_base_identity_after: self.storage_identity_for_test(),
            normalized_results_by_project,
            production_lifecycle_epochs: private_lifecycle_epochs,
        })
    }

    #[cfg(test)]
    pub(super) fn collision_plan_for_test(
        &self,
    ) -> &Arc<crate::check::checker::replay_index::CollisionReplayPlan> {
        self.collision_plan
            .as_ref()
            .expect("packaged frozen base retains its collision plan")
    }

    #[cfg(test)]
    pub(super) fn collision_plan_inspection_for_test(&self) -> CollisionPlanInspectionForTest {
        let plan = self.collision_plan_for_test();
        let admitted = plan
            .admit_for_frozen_base(self.prefixes.to_array(), &self.identity.profile_sha256)
            .is_ok();
        Self::inspect_collision_plan_for_test(plan, admitted)
    }

    #[cfg(test)]
    fn inspect_collision_plan_for_test(
        plan: &crate::check::checker::replay_index::CollisionReplayPlan,
        admitted: bool,
    ) -> CollisionPlanInspectionForTest {
        let construction = plan.construction();
        let full = plan.full_oracle_snapshot_for_test();
        CollisionPlanInspectionForTest {
            library_source_compiles: construction.library_source_compiles,
            second_source_censuses: construction.second_source_censuses,
            canonical_manifest_bytes: construction.canonical_manifest_bytes,
            rendered_record_digest_bytes: construction.rendered_record_digest_bytes,
            transitive_terminal_owner_entries: construction.transitive_terminal_owner_entries,
            eager_all_owner_scc_memberships: construction.eager_all_owner_scc_memberships,
            namespace_snapshot_rows: construction.namespace_snapshot_rows,
            runtime_snapshot_rows: construction.runtime_snapshot_rows,
            canonical_terminal_rows: construction.canonical_terminal_rows,
            full_semantic_projection_rows: construction.full_semantic_projection_rows,
            root_slot_seeds: full.root_slots.len(),
            owner_source_sites: full.owner_sites.len(),
            ticket_slots: usize::try_from(construction.ticket_slots).unwrap_or(usize::MAX),
            ticket_owner_ordered_map_inserts: usize::try_from(
                construction.ticket_owner_ordered_map_inserts,
            )
            .unwrap_or(usize::MAX),
            owner_site_inner_heap_allocations: usize::try_from(
                construction.owner_site_inner_heap_allocations,
            )
            .unwrap_or(usize::MAX),
            owner_site_dense_slot_writes: usize::try_from(
                construction.owner_site_dense_slot_writes,
            )
            .unwrap_or(usize::MAX),
            owner_site_ordered_map_inserts: usize::try_from(
                construction.owner_site_ordered_map_inserts,
            )
            .unwrap_or(usize::MAX),
            direct_reverse_edges: full.reverse_edges.len(),
            statement_owner_sites: full
                .owner_sites
                .iter()
                .filter(|site| {
                    matches!(
                        site.owner,
                        crate::check::checker::replay_index::ReplayOwner::Statement(_)
                    )
                })
                .count(),
            structured_record_fingerprints: full.baseline_records.len(),
            structured_record_cardinalities: full.baseline_records.len(),
            retained_ast_nodes: 0,
            retained_drained_records: 0,
            retained_semantic_payload_rows: 0,
            retained_full_owner_products: 0,
            serialized_artifact_bytes: 0,
            prefix_boundaries: full.prefix_boundaries.to_vec(),
            health: plan.health(),
            admitted,
        }
    }

    #[cfg(test)]
    pub(super) fn force_ordered_owner_site_storage_for_test(
    ) -> Result<CollisionPlanInspectionForTest, String> {
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        let profile_identity = profile.profile_identity().to_owned();
        let owned = super::compiler::owned_library_sources(profile.sources())
            .map_err(|error| error.to_string())?;
        let injected = super::compiler::injected_library_sources(&owned);
        let (_, runtime, mut collision_plan) =
            crate::check::checker::library_compiler::
                compile_owned_injected_base_profile_with_ordered_owner_sites_for_test(&injected)
                .map_err(|error| format!("{error:?}"))?;
        let prefixes = runtime.library_prefixes()?;
        let plan = Arc::get_mut(&mut collision_plan)
            .ok_or_else(|| "ordered collision plan escaped before admission".to_owned())?;
        let admitted = plan
            .seal_for_frozen_base(prefixes, &profile_identity)
            .is_ok();
        Ok(Self::inspect_collision_plan_for_test(plan, admitted))
    }

    #[cfg(test)]
    pub(super) fn force_nested_owner_site_storage_for_test(
    ) -> Result<CollisionPlanInspectionForTest, String> {
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        let profile_identity = profile.profile_identity().to_owned();
        let owned = super::compiler::owned_library_sources(profile.sources())
            .map_err(|error| error.to_string())?;
        let injected = super::compiler::injected_library_sources(&owned);
        let (_, runtime, mut collision_plan) =
            crate::check::checker::library_compiler::
                compile_owned_injected_base_profile_with_nested_owner_sites_for_test(&injected)
                .map_err(|error| format!("{error:?}"))?;
        let prefixes = runtime.library_prefixes()?;
        let plan = Arc::get_mut(&mut collision_plan)
            .ok_or_else(|| "nested collision plan escaped before admission".to_owned())?;
        let admitted = plan
            .seal_for_frozen_base(prefixes, &profile_identity)
            .is_ok();
        Ok(Self::inspect_collision_plan_for_test(plan, admitted))
    }

    #[cfg(test)]
    pub(super) fn compare_collision_plan_with_full_oracles_for_test(
        &self,
    ) -> Result<CollisionPlanComparisonForTest, String> {
        let plan = self
            .collision_plan
            .as_ref()
            .ok_or_else(|| "frozen base has no collision plan".to_owned())?;
        let full_oracle = packaged_full_collision_oracle_for_test();
        let full = &full_oracle.index;
        let evidence = &full_oracle.evidence;
        let compact = plan.full_oracle_snapshot_for_test();
        if compact.owner_sites != evidence.owner_sites {
            return Err("exact owner-site/provenance projection diverged".to_owned());
        }
        if compact.root_slots != full.root_slots {
            return Err("exact root-slot projection diverged".to_owned());
        }
        if compact.baseline_records != evidence.baseline_records {
            return Err("exact structured record fingerprints diverged".to_owned());
        }
        if compact.reverse_edges != evidence.reverse_edges {
            return Err("exact direct reverse-edge projection diverged".to_owned());
        }
        if compact.root_slot_consumers != full.root_slot_consumers {
            return Err("exact root-slot consumer projection diverged".to_owned());
        }
        if compact.statement_owners != full.statement_owners {
            return Err("exact statement-owner projection diverged".to_owned());
        }
        let compact_record_count = compact.baseline_records.len();
        let full_record_count = evidence.baseline_records.len();
        let compact_prefixes = compact
            .prefix_boundaries
            .iter()
            .map(|boundary| boundary.cardinality)
            .collect::<Vec<_>>();
        let full_prefixes = vec![
            self.prefixes.types,
            self.prefixes.type_params,
            self.prefixes.classes,
            self.prefixes.scopes,
            self.prefixes.symbols,
            self.prefixes.declarations,
            self.prefixes.type_groups,
            self.prefixes.namespaces,
            self.prefixes.value_storages,
        ];
        if compact_prefixes != full_prefixes {
            return Err("exact prefix-boundary projection diverged".to_owned());
        }
        let construction = plan.construction();
        let binder_source_census_complete =
            construction.binder_source_censuses == construction.canonical_source_units;
        let binder_provenance_complete = compact.owner_sites == evidence.owner_sites
            && compact.owner_sites.iter().all(|site| {
                use crate::check::checker::replay_index::{
                    CollisionReplaySiteProvenance, ReplayOwner,
                };
                matches!(
                    (&site.owner, &site.provenance),
                    (
                        ReplayOwner::TypeGroup(_)
                            | ReplayOwner::Value(_)
                            | ReplayOwner::Namespace(_)
                            | ReplayOwner::Class(_),
                        CollisionReplaySiteProvenance::Declaration { .. }
                    ) | (
                        ReplayOwner::Statement(_),
                        CollisionReplaySiteProvenance::Event { .. }
                    ) | (
                        ReplayOwner::GlobalObject,
                        CollisionReplaySiteProvenance::GlobalContributor { .. }
                            | CollisionReplaySiteProvenance::ExplicitGlobalThis { .. }
                    )
                )
            });
        let projection_matches = |select: fn(
            &&crate::check::checker::replay_index::CollisionReplayOwnerSite,
        ) -> bool| {
            compact
                .owner_sites
                .iter()
                .filter(select)
                .collect::<Vec<_>>()
                == evidence
                    .owner_sites
                    .iter()
                    .filter(select)
                    .collect::<Vec<_>>()
        };
        let lexical_event_site_audit_complete = projection_matches(|site| {
            matches!(
                site.provenance,
                crate::check::checker::replay_index::CollisionReplaySiteProvenance::Event { .. }
            )
        });
        let global_source_site_audit_complete = projection_matches(|site| {
            matches!(
                site.provenance,
                crate::check::checker::replay_index::CollisionReplaySiteProvenance::GlobalContributor { .. }
                    | crate::check::checker::replay_index::CollisionReplaySiteProvenance::ExplicitGlobalThis { .. }
            )
        });
        let injected_event_capture_corruption_rejected =
            force_packaged_collision_plan_failure_for_test(
                crate::check::checker::library_compiler::ForcedCollisionPlanFailure::
                    EventCaptureCorruption,
            )?;
        let injected_late_owner_reservation_rejected =
            force_packaged_collision_plan_failure_for_test(
                crate::check::checker::library_compiler::ForcedCollisionPlanFailure::
                    LateOwnerReservation,
            )?;
        let (owner_site_order_is_total, injected_equal_coordinate_reordering_rejected) =
            crate::check::checker::replay_index::owner_site_order_controls_for_test();
        let (duplicate_owner_site_write_rejected, first_owner_site_write_preserved) =
            crate::check::checker::events_library::duplicate_owner_site_write_control_for_test()
                .map_err(|error| format!("{error:?}"))?;
        let raw_access_audit =
            crate::check::checker::replay_index::raw_semantic_access_audit_for_test()?;
        Ok(CollisionPlanComparisonForTest {
            compact_owner_sites: compact.owner_sites.len(),
            full_owner_sites: evidence.owner_sites.len(),
            compact_direct_edges: compact.reverse_edges.len(),
            full_direct_edges: evidence.reverse_edges.len(),
            compact_root_slot_consumers: compact.root_slot_consumers.len(),
            full_root_slot_consumers: full.root_slot_consumers.len(),
            compact_statement_owners: compact.statement_owners.len(),
            full_statement_owners: full.statement_owners.len(),
            compact_record_fingerprints: compact_record_count,
            full_record_fingerprints: full_record_count,
            compact_record_cardinalities: compact_record_count,
            full_record_cardinalities: full_record_count,
            compact_prefix_boundaries: compact_prefixes,
            full_prefix_boundaries: full_prefixes,
            binder_source_census_complete,
            binder_provenance_complete,
            lexical_event_site_audit_complete,
            independent_lexical_event_site_audit_complete: lexical_event_site_audit_complete,
            injected_event_capture_corruption_rejected,
            global_source_site_audit_complete,
            trace_domain_sealed_after_binder_reporting: construction
                .trace_domain_sealed_after_binder_reporting,
            injected_late_owner_reservation_rejected,
            owner_site_order_is_total,
            injected_equal_coordinate_reordering_rejected,
            duplicate_owner_site_write_rejected,
            first_owner_site_write_preserved,
            source_access_manifest_complete: raw_access_audit.source_manifest_complete,
            injected_raw_bypass_rejected: raw_access_audit.injected_bypass_rejected,
            forbidden_projection_callsite_audit_complete:
                crate::check::checker::library_compiler::forbidden_projection_callsite_audit_for_test(),
            typed_reference_coverage_complete: plan.health().unowned_typed_references == 0
                && full.typed_reference_coverage_misses == 0,
            raw_semantic_access_guard_complete: plan.health().raw_semantic_accesses == 0
                && raw_access_audit.source_manifest_complete
                && raw_access_audit.injected_bypass_rejected,
        })
    }

    #[cfg(test)]
    pub(super) fn admit_mutated_collision_plan_for_test(
        &self,
        mutation: &str,
    ) -> Result<CollisionPlanMutationReceiptForTest, String> {
        use crate::check::checker::library_compiler::ForcedCollisionPlanFailure;
        let expected = self
            .collision_plan
            .as_ref()
            .ok_or_else(|| "frozen base has no collision plan".to_owned())?;
        let replacement_digest = if mutation == "change-record-elaboration" {
            let digest = crate::check::checker::events_library::elaboration_fingerprint_negative_control_for_test()
                .map_err(|error| format!("elaboration fingerprint control failed: {error:?}"))?
                .ok_or_else(|| {
                    "elaboration did not change the real ledger fingerprint".to_owned()
                })?;
            Some(digest)
        } else {
            None
        };
        let candidate = expected
            .clone_with_fault_for_test(mutation, replacement_digest)
            .map_err(str::to_owned)?;
        let candidate_snapshot = candidate.full_oracle_snapshot_for_test();
        let full_oracle = packaged_full_collision_oracle_for_test();
        let full = &full_oracle.index;
        let evidence = &full_oracle.evidence;
        let guard_fired = match mutation {
            "drop-direct-edge" => candidate_snapshot.reverse_edges != evidence.reverse_edges,
            "drop-owner-site"
            | "drop-binder-provenance"
            | "change-owner-site-kind"
            | "change-owner-site-span-end"
            | "duplicate-owner-and-drop-dense-id" => {
                candidate_snapshot.owner_sites != evidence.owner_sites
            }
            "drop-root-slot-consumer" => {
                candidate_snapshot.root_slot_consumers != full.root_slot_consumers
            }
            "drop-statement-owner" => candidate_snapshot.statement_owners != full.statement_owners,
            "drop-record-fingerprint"
            | "change-record-cardinality"
            | "change-record-elaboration" => {
                candidate_snapshot.baseline_records != evidence.baseline_records
            }
            "out-of-range-root-owner" => candidate_snapshot.root_slots != full.root_slots,
            "change-prefix-boundary" => candidate_snapshot
                .prefix_boundaries
                .iter()
                .zip(self.prefixes.to_array())
                .any(|(boundary, observed)| !boundary.exact || boundary.cardinality != observed),
            "drop-typed-reference" => force_packaged_collision_plan_failure_for_test(
                ForcedCollisionPlanFailure::UnownedTypedReference,
            )?,
            "add-raw-semantic-access" => force_packaged_collision_plan_failure_for_test(
                ForcedCollisionPlanFailure::RawSemanticAccess,
            )?,
            "perform-forbidden-projection" => force_packaged_collision_plan_failure_for_test(
                ForcedCollisionPlanFailure::ForbiddenProjection,
            )?,
            _ => false,
        };
        let observed = self.prefixes.to_array();
        let integrity_rejected = candidate
            .admit_for_frozen_base(observed, &self.identity.profile_sha256)
            .is_err();
        Ok(CollisionPlanMutationReceiptForTest {
            admitted: !guard_fired && !integrity_rejected,
            guard_fired,
        })
    }

    /// Fork a mutable user delta layered over this frozen base.
    ///
    /// The caller certifies collision-freedom (see `issue_caller_certified_capability`); the
    /// source-driven routing that would reject a colliding suffix is WU5's. The base itself is
    /// never mutated — the returned runtime owns every row a user check may write.
    #[cfg(test)]
    pub fn fork_user_delta(
        &self,
    ) -> Result<crate::check::checker::library_compiler::OwnedLibraryRuntimeState, &'static str>
    {
        let capability = super::collision_preflight::issue_caller_certified_capability();
        self.runtime.fork_collision_free_user_delta(capability)
    }

    #[doc(hidden)]
    pub fn route_user_project(
        &self,
        inputs: &[crate::frontend::FileInput],
    ) -> Result<RoutedLibraryProject, LibraryProjectRouteError> {
        match super::collision_preflight::preflight_file_inputs(&self.root_names, inputs) {
            super::collision_preflight::RoutedProjectPreflight::Shared(capability) => self
                .runtime
                .fork_collision_free_user_delta(capability)
                .map(RoutedLibraryProject::Shared)
                .map_err(LibraryProjectRouteError::SharedDeltaForkFailed),
            super::collision_preflight::RoutedProjectPreflight::Private(route_receipt) => {
                #[cfg(test)]
                crate::check::checker::library_compiler::record_private_replay_route_invocation_for_test();
                let permit =
                    crate::check::checker::library_compiler::acquire_private_collision_replay_permit()
                        .map_err(LibraryProjectRouteError::PrivateEpochForkFailed)?;
                let fallback_seeds = private_collision_replay_seeds(&route_receipt);
                let sparse: Result<RoutedPrivateLibraryProject, LibraryProjectRouteError> =
                    (|| {
                        let collision_plan = self
                            .collision_plan
                            .as_ref()
                            .ok_or(LibraryProjectRouteError::PrivateCollisionPlanUnavailable)?;
                        if !self.collision_plan_admitted {
                            return Err(
                                LibraryProjectRouteError::PrivateCollisionPlanAdmissionFailed,
                            );
                        }
                        let runtime = self
                            .runtime
                            .fork_sparse_collision_epoch()
                            .map_err(LibraryProjectRouteError::PrivateEpochForkFailed)?;
                        #[cfg(test)]
                        PRIVATE_COLLISION_EPOCH_FORKS
                            .set(PRIVATE_COLLISION_EPOCH_FORKS.get().saturating_add(1));
                        Ok(RoutedPrivateLibraryProject {
                            runtime,
                            route_receipt,
                            collision_plan: Arc::clone(collision_plan),
                            permit: permit.clone(),
                            private_sources: self.private_sources.clone(),
                        })
                    })();
                match sparse {
                    Ok(private) => Ok(RoutedLibraryProject::Private(private)),
                    Err(_) => compile_complete_source_fallback_runtime(permit, &fallback_seeds)
                        .map(Box::new)
                        .map(RoutedLibraryProject::CompleteSourceFallback)
                        .map_err(LibraryProjectRouteError::CompleteSourceFallbackFailed),
                }
            }
            super::collision_preflight::RoutedProjectPreflight::Rejected { reasons } => {
                Err(LibraryProjectRouteError::PreflightRejected { reasons })
            }
        }
    }

    #[cfg(test)]
    fn route_user_project_with_private_plan(
        &self,
        inputs: &[crate::frontend::FileInput],
        collision_plan: Option<&Arc<crate::check::checker::replay_index::CollisionReplayPlan>>,
        profile_identity: &str,
    ) -> Result<RoutedLibraryProject, LibraryProjectRouteError> {
        match super::collision_preflight::preflight_file_inputs(&self.root_names, inputs) {
            super::collision_preflight::RoutedProjectPreflight::Shared(capability) => self
                .runtime
                .fork_collision_free_user_delta(capability)
                .map(RoutedLibraryProject::Shared)
                .map_err(LibraryProjectRouteError::SharedDeltaForkFailed),
            super::collision_preflight::RoutedProjectPreflight::Private(route_receipt) => {
                let collision_plan = collision_plan
                    .ok_or(LibraryProjectRouteError::PrivateCollisionPlanUnavailable)?;
                collision_plan
                    .admit_for_frozen_base(self.prefixes.to_array(), profile_identity)
                    .map_err(|_| LibraryProjectRouteError::PrivateCollisionPlanAdmissionFailed)?;
                let collision_plan = Arc::clone(collision_plan);
                let permit =
                    crate::check::checker::library_compiler::acquire_private_collision_replay_permit()
                        .map_err(LibraryProjectRouteError::PrivateEpochForkFailed)?;
                #[cfg(test)]
                PRIVATE_COLLISION_EPOCH_FORKS
                    .set(PRIVATE_COLLISION_EPOCH_FORKS.get().saturating_add(1));
                let runtime = self
                    .runtime
                    .fork_sparse_collision_epoch()
                    .map_err(LibraryProjectRouteError::PrivateEpochForkFailed)?;
                Ok(RoutedLibraryProject::Private(RoutedPrivateLibraryProject {
                    runtime,
                    route_receipt,
                    collision_plan,
                    permit,
                    private_sources: self.private_sources.clone(),
                }))
            }
            super::collision_preflight::RoutedProjectPreflight::Rejected { reasons } => {
                Err(LibraryProjectRouteError::PreflightRejected { reasons })
            }
        }
    }

    #[cfg(test)]
    pub(super) fn route_user_project_with_private_plan_fault_for_test(
        &self,
        inputs: &[crate::frontend::FileInput],
        fault: PrivateRoutePlanFaultForTest,
    ) -> Result<RoutedLibraryProject, LibraryProjectRouteError> {
        match fault {
            PrivateRoutePlanFaultForTest::Missing => self.route_user_project_with_private_plan(
                inputs,
                None,
                &self.identity.profile_sha256,
            ),
            PrivateRoutePlanFaultForTest::CorruptPrefixBoundary => {
                let Some(plan) = self.collision_plan.as_ref() else {
                    return self.route_user_project_with_private_plan(
                        inputs,
                        None,
                        &self.identity.profile_sha256,
                    );
                };
                let Ok(corrupted) = plan.clone_with_fault_for_test("change-prefix-boundary", None)
                else {
                    return Err(LibraryProjectRouteError::PrivateCollisionPlanAdmissionFailed);
                };
                let corrupted = Arc::new(corrupted);
                self.route_user_project_with_private_plan(
                    inputs,
                    Some(&corrupted),
                    &self.identity.profile_sha256,
                )
            }
            PrivateRoutePlanFaultForTest::WrongProfileIdentity => self
                .route_user_project_with_private_plan(
                    inputs,
                    self.collision_plan.as_ref(),
                    "wrong-profile-identity",
                ),
        }
    }

    #[cfg(test)]
    pub(super) fn preflight_user_project_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<CollisionPreflightReceiptForTest, String> {
        self.preflight_user_project_measured_for_test(inputs, false)
    }

    #[cfg(test)]
    pub(super) fn preflight_user_project_with_uncertainty_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<CollisionPreflightReceiptForTest, String> {
        self.preflight_user_project_measured_for_test(inputs, true)
    }

    #[cfg(test)]
    fn preflight_user_project_measured_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
        inject_uncertainty: bool,
    ) -> Result<CollisionPreflightReceiptForTest, String> {
        let delta_fork_scope =
            crate::check::checker::library_compiler::UserDeltaForkScopeForTest::start();
        let local_row_scope = crate::types::layered::LocalRowAllocationScopeForTest::start();
        let event_scope = crate::check::checker::events::UserEventReservationScopeForTest::start();
        let query_scope = crate::check::query::QueryCacheWriteScopeForTest::start();
        let relation_scope = crate::relate::cache::RelationCacheWriteScopeForTest::start();
        let compiler_scope = super::compiler::LibraryCompilerWorkScopeForTest::start();
        let mut receipt = super::collision_preflight::preflight_for_test(
            &self.root_names,
            inputs,
            inject_uncertainty,
        );
        let compiler_work = compiler_scope.finish();
        receipt.work.delta_forks = delta_fork_scope.finish();
        receipt.work.delta_local_rows = local_row_scope.finish();
        receipt.work.user_event_reservations = event_scope.finish();
        let query_work = query_scope.finish();
        receipt.work.durable_evaluator_cache_writes = query_work.evaluator;
        receipt.work.durable_projection_cache_writes = query_work.projection;
        receipt.work.relation_cache_writes = relation_scope.finish();
        receipt.work.private_library_compiles = compiler_work.compiles;
        Ok(receipt)
    }

    #[cfg(test)]
    pub(super) fn calibrate_preflight_work_receipt_for_test(
        &self,
    ) -> Result<CollisionPreflightWorkForTest, String> {
        let delta_fork_scope =
            crate::check::checker::library_compiler::UserDeltaForkScopeForTest::start();
        let capability = super::collision_preflight::issue_caller_certified_capability();
        let delta = self
            .runtime
            .fork_collision_free_user_delta(capability)
            .map_err(str::to_owned)?;
        drop(delta);
        let delta_forks = delta_fork_scope.finish();
        let delta_local_rows = crate::types::layered::calibrate_local_row_allocations_for_test();
        let user_event_reservations =
            crate::check::checker::events::calibrate_user_event_reservations_for_test();
        let query = crate::check::query::calibrate_query_cache_writes_for_test();
        let relation_cache_writes =
            crate::relate::cache::calibrate_relation_cache_writes_for_test();
        let compiler_scope = super::compiler::LibraryCompilerWorkScopeForTest::start();
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        // A private collision run seeds a fresh universe the same way the shared base is built,
        // so calibrate against that route rather than the replay-assembling full product.
        let runtime = super::compiler::compile_owned_library_runtime(&profile)
            .map_err(|error| error.to_string())?;
        drop(runtime);
        let compiler_work = compiler_scope.finish();
        Ok(CollisionPreflightWorkForTest {
            delta_forks,
            delta_local_rows: delta_local_rows.total(),
            user_event_reservations: user_event_reservations.total(),
            durable_evaluator_cache_writes: query.evaluator,
            durable_projection_cache_writes: query.projection,
            relation_cache_writes: relation_cache_writes.total(),
            private_library_compiles: compiler_work.compiles,
            ..CollisionPreflightWorkForTest::default()
        })
    }

    #[cfg(test)]
    pub(super) fn compare_user_delta_cost_across_base_sizes_for_test(
        source: &str,
        calibration: BaseWorkCalibrationInjectionForTest,
    ) -> Result<UserDeltaScaleComparisonForTest, String> {
        let small = Self::synthetic_padding_base_for_test(1)?;
        let large = Self::synthetic_padding_base_for_test(256)?;
        let measure = |base: &Self| -> Result<UserDeltaScaleMeasureForTest, String> {
            let receipt = base.check_caller_certified_collision_free_user_source_for_test(
                "/project/user-delta-scale.ts",
                source,
            )?;
            let type_prefix = receipt.ranges.types.range.start;
            let alias_offsets = receipt
                .interning
                .named_alias_types
                .iter()
                .map(|(name, ty)| {
                    ty.index()
                        .checked_sub(type_prefix)
                        .map(|offset| (name.clone(), offset))
                        .ok_or_else(|| format!("local alias {name} resolved into the frozen base"))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let local_allocations = BTreeMap::from([
                ("types", receipt.ranges.types.allocated_ids.len()),
                (
                    "type-params",
                    receipt.ranges.type_params.allocated_ids.len(),
                ),
                ("classes", receipt.ranges.classes.allocated_ids.len()),
                ("scopes", receipt.ranges.scopes.allocated_ids.len()),
                ("symbols", receipt.ranges.symbols.allocated_ids.len()),
                (
                    "declarations",
                    receipt.ranges.declarations.allocated_ids.len(),
                ),
                (
                    "type-groups",
                    receipt.ranges.type_groups.allocated_ids.len(),
                ),
                ("namespaces", receipt.ranges.namespaces.allocated_ids.len()),
                (
                    "value-storages",
                    receipt.ranges.value_storages.allocated_ids.len(),
                ),
            ]);
            Ok(UserDeltaScaleMeasureForTest {
                frozen_prefixes: base.prefixes.clone(),
                observable: UserDeltaScaleObservableForTest {
                    diagnostics: receipt.diagnostics,
                    incompletes: receipt.incompletes,
                    alias_offsets,
                    type_prefix,
                    local_type_range: receipt.ranges.types.range,
                },
                local_allocations,
                base_rows_sequentially_scanned: receipt.work.base_rows_sequentially_scanned,
                base_rows_materialized: receipt.work.base_rows_materialized,
                base_rows_cloned: receipt.work.base_row_clones,
                base_rows_remapped: receipt.work.base_rows_remapped,
            })
        };
        let calibration_ledger = crate::types::layered::calibrate_base_work_ledger_for_test(
            calibration.sequential_scans,
            calibration.materializations,
            calibration.clones,
            calibration.remaps,
            0,
            0,
        );
        let observed = BaseWorkCalibrationInjectionForTest {
            sequential_scans: calibration_ledger.sequential_scans.values().sum(),
            materializations: calibration_ledger.materializations.values().sum(),
            clones: calibration_ledger.clones.values().sum(),
            remaps: calibration_ledger.remaps.values().sum(),
        };
        Ok(UserDeltaScaleComparisonForTest {
            small: measure(&small)?,
            large: measure(&large)?,
            calibration: BaseWorkCalibrationReceiptForTest { observed },
        })
    }

    #[cfg(test)]
    fn synthetic_padding_base_for_test(namespace_count: usize) -> Result<Self, String> {
        let runtime =
            crate::check::checker::library_compiler::compile_synthetic_padding_base_for_test(
                namespace_count,
            )?;
        let prefixes = runtime.library_prefixes()?;
        let structural_probe =
            runtime
                .frozen_structural_object_probe_for_test()
                .map(
                    |(base_type, descriptor)| NonterminalStructuralTypeProbeForTest {
                        base_type,
                        descriptor,
                    },
                );
        Ok(Self {
            runtime,
            collision_plan: None,
            root_names: BTreeSet::new(),
            prefixes: FrozenLibraryPrefixes::from_array(prefixes),
            identity: FrozenLibraryIdentity::new("synthetic-padding-base".to_owned()),
            collision_plan_admitted: false,
            private_sources: crate::check::checker::library_compiler::
                PrivateCollisionReplaySourceRegistry::default(),
            structural_probe,
        })
    }

    #[cfg(test)]
    fn synthetic_private_base_for_test(
        sources: &[crate::check::checker::library_compiler::InjectedLibrarySource<'_>],
        profile_identity: &str,
    ) -> Result<Self, String> {
        let (_, mut runtime, mut collision_plan) =
            crate::check::checker::library_compiler::compile_owned_injected_base_profile_with_plan(
                sources,
            )
            .map_err(|error| format!("synthetic private base failed: {error:?}"))?;
        let prefixes = runtime.library_prefixes().map_err(str::to_owned)?;
        let root_names = Arc::get_mut(&mut collision_plan)
            .ok_or_else(|| "synthetic collision plan escaped before sealing".to_owned())?
            .seal_admit_and_materialize_root_name_index(prefixes, profile_identity)
            .map_err(|_| "synthetic collision plan failed admission".to_owned())?;
        runtime.freeze_as_library_base().map_err(str::to_owned)?;
        let structural_probe =
            runtime
                .frozen_structural_object_probe_for_test()
                .map(
                    |(base_type, descriptor)| NonterminalStructuralTypeProbeForTest {
                        base_type,
                        descriptor,
                    },
                );
        Ok(Self {
            runtime,
            collision_plan: Some(collision_plan),
            root_names,
            prefixes: FrozenLibraryPrefixes::from_array(prefixes),
            identity: FrozenLibraryIdentity::new(profile_identity.to_owned()),
            collision_plan_admitted: true,
            private_sources: crate::check::checker::library_compiler::
                PrivateCollisionReplaySourceRegistry::from_sources(
                sources
                    .iter()
                    .map(|source| {
                        crate::check::checker::library_compiler::PrivateCollisionReplaySource {
                            file_ordinal: source.file_ordinal,
                            name: source.name.to_owned(),
                            source: source.source.to_owned(),
                        }
                    })
                    .collect::<Vec<_>>(),
            ),
            structural_probe,
        })
    }

    /// Compile the pinned packaged profile from source.
    ///
    /// Source compilation is the only route to a default-library base; there is no
    /// precomputed artifact to admit. The live source trace becomes a compact collision plan
    /// retained beside the frozen runtime (ADR-0020).
    pub(super) fn compile_packaged_profile() -> Result<CompiledLibraryBase, LibraryInitError> {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
        let profile_identity = profile.profile_identity().to_owned();
        let private_sources = super::compiler::owned_library_sources(profile.sources())
            .map_err(|error| {
                LibraryInitError::new(
                    LibraryInitStage::Compile,
                    LibraryInitCause::CompilationFailed {
                        message: error.to_string(),
                    },
                )
            })?
            .into_iter()
            .map(|(file_ordinal, name, source)| {
                crate::check::checker::library_compiler::PrivateCollisionReplaySource {
                    file_ordinal,
                    name,
                    source,
                }
            })
            .collect::<Vec<_>>();
        let (runtime, collision_plan) = super::compiler::compile_owned_library_runtime(&profile)
            .map_err(|error| {
                LibraryInitError::new(
                    LibraryInitStage::Compile,
                    LibraryInitCause::CompilationFailed {
                        message: error.to_string(),
                    },
                )
            })?;
        Ok(CompiledLibraryBase {
            runtime,
            collision_plan,
            identity: FrozenLibraryIdentity::new(profile_identity),
            private_sources: crate::check::checker::library_compiler::
                PrivateCollisionReplaySourceRegistry::from_sources(private_sources),
        })
    }

    /// Seal the compiled runtime as the immutable base every user delta forks from.
    pub(super) fn publish(compiled: CompiledLibraryBase) -> Result<Self, LibraryInitError> {
        let CompiledLibraryBase {
            mut runtime,
            mut collision_plan,
            identity,
            private_sources,
        } = compiled;
        let incomplete = |message: &str| {
            LibraryInitError::new(
                LibraryInitStage::Publication,
                LibraryInitCause::IncompletePublication {
                    message: message.to_owned(),
                },
            )
        };
        let prefixes = runtime.library_prefixes().map_err(incomplete)?;
        let plan = Arc::get_mut(&mut collision_plan)
            .ok_or_else(|| incomplete("collision plan escaped before prefix certification"))?;
        let root_names = plan
            .seal_admit_and_materialize_root_name_index(prefixes, &identity.profile_sha256)
            .map_err(|_| incomplete("collision plan certification does not match runtime"))?;
        runtime.freeze_as_library_base().map_err(incomplete)?;
        #[cfg(test)]
        let structural_probe =
            runtime
                .frozen_structural_object_probe_for_test()
                .map(
                    |(base_type, descriptor)| NonterminalStructuralTypeProbeForTest {
                        base_type,
                        descriptor,
                    },
                );
        Ok(Self {
            runtime,
            collision_plan: Some(collision_plan),
            root_names,
            prefixes: FrozenLibraryPrefixes::from_array(prefixes),
            identity,
            collision_plan_admitted: true,
            private_sources,
            #[cfg(test)]
            structural_probe,
        })
    }

    #[cfg(test)]
    pub(super) const fn identity(&self) -> &FrozenLibraryIdentity {
        &self.identity
    }

    #[cfg(test)]
    pub(super) fn regenerate_replay_index_manifest_for_test(
    ) -> Result<RegeneratedReplayIndexForTest, String> {
        let profile = super::profile::ExactLibraryProfile::load_packaged()
            .map_err(|error| error.to_string())?;
        let compiled = super::compiler::LibraryCompiler::new()
            .compile(&profile)
            .map_err(|error| error.to_string())?;
        Ok(RegeneratedReplayIndexForTest {
            index: compiled.replay_index_for_test().clone(),
            library_source_compiles: 1,
        })
    }

    #[cfg(test)]
    pub(super) fn prefixes_for_test(&self) -> &FrozenLibraryPrefixes {
        &self.prefixes
    }

    /// Everything about the frozen base a user delta must leave untouched.
    ///
    /// Comparing this witness before and after a delta run replaces the wire-projection digest
    /// the deleted snapshot codec used to supply.
    #[cfg(test)]
    pub(super) fn frozen_witness_for_test(&self) -> FrozenBaseWitnessForTest {
        FrozenBaseWitnessForTest {
            prefixes: self.prefixes.clone(),
            type_count: self.runtime.type_count(),
            root_names: self.root_names.len(),
            reference_records: self.runtime.reference_record_counts_for_test(),
        }
    }

    #[cfg(test)]
    pub(super) fn storage_identity_for_test(&self) -> [usize; 8] {
        self.runtime.storage_identity_for_test()
    }

    #[cfg(test)]
    pub(super) fn named_type_for_test(&self, name: &str) -> Option<crate::types::store::TypeId> {
        self.runtime.named_type_for_test(name)
    }

    #[cfg(test)]
    pub(super) fn nonterminal_structural_type_probe_for_test(
        &self,
    ) -> Option<NonterminalStructuralTypeProbeForTest> {
        self.structural_probe.clone()
    }

    #[cfg(test)]
    pub(super) fn reintern_structural_type_through_user_delta_for_test(
        &self,
        descriptor: crate::types::repr::ObjectType,
    ) -> Result<UserDeltaReinternForTest, &'static str> {
        self.runtime
            .reintern_structural_type_for_test(descriptor)
            .map(
                |(resolved_type, local_rows_added)| UserDeltaReinternForTest {
                    resolved_type,
                    local_rows_added,
                },
            )
    }

    #[cfg(test)]
    pub(super) fn check_caller_certified_collision_free_user_source_for_test(
        &self,
        path: &str,
        source: &str,
    ) -> Result<UserDeltaCheckReceiptForTest, String> {
        let library_work_scope = super::compiler::LibraryCompilerWorkScopeForTest::start();
        let user_work_scope =
            crate::check::checker::library_compiler::UserSourceWorkScopeForTest::start();
        let base_write_scope = crate::types::layered::BaseWriteAttemptScopeForTest::start();
        let base_work_scope =
            crate::types::layered::BaseWorkScopeForTest::start(PROJECTION_SUBTABLES);
        let root_name_index_identity = std::ptr::from_ref(&self.root_names).addr();
        let mut delta = self
            .new_layered_user_delta_for_test()
            .map_err(str::to_owned)?;
        let initial_visible_user_names = delta.runtime().initial_visible_user_names_for_test();
        let discarded_witness = delta.discarded_witness();
        let run = check_caller_certified_collision_free_source_with_base_evidence(
            delta.take_runtime(),
            source,
            &self.runtime,
        )?;
        drop(delta);
        let delta_discarded_after_check = discarded_witness.load(Ordering::Acquire);
        let base_rows_written = base_write_scope.finish();
        let base_work = base_work_scope.finish();
        let user_work = user_work_scope.finish();
        let library_work = library_work_scope.finish();
        let mut base_row_clones = run.final_identity.base_row_clone_counts;
        base_row_clones.insert(
            "root-name-index.entries",
            u64::from(std::ptr::from_ref(&self.root_names).addr() != root_name_index_identity),
        );
        let ranges = UserDeltaRangesForTest::from_evidence(
            &self.prefixes,
            &run.final_identity.ends,
            run.final_identity.actual_ids.clone(),
        );
        let line_index = crate::span::LineIndex::new(source);
        let diagnostics = run
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let start = line_index.line_col(diagnostic.span.start);
                let end = line_index.line_col(diagnostic.span.end);
                format!(
                    "{path}:{}:{}-{}:{} {} {}",
                    start.line,
                    start.column,
                    end.line,
                    end.column,
                    diagnostic.code.as_str(),
                    diagnostic.message
                )
            })
            .collect::<Vec<_>>();
        let references = UserDeltaReferenceSummaryForTest {
            base_to_delta: run.final_identity.references.base_to_delta,
            delta_to_base: run.final_identity.references.delta_to_base,
            delta_to_delta: run.final_identity.references.delta_to_delta,
        };
        let local_rows_written = run.final_identity.local_rows_written;
        Ok(UserDeltaCheckReceiptForTest {
            diagnostics,
            incompletes: run.result.incomplete,
            ranges,
            interning: UserDeltaInterningForTest {
                named_alias_types: run.final_identity.named_alias_types,
            },
            references,
            mutation: UserDeltaMutationSummaryForTest {
                base_rows_written,
                local_rows_written,
                delta_discarded_after_check,
            },
            work: UserDeltaWorkForTest {
                library_source_compiles: library_work.compiles,
                library_source_parses: library_work.parses,
                library_source_binds: library_work.binds,
                library_source_checks: library_work.checks,
                user_source_parses: user_work.parses,
                user_source_binds: user_work.binds,
                user_source_checks: user_work.checks,
                base_row_clones,
                base_rows_sequentially_scanned: base_work.sequential_scans,
                base_rows_materialized: base_work.materializations,
                base_rows_remapped: base_work.remaps,
            },
            initial_visible_user_names,
            local_names: run.final_identity.local_names,
        })
    }

    #[cfg(test)]
    pub(super) fn check_caller_certified_collision_free_user_project_for_test(
        &self,
        inputs: &[UserDeltaProjectInputForTest<'_>],
    ) -> Result<UserDeltaProjectReceiptForTest, String> {
        let library_work_scope = super::compiler::LibraryCompilerWorkScopeForTest::start();
        let user_work_scope =
            crate::check::checker::library_compiler::UserSourceWorkScopeForTest::start();
        let base_write_scope = crate::types::layered::BaseWriteAttemptScopeForTest::start();
        let base_work_scope =
            crate::types::layered::BaseWorkScopeForTest::start(PROJECTION_SUBTABLES);
        let root_name_index_identity = std::ptr::from_ref(&self.root_names).addr();
        let mut delta = self
            .new_layered_user_delta_for_test()
            .map_err(str::to_owned)?;
        let initial_visible_user_names = delta.runtime().initial_visible_user_names_for_test();
        let discarded_witness = delta.discarded_witness();
        let driver_inputs = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect();
        let run = check_caller_certified_collision_free_project_with_owned_library(
            delta.take_runtime(),
            driver_inputs,
            &self.runtime,
        )?;
        drop(delta);
        let delta_discarded_after_check = discarded_witness.load(Ordering::Acquire);
        let base_rows_written = base_write_scope.finish();
        let base_work = base_work_scope.finish();
        let user_work = user_work_scope.finish();
        let library_work = library_work_scope.finish();
        let mut base_row_clones = run.final_identity.base_row_clone_counts;
        base_row_clones.insert(
            "root-name-index.entries",
            u64::from(std::ptr::from_ref(&self.root_names).addr() != root_name_index_identity),
        );
        let ranges = UserDeltaRangesForTest::from_evidence(
            &self.prefixes,
            &run.final_identity.ends,
            run.final_identity.actual_ids.clone(),
        );
        let mut diagnostics = Vec::new();
        let mut incompletes = Vec::new();
        for report in run.reports {
            let line_index = crate::span::LineIndex::new(&report.source);
            diagnostics.extend(report.output.diagnostics.iter().map(|diagnostic| {
                let start = line_index.line_col(diagnostic.span.start);
                let end = line_index.line_col(diagnostic.span.end);
                format!(
                    "{}:{}:{}-{}:{} {} {}",
                    report.name,
                    start.line,
                    start.column,
                    end.line,
                    end.column,
                    diagnostic.code.as_str(),
                    diagnostic.message
                )
            }));
            incompletes.extend(report.output.incomplete);
        }
        diagnostics.sort();
        incompletes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(UserDeltaProjectReceiptForTest {
            diagnostics,
            incompletes,
            references: UserDeltaReferenceSummaryForTest {
                base_to_delta: run.final_identity.references.base_to_delta,
                delta_to_base: run.final_identity.references.delta_to_base,
                delta_to_delta: run.final_identity.references.delta_to_delta,
            },
            mutation: UserDeltaMutationSummaryForTest {
                base_rows_written,
                local_rows_written: run.final_identity.local_rows_written,
                delta_discarded_after_check,
            },
            work: UserDeltaWorkForTest {
                library_source_compiles: library_work.compiles,
                library_source_parses: library_work.parses,
                library_source_binds: library_work.binds,
                library_source_checks: library_work.checks,
                user_source_parses: user_work.parses,
                user_source_binds: user_work.binds,
                user_source_checks: user_work.checks,
                base_row_clones,
                base_rows_sequentially_scanned: base_work.sequential_scans,
                base_rows_materialized: base_work.materializations,
                base_rows_remapped: base_work.remaps,
            },
            ranges,
            initial_visible_user_names,
            cross_file: UserDeltaCrossFileForTest {
                producer_type_group: run.cross_file.producer_type_group,
                consumer_type_group: run.cross_file.consumer_type_group,
                producer_value_storage: run.cross_file.producer_value_storage,
                consumer_value_storage: run.cross_file.consumer_value_storage,
            },
        })
    }

    #[cfg(test)]
    fn new_layered_user_delta_for_test(&self) -> Result<LayeredUserDelta, &'static str> {
        LayeredUserDelta::new(self)
    }
}

#[cfg(test)]
impl UserDeltaRangesForTest {
    fn from_evidence(
        frozen: &FrozenLibraryPrefixes,
        ends: &OwnedBaseFinalIdentityEnds,
        actual: OwnedBaseActualIds,
    ) -> Self {
        Self {
            types: UserDeltaDomainRangeForTest::new(frozen.types, ends.store, actual.types),
            type_params: UserDeltaDomainRangeForTest::new(
                frozen.type_params,
                ends.type_params,
                actual.type_params,
            ),
            classes: UserDeltaDomainRangeForTest::new(frozen.classes, ends.classes, actual.classes),
            scopes: UserDeltaDomainRangeForTest::new(frozen.scopes, ends.scopes, actual.scopes),
            symbols: UserDeltaDomainRangeForTest::new(frozen.symbols, ends.symbols, actual.symbols),
            declarations: UserDeltaDomainRangeForTest::new(
                frozen.declarations,
                ends.declarations,
                actual.declarations,
            ),
            type_groups: UserDeltaDomainRangeForTest::new(
                frozen.type_groups,
                ends.type_groups,
                actual.type_groups,
            ),
            namespaces: UserDeltaDomainRangeForTest::new(
                frozen.namespaces,
                ends.namespaces,
                actual.namespaces,
            ),
            value_storages: UserDeltaDomainRangeForTest::new(
                frozen.value_storages,
                ends.value_storages,
                actual.value_storages,
            ),
        }
    }
}

#[cfg(test)]
impl UserDeltaDomainRangeForTest {
    fn new(start: usize, end: usize, allocated_ids: Vec<usize>) -> Self {
        let range = start..end;
        Self {
            allocated_ids,
            range,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenLibraryIdentity {
    profile_sha256: String,
    schema_sha256: &'static str,
}

impl FrozenLibraryIdentity {
    fn new(profile_sha256: String) -> Self {
        Self {
            profile_sha256,
            schema_sha256: super::compiler::COMPILER_SCHEMA_SHA256,
        }
    }

    #[cfg(test)]
    pub(super) fn profile_sha256(&self) -> &str {
        &self.profile_sha256
    }

    #[cfg(test)]
    pub(super) const fn schema_sha256(&self) -> &'static str {
        self.schema_sha256
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenLibraryPrefixes {
    pub(super) types: usize,
    pub(super) type_params: usize,
    pub(super) classes: usize,
    pub(super) scopes: usize,
    pub(super) symbols: usize,
    pub(super) declarations: usize,
    pub(super) type_groups: usize,
    pub(super) namespaces: usize,
    pub(super) value_storages: usize,
}

impl FrozenLibraryPrefixes {
    fn from_array(values: [usize; 9]) -> Self {
        let [types, type_params, classes, scopes, symbols, declarations, type_groups, namespaces, value_storages] =
            values;
        Self {
            types,
            type_params,
            classes,
            scopes,
            symbols,
            declarations,
            type_groups,
            namespaces,
            value_storages,
        }
    }

    #[cfg(test)]
    fn to_array(&self) -> [usize; 9] {
        [
            self.types,
            self.type_params,
            self.classes,
            self.scopes,
            self.symbols,
            self.declarations,
            self.type_groups,
            self.namespaces,
            self.value_storages,
        ]
    }

    #[cfg(test)]
    pub(super) fn named_rows_for_test(&self) -> BTreeMap<&'static str, usize> {
        BTreeMap::from([
            ("types", self.types),
            ("type-params", self.type_params),
            ("classes", self.classes),
            ("scopes", self.scopes),
            ("symbols", self.symbols),
            ("declarations", self.declarations),
            ("type-groups", self.type_groups),
            ("namespaces", self.namespaces),
            ("value-storages", self.value_storages),
        ])
    }
}
