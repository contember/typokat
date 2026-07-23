//! Immutable semantic state decoded from the canonical default-library snapshot.

#[cfg(test)]
use super::provider::LibraryInitError;
#[cfg(test)]
use super::snapshot::map_snapshot_error;
use super::snapshot::DecodedCanonicalLibrary;
use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;
#[cfg(test)]
use crate::check::checker::library_compiler::{
    check_caller_certified_collision_free_project_with_owned_library,
    check_caller_certified_collision_free_source_with_base_evidence, OwnedBaseActualIds,
    OwnedBaseFinalIdentityEnds,
};
#[cfg(test)]
use crate::check::checker::library_snapshot_codec::{
    recompute_runtime_projection, validate_runtime_references, RuntimeProjectionForTest,
};
#[cfg(test)]
use crate::check::checker::replay_index::{
    AdmittedCollisionReplayIndex, CollisionReplayIndex, COLLISION_REPLAY_MANIFEST_SHA256,
};
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
#[cfg(test)]
use std::ops::Range;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
const COMPONENT_NAMES: [&str; 11] = [
    "store",
    "interner",
    "binder",
    "declaration-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "id-prefixes",
    "collision-replay-index",
];

#[cfg(test)]
const RUNTIME_FAMILIES: [&str; 11] = [
    "store",
    "interner",
    "binder",
    "decl-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "next-ids",
    "collision-replay-index",
];

#[cfg(test)]
const PROJECTION_SUBTABLES: [&str; 31] = [
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
];

pub struct FrozenLibraryBase {
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "retained for decoded-base user checking")
    )]
    runtime: OwnedLibraryRuntimeState,
    root_names: BTreeSet<String>,
    prefixes: FrozenLibraryPrefixes,
    identity: FrozenLibraryIdentity,
    #[cfg(test)]
    structural_probe: Option<NonterminalStructuralTypeProbeForTest>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReplayIndexMutationForTest {
    MissingSection,
    WrongSectionTag,
    WrongSectionDigest,
    TruncatedSection,
    TrailingSectionBytes,
    WrongManifestDomain,
    UnknownSchema,
    TruncatedInternalLength,
    InternalLengthOverflow,
    InvalidUtf8RootName,
    InvalidOptionalTag,
    InvalidBooleanTag,
    SemanticTrailingBytes,
    InvalidOwnerTag,
    RootNameLengthOverflow,
    MissingOwner,
    MissingStaleButInRangeOwner,
    DuplicateOwner,
    ReorderedOwner,
    UnknownOwner,
    MissingGlobalObjectOwner,
    DuplicateGlobalObjectOwner,
    DuplicateRoot,
    EmptyRootName,
    RootIdOutsidePrefix,
    PopulatedRootIndexMismatch,
    MissingCanonicalPopulatedRoot,
    UnusedPlaceholderRoot,
    DuplicateReverseEdge,
    SelfReverseEdge,
    ReorderedReverseEdge,
    UnknownReverseEdgeOwner,
    DuplicateRootConsumer,
    ReorderedRootConsumer,
    InvalidRootConsumerSlot,
    UnknownRootConsumerOwner,
    UnknownRootConsumerName,
    MissingOwnerSite,
    DuplicateOwnerSite,
    InvalidOwnerSiteSpan,
    UnknownOwnerSiteOwner,
    OwnerSiteFileOutsideProfile,
    InvalidScc,
    MissingSccOwner,
    DuplicateSccOwner,
    ReorderedScc,
    WrongDependencyFirstScc,
    WrongStatementOwner,
    DuplicateStatementOwner,
    MissingStatementOwner,
    StatementFileOutsideProfile,
    ReorderedStatementOwner,
    NonStatementBaselineCountNonzero,
    NonStatementBaselineDigestNoncanonical,
    DuplicateBaseline,
    MissingBaseline,
    NonzeroUnownedDemands,
    NonzeroInvalidOwnerSites,
    NonzeroNoncanonicalEdges,
    NonzeroTypedReferenceMisses,
    SelfConsistentMissingReverseEdge,
    SelfConsistentMissingRootConsumer,
    SelfConsistentMissingOwnerSite,
    SelfConsistentWrongBaseline,
    SelfConsistentWrongBaselineCount,
    SelfConsistentWrongRootProvenance,
    SelfConsistentCrossArtifactSection,
    SelfConsistentButUnpinned,
}

#[cfg(test)]
pub(super) struct RegeneratedReplayIndexForTest {
    index: CollisionReplayIndex,
    pub(super) library_source_compiles: u64,
    pub(super) snapshot_decodes: u64,
}

#[cfg(test)]
impl std::ops::Deref for RegeneratedReplayIndexForTest {
    type Target = CollisionReplayIndex;

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
        let capability = super::collision_preflight::issue_caller_certified_capability_for_test();
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
    pub(super) snapshot_decodes: u64,
    pub(super) snapshot_validations: u64,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PreflightSlotForTest {
    Value,
    Type,
    Namespace,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModuleClassificationForTest {
    Script,
    External,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModuleClassificationEntryForTest {
    pub(super) path: String,
    pub(super) classification: ModuleClassificationForTest,
}

#[cfg(test)]
impl PartialEq<(&str, ModuleClassificationForTest)> for ModuleClassificationEntryForTest {
    fn eq(&self, other: &(&str, ModuleClassificationForTest)) -> bool {
        self.path == other.0 && self.classification == other.1
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CollisionRouteForTest {
    SharedDelta,
    PrivateCombined,
    RejectedBeforeSemantics,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CollisionCandidateForTest {
    pub(super) name: String,
    pub(super) slots: BTreeSet<PreflightSlotForTest>,
    pub(super) global_object_contributor: bool,
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

impl fmt::Debug for FrozenLibraryBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenLibraryBase")
            .field("root_name_count", &self.root_names.len())
            .field("prefixes", &self.prefixes)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl FrozenLibraryBase {
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
        let capability = super::collision_preflight::issue_caller_certified_capability_for_test();
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
        let compiled = super::compiler::LibraryCompiler::new()
            .compile(&profile)
            .map_err(|error| error.to_string())?;
        drop(compiled);
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
        let ends = runtime.identity_ends_for_test();
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
            root_names: BTreeSet::new(),
            prefixes: FrozenLibraryPrefixes {
                types: ends.store,
                type_params: ends.type_params,
                classes: ends.classes,
                scopes: ends.scopes,
                symbols: ends.symbols,
                declarations: ends.declarations,
                type_groups: ends.type_groups,
                namespaces: ends.namespaces,
                value_storages: ends.value_storages,
            },
            identity: FrozenLibraryIdentity::canonical(),
            structural_probe,
        })
    }

    pub(super) fn from_decoded(decoded: DecodedCanonicalLibrary) -> Result<Self, &'static str> {
        let DecodedCanonicalLibrary {
            mut runtime,
            root_names,
            prefixes,
            typed_validation_sha256: _,
            identity,
        } = decoded;
        runtime.freeze_as_library_base()?;
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
            root_names,
            prefixes: FrozenLibraryPrefixes::from_array(prefixes),
            identity,
            #[cfg(test)]
            structural_probe,
        })
    }

    #[cfg(test)]
    pub(super) const fn identity(&self) -> &FrozenLibraryIdentity {
        &self.identity
    }

    #[cfg(test)]
    pub(super) fn replay_index_for_test(&self) -> &AdmittedCollisionReplayIndex {
        self.runtime
            .admitted_replay_index()
            .expect("frozen base retains an admitted replay index")
    }

    #[cfg(test)]
    pub(super) const fn pinned_replay_manifest_sha256_for_test(&self) -> [u8; 32] {
        COLLISION_REPLAY_MANIFEST_SHA256
    }

    #[cfg(test)]
    pub(super) fn snapshot_section_inventory_for_test(&self) -> [&'static str; 11] {
        self.recompute_canonical_projection_for_test()
            .expect("frozen projection")
            .projection
            .family_names()
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
            snapshot_decodes: 0,
        })
    }

    #[cfg(test)]
    pub(super) fn inventory_for_test(&self) -> FrozenLibraryInventory<'_> {
        FrozenLibraryInventory { base: self }
    }

    #[cfg(test)]
    pub(super) fn root_names_for_test(&self) -> &BTreeSet<String> {
        &self.root_names
    }

    #[cfg(test)]
    pub(super) fn prefixes_for_test(&self) -> &FrozenLibraryPrefixes {
        &self.prefixes
    }

    #[cfg(test)]
    pub(super) fn type_count_for_test(&self) -> usize {
        self.runtime.type_count()
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
        let snapshot_work_scope = super::snapshot::SnapshotWorkScopeForTest::start();
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
        let snapshot_work = snapshot_work_scope.finish();
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
                snapshot_decodes: snapshot_work.decodes,
                snapshot_validations: snapshot_work.validations,
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
        let snapshot_work_scope = super::snapshot::SnapshotWorkScopeForTest::start();
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
            .map(|input| crate::driver::FileInput {
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
        let snapshot_work = snapshot_work_scope.finish();
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
                snapshot_decodes: snapshot_work.decodes,
                snapshot_validations: snapshot_work.validations,
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

    #[cfg(test)]
    pub(super) fn recompute_canonical_projection_for_test(
        &self,
    ) -> Result<CanonicalLibraryProjection, LibraryInitError> {
        recompute_runtime_projection(&self.runtime)
            .map(CanonicalLibraryProjection::new)
            .map_err(map_snapshot_error)
    }

    #[cfg(test)]
    pub(super) fn validate_frozen_reference_boundaries_for_test(
        &self,
    ) -> Result<FrozenReferenceBoundarySummary, LibraryInitError> {
        validate_runtime_references(&self.runtime)
            .map(|summary| FrozenReferenceBoundarySummary {
                checked: summary.checked,
                outside_frozen_prefix: summary.outside_frozen_prefix,
                base_to_delta: summary.base_to_delta,
                untyped_or_unowned: summary.untyped_or_unowned,
            })
            .map_err(map_snapshot_error)
    }

    #[cfg(test)]
    pub(super) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(super) const fn retained_archive_bytes_for_test(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(super) const fn retained_projection_witnesses_for_test(&self) -> usize {
        0
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
    profile_sha256: &'static str,
    schema_sha256: &'static str,
    artifact_sha256: &'static str,
    artifact_bytes: usize,
}

impl FrozenLibraryIdentity {
    pub(super) const fn canonical() -> Self {
        Self {
            profile_sha256: super::snapshot::PROFILE_SHA256,
            schema_sha256: super::snapshot::SCHEMA_SHA256,
            artifact_sha256: super::artifact::CANONICAL_SNAPSHOT_SHA256,
            artifact_bytes: super::artifact::CANONICAL_SNAPSHOT_BYTES,
        }
    }

    #[cfg(test)]
    pub(super) const fn profile_sha256(&self) -> &'static str {
        self.profile_sha256
    }

    #[cfg(test)]
    pub(super) const fn schema_sha256(&self) -> &'static str {
        self.schema_sha256
    }

    #[cfg(test)]
    pub(super) const fn artifact_sha256(&self) -> &'static str {
        self.artifact_sha256
    }

    #[cfg(test)]
    pub(super) const fn artifact_bytes(&self) -> usize {
        self.artifact_bytes
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

#[cfg(test)]
pub(super) struct FrozenLibraryInventory<'base> {
    base: &'base FrozenLibraryBase,
}

#[cfg(test)]
impl FrozenLibraryInventory<'_> {
    pub(super) fn source_file_count(&self) -> u32 {
        self.base.runtime.source_file_count()
    }

    pub(super) fn reference_count(&self) -> u64 {
        validate_runtime_references(&self.base.runtime)
            .map(|summary| summary.checked)
            .unwrap_or(0)
    }

    pub(super) const fn runtime_family_count(&self) -> usize {
        RUNTIME_FAMILIES.len()
    }

    pub(super) const fn projection_subtable_count(&self) -> usize {
        PROJECTION_SUBTABLES.len()
    }

    pub(super) const fn component_names(&self) -> [&'static str; 11] {
        COMPONENT_NAMES
    }

    pub(super) fn root_name_count(&self) -> usize {
        self.base.root_names.len()
    }

    pub(super) const fn prefixes(&self) -> &FrozenLibraryPrefixes {
        &self.base.prefixes
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalLibraryProjection {
    projection: RuntimeProjectionForTest,
    prefixes: FrozenLibraryPrefixes,
}

#[cfg(test)]
impl CanonicalLibraryProjection {
    pub(super) fn new(projection: RuntimeProjectionForTest) -> Self {
        Self {
            prefixes: FrozenLibraryPrefixes::from_array(projection.prefixes_for_library()),
            projection,
        }
    }

    pub(super) const fn runtime_families(&self) -> &[&'static str; 11] {
        &RUNTIME_FAMILIES
    }

    pub(super) const fn subtables(&self) -> &[&'static str; 31] {
        &PROJECTION_SUBTABLES
    }

    pub(super) fn reference_family_counts(&self) -> [u64; 9] {
        self.projection.reference_family_counts_for_library()
    }

    pub(super) fn root_names(&self) -> &BTreeSet<String> {
        self.projection.root_names_for_library()
    }

    pub(super) const fn prefixes(&self) -> &FrozenLibraryPrefixes {
        &self.prefixes
    }

    pub(super) fn typed_validation_sha256(&self) -> String {
        self.projection.typed_validation_sha256_for_library()
    }
}

#[cfg(test)]
pub(super) struct FrozenReferenceBoundarySummary {
    pub(super) checked: u64,
    pub(super) outside_frozen_prefix: u64,
    pub(super) base_to_delta: u64,
    pub(super) untyped_or_unowned: u64,
}
