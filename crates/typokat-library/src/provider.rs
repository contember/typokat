//! Instance-scoped, failure-caching publication of the source-compiled library base.

use super::base::FrozenLibraryBase;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "project-binder-probe")]
thread_local! {
    static PROBE_DECLARATION_PRODUCT_INITIALIZATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROBE_SEMANTIC_BASE_INITIALIZATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(feature = "project-binder-probe")]
#[derive(Clone, Copy)]
pub(crate) struct ProjectBinderInitializationWorkForProbe {
    pub(crate) declaration_products: u64,
    pub(crate) semantic_bases: u64,
}

#[cfg(feature = "project-binder-probe")]
pub(crate) struct ProjectBinderInitializationScopeForProbe(ProjectBinderInitializationWorkForProbe);

#[cfg(feature = "project-binder-probe")]
impl ProjectBinderInitializationScopeForProbe {
    pub(crate) fn start() -> Self {
        Self(project_binder_initialization_work_for_probe())
    }

    pub(crate) fn finish(self) -> ProjectBinderInitializationWorkForProbe {
        let end = project_binder_initialization_work_for_probe();
        ProjectBinderInitializationWorkForProbe {
            declaration_products: end
                .declaration_products
                .saturating_sub(self.0.declaration_products),
            semantic_bases: end.semantic_bases.saturating_sub(self.0.semantic_bases),
        }
    }
}

#[cfg(feature = "project-binder-probe")]
fn project_binder_initialization_work_for_probe() -> ProjectBinderInitializationWorkForProbe {
    ProjectBinderInitializationWorkForProbe {
        declaration_products: PROBE_DECLARATION_PRODUCT_INITIALIZATIONS.get(),
        semantic_bases: PROBE_SEMANTIC_BASE_INITIALIZATIONS.get(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryInitStage {
    ProfileLoad,
    Compile,
    Publication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryInitCause {
    ProfileRejected { message: String },
    CompilationFailed { message: String },
    IncompletePublication { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryInitError {
    stage: LibraryInitStage,
    cause: LibraryInitCause,
}

impl LibraryInitError {
    pub(super) const fn new(stage: LibraryInitStage, cause: LibraryInitCause) -> Self {
        Self { stage, cause }
    }

    pub const fn stage(&self) -> LibraryInitStage {
        self.stage
    }

    pub const fn cause(&self) -> &LibraryInitCause {
        &self.cause
    }

    #[doc(hidden)]
    pub fn injected_initialization_failure(message: String) -> Self {
        Self::new(
            LibraryInitStage::ProfileLoad,
            LibraryInitCause::ProfileRejected { message },
        )
    }
}

impl fmt::Display for LibraryInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "default library initialization failed at {:?}: {:?}",
            self.stage, self.cause
        )
    }
}

impl std::error::Error for LibraryInitError {}

pub struct LibraryBaseProvider {
    result: OnceLock<Result<Arc<InitializedLibrary>, Arc<LibraryInitError>>>,
    #[cfg_attr(not(test), allow(dead_code))]
    declaration_product: OnceLock<
        Result<
            Arc<typokat_library_product::LibraryDeclarationProduct>,
            Arc<super::declaration_product::LibraryDeclarationProductBuildError>,
        >,
    >,
    attempts: AtomicU64,
    publications: AtomicU64,
    compile_us: AtomicU64,
    publication_us: AtomicU64,
}

struct InitializedLibrary {
    base: Arc<FrozenLibraryBase>,
}

#[doc(hidden)]
pub struct LibraryProjectBinderContinuation {
    library_unit_count: usize,
    bound: crate::check::checker::BoundProjectBinder,
}

#[cfg(test)]
pub(super) struct CompleteSourceDeclarationOracleForTest {
    pub(super) library_units: Vec<crate::binder::bind::LibraryBinderUnit>,
    pub(super) bound: crate::check::checker::BoundProjectBinder,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CompleteSourceOwnerCountsForTest {
    pub(super) type_groups: usize,
    pub(super) values: usize,
    pub(super) namespaces: usize,
    pub(super) classes: usize,
    pub(super) global_objects: usize,
    pub(super) statements: usize,
}

#[cfg(test)]
impl CompleteSourceOwnerCountsForTest {
    pub(super) const fn semantic_total(self) -> usize {
        self.type_groups + self.values + self.namespaces + self.classes + self.global_objects
    }

    pub(super) const fn total(self) -> usize {
        self.semantic_total() + self.statements
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CompleteSourceSemanticDemandOracleForTest {
    pub(super) owner_counts: CompleteSourceOwnerCountsForTest,
    pub(super) library_semantic_counts: CompleteSourceOwnerCountsForTest,
    pub(super) user_semantic_counts: CompleteSourceOwnerCountsForTest,
    pub(super) mixed_semantic_counts: CompleteSourceOwnerCountsForTest,
    pub(super) phase_counts: crate::check::checker::library_compiler::LibraryPhaseCounts,
    pub(super) owner_rows: Vec<String>,
    pub(super) library_semantic_owner_rows: Vec<String>,
    pub(super) user_semantic_owner_rows: Vec<String>,
    pub(super) mixed_semantic_owner_rows: Vec<String>,
    pub(super) root_rows: Vec<String>,
    pub(super) global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub(super) reverse_edges: Vec<ProjectLocalDependencyEdgeForTest>,
    pub(super) root_consumers: Vec<ProjectLocalRootConsumerForTest>,
    pub(super) sccs: Vec<ProjectLocalSccForTest>,
    pub(super) publication_sccs: Vec<ProjectLocalPublicationSccForTest>,
    pub(super) unavailable_terminals: Vec<ProjectLocalUnavailableTerminalForTest>,
    pub(super) user_statement_rows: Vec<String>,
    pub(super) semantic_closure_rows: Vec<String>,
    pub(super) source_inputs: Vec<ProjectLocalSourceInputForTest>,
    pub(super) source_visit_rows: Vec<String>,
    pub(super) semantic_probes: ProjectLocalSemanticProbesForTest,
    pub(super) force_all_semantic_probes: ProjectLocalSemanticProbesForTest,
    pub(super) user_event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    pub(super) user_check_results: Vec<ProjectLocalCheckResultForTest>,
    pub(super) library_record_kinds: Vec<ProjectLocalRecordKindForTest>,
    pub(super) library_record_ledger: Vec<u8>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectLocalRecordKindForTest {
    Diagnostic,
    Incomplete,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalDependencyEdgeForTest {
    pub(super) dependency: String,
    pub(super) consumer: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalRootConsumerForTest {
    pub(super) name: String,
    pub(super) slot: &'static str,
    pub(super) consumer: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalSccForTest {
    pub(super) members: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalPublicationGroupForTest {
    pub(super) name: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalPublicationSccForTest {
    pub(super) members: Vec<ProjectLocalPublicationGroupForTest>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectLocalOrderForTest {
    Forward,
    Reverse,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectLocalSemanticFaultForTest {
    OmitDependencyEdge,
    PublishBeforeSccComplete,
    WriteResolvingToDurableCache,
    PublishStaleGlobalObject,
    InvokeForceAllFromOrdinaryDemand,
}

#[cfg(test)]
#[derive(Clone)]
pub(super) struct ProjectLocalSemanticRequestForTest {
    pub(super) case_id: &'static str,
    pub(super) inputs: Vec<super::base::UserDeltaProjectInputForTest<'static>>,
    pub(super) source_order: ProjectLocalOrderForTest,
    pub(super) demand_order: ProjectLocalOrderForTest,
    pub(super) force_all: bool,
    pub(super) fault: Option<ProjectLocalSemanticFaultForTest>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalSourceInputForTest {
    pub(super) path: String,
    pub(super) byte_len: usize,
    pub(super) sha256: String,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectLocalTerminalStateForTest {
    Resolving,
    Ready,
    Unavailable(ProjectLocalUnavailableReasonForTest),
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectLocalUnavailableReasonForTest {
    MissingExportedMemberName,
    DuplicateExportedValue,
    UnboundExportedVariable,
    MissingExportedVariableSyntax,
    InvalidUsingDeclaration,
    VariableSurfaceUnavailable,
    UnboundExportedFunction,
    MissingExportedFunctionSyntax,
    UnboundExportedClass,
    UnboundExportedClassIdentity,
    UnboundNestedNamespace,
    UnsupportedExportedMember,
    DeferredExportedMember,
    FunctionNamespacePayloadUnavailable,
    FunctionOwnerCallSurfaceUnavailable,
    FunctionSurfaceUnavailable,
    ClassSurfaceUnavailable,
    NestedNamespaceUnavailable,
    ExistingOwnerUnavailable,
    NamespaceContainmentCycle,
    InvalidPrivateNamespaceMember,
    ClassValueSurfaceUnavailable,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalNamespaceFragmentForTest {
    pub(super) source_path: String,
    pub(super) origin: ProjectLocalContributorOriginForTest,
    pub(super) source_start: u32,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalUnavailableTerminalForTest {
    pub(super) name: String,
    pub(super) fragments: Vec<ProjectLocalNamespaceFragmentForTest>,
    pub(super) reason: ProjectLocalUnavailableReasonForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalUnavailableAuditForTest {
    pub(super) terminal: ProjectLocalUnavailableTerminalForTest,
    pub(super) discarded_provisional_writes: Vec<String>,
    pub(super) published_provisional_writes: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalSccPublicationForTest {
    pub(super) scc: ProjectLocalSccForTest,
    pub(super) reserved_members: Vec<String>,
    pub(super) published_members: Vec<String>,
    pub(super) observed_before_complete: Vec<String>,
    pub(super) state_before_publish: ProjectLocalTerminalStateForTest,
    pub(super) final_state: ProjectLocalTerminalStateForTest,
    pub(super) discarded_provisional_writes: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalPublicationSccCommitForTest {
    pub(super) scc: ProjectLocalPublicationSccForTest,
    pub(super) reserved_members: Vec<ProjectLocalPublicationGroupForTest>,
    pub(super) published_members: Vec<ProjectLocalPublicationGroupForTest>,
    pub(super) observed_before_complete: Vec<ProjectLocalPublicationGroupForTest>,
    pub(super) state_before_publish: ProjectLocalTerminalStateForTest,
    pub(super) final_state: ProjectLocalTerminalStateForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalCacheWriteForTest {
    pub(super) owner: String,
    pub(super) state: ProjectLocalTerminalStateForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalRepeatedDemandForTest {
    pub(super) owner: String,
    pub(super) first_identity: String,
    pub(super) repeated_identity: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalForceAllTraceForTest {
    pub(super) semantic_owner_rows: Vec<String>,
    pub(super) semantic_probes: ProjectLocalSemanticProbesForTest,
    pub(super) record_kinds: Vec<ProjectLocalRecordKindForTest>,
    pub(super) record_ledger: Vec<u8>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalUserEventRecordForTest {
    pub(super) source_path: String,
    pub(super) source_start: u32,
    pub(super) event_ordinal: usize,
    pub(super) record_ordinal: usize,
    pub(super) record: crate::check::checker::library_compiler::NormalizedCheckerRecordForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalCheckResultForTest {
    pub(super) source_path: String,
    pub(super) diagnostics:
        Vec<crate::check::checker::library_compiler::NormalizedDiagnosticForTest>,
    pub(super) incomplete:
        Vec<crate::check::checker::library_compiler::NormalizedIncompleteForTest>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalModuleTypeProbeForTest {
    pub(super) source_path: String,
    pub(super) probe: crate::check::checker::library_compiler::NormalizedTypeProbeForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalCallableMemberProbeForTest {
    pub(super) name: String,
    pub(super) source_path: String,
    pub(super) reservation_source_path: String,
    pub(super) source_start: u32,
    pub(super) call_signature_count: usize,
    pub(super) signatures: Vec<crate::check::checker::library_compiler::SignatureProbe>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalValueProbeForTest {
    pub(super) name: String,
    pub(super) declaration_count: usize,
    pub(super) call_signature_count: usize,
    pub(super) signatures: Vec<crate::check::checker::library_compiler::SignatureProbe>,
    pub(super) member_names: Vec<String>,
    pub(super) callable_members: Vec<ProjectLocalCallableMemberProbeForTest>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalSemanticProbesForTest {
    pub(super) global_types:
        Vec<crate::check::checker::library_compiler::NormalizedTypeProbeForTest>,
    pub(super) module_types: Vec<ProjectLocalModuleTypeProbeForTest>,
    pub(super) global_values: Vec<ProjectLocalValueProbeForTest>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ProjectLocalContributorOriginForTest {
    Library,
    User,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ProjectLocalContributorKindForTest {
    Ordinary,
    Namespace,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalGlobalContributorSiteForTest {
    pub(super) source_path: String,
    pub(super) origin: ProjectLocalContributorOriginForTest,
    pub(super) span_start: u32,
    pub(super) span_end: u32,
    pub(super) kind: ProjectLocalContributorKindForTest,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProjectLocalGlobalContributorForTest {
    pub(super) name: String,
    pub(super) has_value: bool,
    pub(super) has_type: bool,
    pub(super) has_namespace: bool,
    pub(super) sites: Vec<ProjectLocalGlobalContributorSiteForTest>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalSemanticTraceForTest {
    pub(super) case_id: String,
    pub(super) project_session_identity: u64,
    pub(super) publisher_session_identity: u64,
    pub(super) source_inputs: Vec<ProjectLocalSourceInputForTest>,
    pub(super) source_visit_rows: Vec<String>,
    pub(super) demand_visit_rows: Vec<String>,
    pub(super) graph_owner_rows: Vec<String>,
    pub(super) root_rows: Vec<String>,
    pub(super) reverse_edges: Vec<ProjectLocalDependencyEdgeForTest>,
    pub(super) root_consumers: Vec<ProjectLocalRootConsumerForTest>,
    pub(super) sccs: Vec<ProjectLocalSccForTest>,
    pub(super) publication_sccs: Vec<ProjectLocalPublicationSccForTest>,
    pub(super) unavailable_terminals: Vec<ProjectLocalUnavailableTerminalForTest>,
    pub(super) ordinary_semantic_owner_rows: Vec<String>,
    pub(super) semantic_probes: ProjectLocalSemanticProbesForTest,
    pub(super) user_event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    pub(super) user_check_results: Vec<ProjectLocalCheckResultForTest>,
    pub(super) unavailable_audits: Vec<ProjectLocalUnavailableAuditForTest>,
    pub(super) scc_publications: Vec<ProjectLocalSccPublicationForTest>,
    pub(super) publication_scc_commits: Vec<ProjectLocalPublicationSccCommitForTest>,
    pub(super) durable_cache_writes: Vec<ProjectLocalCacheWriteForTest>,
    pub(super) final_global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub(super) published_global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub(super) repeated_demands: Vec<ProjectLocalRepeatedDemandForTest>,
    pub(super) ordinary_force_all_invocations: Vec<String>,
    pub(super) force_all: Option<ProjectLocalForceAllTraceForTest>,
}

#[cfg(test)]
pub(super) struct PreparedProjectDeclarationUniverseForTest {
    pub(super) product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    pub(super) library_units: Vec<crate::binder::bind::LibraryBinderUnit>,
    pub(super) bound: crate::check::checker::BoundProjectBinder,
    pub(super) metrics: ProjectDeclarationUniverseMetricsForTest,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProjectDeclarationUniverseMetricsForTest {
    pub(super) product_consumptions: u64,
    pub(super) bound_user_units: u64,
    pub(super) binding_wall_us: u64,
}

impl LibraryProjectBinderContinuation {
    #[doc(hidden)]
    pub const fn library_unit_count(&self) -> usize {
        self.library_unit_count
    }

    #[doc(hidden)]
    pub fn project_unit_count(&self) -> usize {
        self.bound.project_sources.len()
    }

    #[doc(hidden)]
    pub fn project_source_kind(
        &self,
        path: &str,
    ) -> Option<crate::binder::namespace::SourceFileKind> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.source_file_kind)
    }

    #[doc(hidden)]
    pub fn project_source_is_external(&self, path: &str) -> Option<bool> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.external_module)
    }

    #[cfg(test)]
    pub(crate) fn project_sources_for_test(
        &self,
    ) -> &[crate::check::checker::ProjectSourceBindingRow] {
        &self.bound.project_sources
    }

    #[cfg(test)]
    pub(crate) fn normalized_per_path_binding_shape_for_test(&self) -> Vec<String> {
        self.bound
            .normalized
            .normalized_per_path_binding_shape
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn normalized_import_export_shape_for_test(&self) -> Vec<String> {
        self.bound.normalized.normalized_import_export_shape.clone()
    }

    #[cfg(test)]
    pub(crate) fn normalized_namespace_shape_for_test(&self) -> Vec<String> {
        self.bound.normalized.normalized_namespace_shape.clone()
    }
}

impl LibraryBaseProvider {
    pub const fn new() -> Self {
        Self {
            result: OnceLock::new(),
            declaration_product: OnceLock::new(),
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
            compile_us: AtomicU64::new(0),
            publication_us: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
        self.initialized()
            .map(|initialized| initialized.base.clone())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn declaration_product(
        &self,
    ) -> Result<
        Arc<typokat_library_product::LibraryDeclarationProduct>,
        Arc<super::declaration_product::LibraryDeclarationProductBuildError>,
    > {
        self.declaration_product
            .get_or_init(|| {
                #[cfg(feature = "project-binder-probe")]
                PROBE_DECLARATION_PRODUCT_INITIALIZATIONS.set(
                    PROBE_DECLARATION_PRODUCT_INITIALIZATIONS
                        .get()
                        .saturating_add(1),
                );
                super::declaration_product::build_packaged_declaration_product().map_err(Arc::new)
            })
            .clone()
    }

    #[cfg(test)]
    pub(super) fn prepare_project_declaration_universe_for_test(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
        perturb_local_ids: bool,
    ) -> Result<PreparedProjectDeclarationUniverseForTest, String> {
        let product = self
            .declaration_product()
            .map_err(|error| error.to_string())?;
        let files = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        crate::check::checker::library_compiler::record_user_source_parses_for_test(files.len());
        let product_for_build = Arc::clone(&product);
        let run = crate::frontend::run_prepared_project_frontend(
            files,
            move |units, source_reexports, default_modules| {
                let started = Instant::now();
                let result = if perturb_local_ids {
                    crate::check::checker::bind_library_product_project_programs_perturbed_for_test(
                        &product_for_build,
                        units,
                        source_reexports,
                        default_modules,
                        17,
                    )
                } else {
                    crate::check::checker::bind_library_product_project_programs(
                        &product_for_build,
                        units,
                        source_reexports,
                        default_modules,
                    )
                };
                let binding_wall_us =
                    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                result.map(|(library_units, bound)| {
                    crate::check::checker::library_compiler::record_user_source_binds_for_test(
                        units.len(),
                    );
                    (library_units, bound, binding_wall_us)
                })
            },
        );
        let Some(result) = run.product else {
            if let Some(error) = run.preparation_error {
                return Err(format!("prepared project inventory failed: {error}"));
            }
            return Err(format!(
                "prepared project parse failed: {:?}",
                run.parse_errors
            ));
        };
        let (library_units, bound, binding_wall_us) = result.map_err(|error| error.to_string())?;
        Ok(PreparedProjectDeclarationUniverseForTest {
            product,
            library_units,
            bound,
            metrics: ProjectDeclarationUniverseMetricsForTest {
                product_consumptions: 1,
                bound_user_units: u64::try_from(inputs.len()).unwrap_or(u64::MAX),
                binding_wall_us,
            },
        })
    }

    /// Continue a compiled library binder checkpoint through the project's own files.
    ///
    /// Provenance is structural, not cryptographic: `LibraryBinderCheckpoint` is opaque and
    /// only `LibraryCompiler::compile_binder_checkpoint` can produce the value passed here.
    #[doc(hidden)]
    pub fn continue_library_project_binder(
        &self,
        checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
        inputs: Vec<crate::frontend::FileInput>,
    ) -> Result<LibraryProjectBinderContinuation, LibraryInitError> {
        let library_unit_count = checkpoint.library_unit_count();
        let bound = crate::check::checker::library_compiler::continue_library_project_binder(
            checkpoint, inputs,
        )
        .map_err(compilation_failed)?;
        Ok(LibraryProjectBinderContinuation {
            library_unit_count,
            bound,
        })
    }

    #[cfg(test)]
    pub(super) fn complete_source_declaration_oracle_for_test(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
    ) -> Result<CompleteSourceDeclarationOracleForTest, LibraryInitError> {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
        let owned_sources = super::compiler::owned_library_sources(profile.sources())
            .map_err(|error| compilation_failed(error.to_string()))?;
        let injected = super::compiler::injected_library_sources(&owned_sources);
        let auxiliary = owned_sources
            .iter()
            .map(
                |(ordinal, name, source)| crate::frontend::AuxiliarySourceInput {
                    source_ordinal: ordinal.index(),
                    name: name.clone(),
                    source: source.clone(),
                },
            )
            .collect::<Vec<_>>();
        let compiler_inputs = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        crate::check::checker::library_compiler::record_user_source_parses_for_test(inputs.len());
        let clean_auxiliary = auxiliary.clone();
        let run = crate::frontend::run_prepared_project_frontend(
            compiler_inputs,
            |units, source_reexports, _| {
                crate::frontend::run_project_frontend_with_auxiliary_parse_gate(
                    Vec::new(),
                    clean_auxiliary,
                    |_, _, library_programs, _, parse_work| {
                        crate::check::checker::library_compiler::record_complete_source_auxiliary_parse_work_for_test(
                            parse_work.parser_invocations,
                            parse_work.source_reparses,
                        );
                        crate::check::checker::library_compiler::compile_project_declaration_oracle_one_root_for_test(
                            &injected,
                            library_programs,
                            units,
                            source_reexports,
                        )
                    },
                )
                .product
            },
        );
        let Some(result) = run.product else {
            let message = run.preparation_error.map_or_else(
                || {
                    format!(
                        "prepared user project contains parse errors: {:?}",
                        run.parse_errors
                    )
                },
                |error| format!("prepared user project inventory failed: {error}"),
            );
            return Err(compilation_failed(message));
        };
        let (library_units, bound) =
            result.map_err(|error| compilation_failed(format!("{error:?}")))?;
        crate::check::checker::library_compiler::record_user_source_binds_for_test(inputs.len());
        Ok(CompleteSourceDeclarationOracleForTest {
            library_units,
            bound,
        })
    }

    #[cfg(test)]
    pub(super) fn complete_source_semantic_demand_oracle_for_test(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
    ) -> Result<CompleteSourceSemanticDemandOracleForTest, LibraryInitError> {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
        let publication_sccs = project_local_publication_sccs()?;
        let mut owned_sources = super::compiler::owned_library_sources(profile.sources())
            .map_err(|error| compilation_failed(error.to_string()))?;
        let library_count = owned_sources.len();
        for (input_ordinal, input) in inputs.iter().enumerate() {
            owned_sources.push((
                crate::source::LibraryFileOrdinal::new(library_count + input_ordinal),
                input.path.to_owned(),
                input.source.to_owned(),
            ));
        }
        let file_names = owned_sources
            .iter()
            .map(|(ordinal, name, _)| (*ordinal, name.clone()))
            .collect::<BTreeMap<_, _>>();
        if file_names.len() != owned_sources.len() {
            return Err(compilation_failed(
                "complete-source demand oracle received duplicate source ordinals".to_owned(),
            ));
        }
        let injected = super::compiler::injected_library_sources(&owned_sources);
        let (run, runtime) = crate::check::checker::library_compiler::compile_complete_combined_replay_oracle_for_test(
            &injected,
            library_count,
        )
        .map_err(|error| compilation_failed(format!("{error:?}")))?;
        let user_event_ledger = normalize_project_local_user_events(
            run.normalized_user_event_store_ledger_for_test(),
            &file_names,
        )?;
        let user_check_results = normalize_project_local_check_results(
            run.normalized_user_check_results_for_test()
                .map_err(|error| compilation_failed(format!("{error:?}")))?,
            &file_names,
        )?;
        let force_all_semantic_probes = normalize_project_local_semantic_probes(
            run.normalized_raw_semantic_probes_for_test()
                .map_err(|error| compilation_failed(format!("{error:?}")))?,
            &file_names,
        )?;
        let library_record_kinds = run
            .library_records
            .iter()
            .map(|(_, record)| {
                if record.is_diagnostic() {
                    ProjectLocalRecordKindForTest::Diagnostic
                } else {
                    ProjectLocalRecordKindForTest::Incomplete
                }
            })
            .collect::<Vec<_>>();
        let library_record_ledger =
            crate::check::checker::library_compiler::canonical_library_evidence_for_test(
                &injected,
                &run.library_records,
            )
            .map_err(|error| compilation_failed(format!("{error:?}")))?
            .ledger;
        let graph = runtime
            .complete_source_replay_graph_for_test()
            .map_err(|error| compilation_failed(error.to_owned()))?;
        let evidence = CompleteSourceSemanticEvidenceForTest {
            phase_counts: run.phase_counts,
            library_record_kinds,
            library_record_ledger,
            user_event_ledger,
            user_check_results,
            force_all_semantic_probes,
        };
        let mut receipt =
            complete_source_semantic_demand_receipt(graph, &file_names, library_count, evidence)?;
        receipt.source_inputs = normalize_project_local_source_inputs(inputs);
        receipt.source_visit_rows = inputs.iter().map(|input| input.path.to_owned()).collect();
        receipt.publication_sccs = publication_sccs;
        Ok(receipt)
    }

    #[cfg(test)]
    pub(super) fn project_local_semantic_candidate_trace_for_test(
        &self,
        request: ProjectLocalSemanticRequestForTest,
    ) -> Result<ProjectLocalSemanticTraceForTest, LibraryInitError> {
        let ProjectLocalSemanticRequestForTest {
            case_id,
            inputs,
            source_order,
            demand_order,
            force_all,
            fault,
        } = request;
        drop((
            case_id,
            inputs,
            source_order,
            demand_order,
            force_all,
            fault,
        ));
        Err(compilation_failed(
            "WU4 project-local semantic coordinator is not implemented".to_owned(),
        ))
    }

    #[cfg(test)]
    pub(super) fn continue_library_binder_checkpoint_for_test<F>(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
        inspect: F,
    ) -> Result<super::base::LibraryBinderContinuationReceiptForTest, LibraryInitError>
    where
        F: FnOnce(
            &crate::binder::bind::LibraryBinderCheckpointInspectionForTest<'_>,
            &crate::check::checker::replay_index::AdmittedCollisionReplayIndex,
        ),
    {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
        let checkpoint = super::compiler::LibraryCompiler::new()
            .compile_binder_checkpoint(&profile)
            .map_err(|error| compilation_failed(error.to_string()))?;
        // The shared base must be published before the checkpoint continues through it.
        self.initialized().map_err(|error| error.as_ref().clone())?;
        let inspection = checkpoint.inspection_for_test();
        let checkpoint_ends = inspection.ends;
        let array_symbol = inspection.array_symbol;
        let array_type_group = inspection.array_type_group;
        let library_modules = inspection
            .library_units
            .iter()
            .map(|unit| unit.module)
            .collect::<Vec<_>>();
        inspect(
            &inspection,
            super::base::deferred_packaged_replay_index_for_test(),
        );
        let compiler_inputs = inputs
            .iter()
            .map(|input| crate::frontend::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let bound = crate::check::checker::library_compiler::continue_library_project_binder(
            checkpoint,
            compiler_inputs,
        )
        .map_err(compilation_failed)?;
        let continuation = crate::check::checker::library_compiler::continuation_receipt_for_test(
            checkpoint_ends,
            array_symbol,
            array_type_group,
            bound,
        )
        .map_err(compilation_failed)?;
        let mapped_owner_sites = super::base::deferred_packaged_replay_index_for_test()
            .owner_sites
            .iter()
            .map(|site| super::base::MappedReplayOwnerSiteForTest {
                owner: site.owner,
                file_ordinal: site.file_ordinal,
                span: site.span,
                syntax_module: library_modules[site.file_ordinal.index()],
            })
            .collect();
        Ok(super::base::LibraryBinderContinuationReceiptForTest {
            continuation,
            mapped_owner_sites,
        })
    }

    fn initialized(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        self.result.get_or_init(|| self.initialize()).clone()
    }

    fn initialize(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        #[cfg(feature = "project-binder-probe")]
        PROBE_SEMANTIC_BASE_INITIALIZATIONS
            .set(PROBE_SEMANTIC_BASE_INITIALIZATIONS.get().saturating_add(1));
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let compile = Instant::now();
        let compiled = FrozenLibraryBase::compile_packaged_profile().map_err(Arc::new)?;
        self.compile_us
            .store(elapsed_us(compile), Ordering::Relaxed);
        let publication = Instant::now();
        let base = Arc::new(FrozenLibraryBase::publish(compiled).map_err(Arc::new)?);
        self.publication_us
            .store(elapsed_us(publication), Ordering::Relaxed);
        self.publications.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(InitializedLibrary { base }))
    }

    #[cfg(test)]
    pub(super) fn measurement_for_test(&self) -> InitializationMeasurement {
        InitializationMeasurement {
            attempts: self.attempts.load(Ordering::Relaxed),
            publications: self.publications.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(super) fn declaration_product_initialized_for_test(&self) -> bool {
        self.declaration_product.get().is_some()
    }
}

#[cfg(test)]
struct CompleteSourceSemanticEvidenceForTest {
    phase_counts: crate::check::checker::library_compiler::LibraryPhaseCounts,
    library_record_kinds: Vec<ProjectLocalRecordKindForTest>,
    library_record_ledger: Vec<u8>,
    user_event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    user_check_results: Vec<ProjectLocalCheckResultForTest>,
    force_all_semantic_probes: ProjectLocalSemanticProbesForTest,
}

#[cfg(test)]
fn complete_source_semantic_demand_receipt(
    graph: crate::check::checker::library_compiler::CompleteSourceReplayGraphForTest,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
    library_count: usize,
    evidence: CompleteSourceSemanticEvidenceForTest,
) -> Result<CompleteSourceSemanticDemandOracleForTest, LibraryInitError> {
    let CompleteSourceSemanticEvidenceForTest {
        phase_counts,
        library_record_kinds,
        library_record_ledger,
        user_event_ledger,
        user_check_results,
        force_all_semantic_probes,
    } = evidence;
    if graph.unowned_demand_count != 0
        || graph.invalid_owner_site_count != 0
        || graph.noncanonical_edge_count != 0
        || graph.typed_reference_coverage_misses != 0
    {
        return Err(compilation_failed(format!(
            "complete-source demand graph is unhealthy: unowned={}, invalid-sites={}, noncanonical-edges={}, reference-misses={}",
            graph.unowned_demand_count,
            graph.invalid_owner_site_count,
            graph.noncanonical_edge_count,
            graph.typed_reference_coverage_misses,
        )));
    }

    let global_contributors = normalize_project_local_global_contributors(
        graph.final_global_contributors.clone(),
        file_names,
        library_count,
    )?;
    let unavailable_terminals = normalize_project_local_unavailable_terminals(
        &graph.standalone_namespace_terminals,
        file_names,
        library_count,
    )?;

    let mut names = graph.owner_names.into_iter().collect::<BTreeMap<_, _>>();
    let mut sites = BTreeMap::<_, Vec<_>>::new();
    let mut owner_origins = BTreeMap::<_, (bool, bool)>::new();
    for site in graph.owner_sites {
        let origins = owner_origins.entry(site.owner).or_default();
        if site.file_ordinal.index() < library_count {
            origins.0 = true;
        } else {
            origins.1 = true;
        }
        sites.entry(site.owner).or_default().push(site);
    }
    let mut stable_owners = BTreeMap::new();
    let mut library_origin_stable_owners = BTreeMap::new();
    let mut user_origin_stable_owners = BTreeMap::new();
    let mut owner_counts = CompleteSourceOwnerCountsForTest::default();
    for owner in &graph.owner_partition {
        match owner {
            crate::check::checker::replay_index::ReplayOwner::TypeGroup(_) => {
                owner_counts.type_groups += 1;
            }
            crate::check::checker::replay_index::ReplayOwner::Value(_) => {
                owner_counts.values += 1;
            }
            crate::check::checker::replay_index::ReplayOwner::Namespace(_) => {
                owner_counts.namespaces += 1;
            }
            crate::check::checker::replay_index::ReplayOwner::Class(_) => {
                owner_counts.classes += 1;
            }
            crate::check::checker::replay_index::ReplayOwner::GlobalObject => {
                owner_counts.global_objects += 1;
            }
            crate::check::checker::replay_index::ReplayOwner::Statement(_) => {
                owner_counts.statements += 1;
            }
        }
        let owner_name = names
            .remove(owner)
            .ok_or_else(|| compilation_failed("complete-source owner has no name".to_owned()))?;
        let owner_sites = sites.remove(owner).unwrap_or_default();
        let (library, user) = owner_origins.get(owner).copied().unwrap_or_default();
        if library {
            library_origin_stable_owners.insert(
                *owner,
                normalize_complete_source_owner(
                    *owner,
                    owner_name.clone(),
                    owner_sites
                        .iter()
                        .filter(|site| site.file_ordinal.index() < library_count)
                        .cloned()
                        .collect(),
                    file_names,
                )?,
            );
        }
        if user {
            user_origin_stable_owners.insert(
                *owner,
                normalize_complete_source_owner(
                    *owner,
                    owner_name.clone(),
                    owner_sites
                        .iter()
                        .filter(|site| site.file_ordinal.index() >= library_count)
                        .cloned()
                        .collect(),
                    file_names,
                )?,
            );
        }
        let stable = normalize_complete_source_owner(*owner, owner_name, owner_sites, file_names)?;
        stable_owners.insert(*owner, stable);
    }
    if owner_counts.total() != graph.owner_partition.len()
        || !names.is_empty()
        || stable_owners.len() != graph.owner_partition.len()
    {
        return Err(compilation_failed(
            "complete-source owner partition did not normalize bijectively".to_owned(),
        ));
    }
    let owner_rows = sorted_unique_rows(stable_owners.values().cloned().collect());
    if owner_rows.len() != stable_owners.len() {
        return Err(compilation_failed(
            "complete-source stable owner names are not unique".to_owned(),
        ));
    }
    let mut library_semantic_counts = CompleteSourceOwnerCountsForTest::default();
    let mut user_semantic_counts = CompleteSourceOwnerCountsForTest::default();
    let mut mixed_semantic_counts = CompleteSourceOwnerCountsForTest::default();
    let mut library_semantic_owner_rows = Vec::new();
    let mut user_semantic_owner_rows = Vec::new();
    let mut mixed_semantic_owner_rows = Vec::new();
    for (owner, row) in &stable_owners {
        if matches!(
            owner,
            crate::check::checker::replay_index::ReplayOwner::Statement(_)
        ) {
            continue;
        }
        let (library, user) = owner_origins.get(owner).copied().unwrap_or_default();
        if library {
            increment_complete_source_owner_count(&mut library_semantic_counts, *owner);
            library_semantic_owner_rows.push(
                library_origin_stable_owners
                    .get(owner)
                    .cloned()
                    .ok_or_else(|| {
                        compilation_failed("library owner has no library projection".to_owned())
                    })?,
            );
        }
        if user {
            increment_complete_source_owner_count(&mut user_semantic_counts, *owner);
            user_semantic_owner_rows.push(
                user_origin_stable_owners
                    .get(owner)
                    .cloned()
                    .ok_or_else(|| {
                        compilation_failed("user owner has no user projection".to_owned())
                    })?,
            );
        }
        if library && user {
            increment_complete_source_owner_count(&mut mixed_semantic_counts, *owner);
            mixed_semantic_owner_rows.push(row.clone());
        }
    }
    library_semantic_owner_rows = sorted_unique_rows(library_semantic_owner_rows);
    user_semantic_owner_rows = sorted_unique_rows(user_semantic_owner_rows);
    mixed_semantic_owner_rows = sorted_unique_rows(mixed_semantic_owner_rows);

    let stable = |owner| {
        stable_owners
            .get(&owner)
            .cloned()
            .ok_or_else(|| compilation_failed("edge references an unknown owner".to_owned()))
    };
    let mut root_rows = Vec::with_capacity(graph.root_slots.len());
    let mut root_seed_owners = BTreeMap::<
        (String, crate::check::checker::replay_index::RootSlotKind),
        BTreeSet<crate::check::checker::replay_index::ReplayOwner>,
    >::new();
    for root in &graph.root_slots {
        let value = root
            .value
            .map(crate::check::checker::replay_index::ReplayOwner::Value)
            .map(&stable)
            .transpose()?
            .unwrap_or_else(|| "-".to_owned());
        let ty = root
            .ty
            .map(crate::check::checker::replay_index::ReplayOwner::TypeGroup)
            .map(&stable)
            .transpose()?
            .unwrap_or_else(|| "-".to_owned());
        let namespace = root
            .namespace
            .map(crate::check::checker::replay_index::ReplayOwner::Namespace)
            .map(&stable)
            .transpose()?
            .unwrap_or_else(|| "-".to_owned());
        root_rows.push(format!(
            "root:{:?}|value={value}|type={ty}|namespace={namespace}|global-object={}|explicit-global-this={}",
            root.name, root.global_object_contributor, root.explicit_global_this,
        ));
        if let Some(value) = root.value {
            root_seed_owners
                .entry((
                    root.name.clone(),
                    crate::check::checker::replay_index::RootSlotKind::Value,
                ))
                .or_default()
                .insert(crate::check::checker::replay_index::ReplayOwner::Value(
                    value,
                ));
        }
        if let Some(ty) = root.ty {
            root_seed_owners
                .entry((
                    root.name.clone(),
                    crate::check::checker::replay_index::RootSlotKind::Type,
                ))
                .or_default()
                .insert(crate::check::checker::replay_index::ReplayOwner::TypeGroup(
                    ty,
                ));
        }
        if let Some(namespace) = root.namespace {
            root_seed_owners
                .entry((
                    root.name.clone(),
                    crate::check::checker::replay_index::RootSlotKind::Namespace,
                ))
                .or_default()
                .insert(crate::check::checker::replay_index::ReplayOwner::Namespace(
                    namespace,
                ));
        }
    }
    root_rows = sorted_unique_rows(root_rows);

    let mut dependencies = BTreeMap::<_, BTreeSet<_>>::new();
    let mut reverse_edges = Vec::with_capacity(graph.reverse_edges.len());
    for edge in graph.reverse_edges {
        dependencies
            .entry(edge.consumer)
            .or_default()
            .insert(edge.dependency);
        reverse_edges.push(ProjectLocalDependencyEdgeForTest {
            dependency: stable(edge.dependency)?,
            consumer: stable(edge.consumer)?,
        });
    }
    reverse_edges.sort();
    reverse_edges.dedup();

    let mut root_consumers = Vec::with_capacity(graph.root_slot_consumers.len());
    for consumer in &graph.root_slot_consumers {
        root_consumers.push(ProjectLocalRootConsumerForTest {
            name: consumer.name.clone(),
            slot: root_slot_kind_name(consumer.slot),
            consumer: stable(consumer.consumer)?,
        });
    }
    root_consumers.sort();
    root_consumers.dedup();
    for consumer in &graph.root_slot_consumers {
        if let Some(root_seeds) = root_seed_owners.get(&(consumer.name.clone(), consumer.slot)) {
            dependencies
                .entry(consumer.consumer)
                .or_default()
                .extend(root_seeds.iter().copied());
        }
    }

    let mut owner_to_scc = BTreeMap::new();
    let mut sccs = Vec::with_capacity(graph.scc_membership.len());
    for scc in &graph.scc_membership {
        let members = scc.owners.iter().copied().collect::<Vec<_>>();
        for owner in &members {
            if owner_to_scc.insert(*owner, members.clone()).is_some() {
                return Err(compilation_failed(
                    "complete-source owner occurs in multiple SCCs".to_owned(),
                ));
            }
        }
        let member_rows = members
            .iter()
            .copied()
            .map(&stable)
            .collect::<Result<Vec<_>, _>>()?;
        sccs.push(ProjectLocalSccForTest {
            members: sorted_unique_rows(member_rows),
        });
    }
    if owner_to_scc.len() != stable_owners.len() {
        return Err(compilation_failed(
            "complete-source SCC partition does not cover every owner".to_owned(),
        ));
    }
    sccs.sort();
    sccs.dedup();

    let mut user_statements = BTreeSet::new();
    for (key, owner) in graph.statement_owners {
        if key.file_ordinal.index() >= library_count {
            if owner != crate::check::checker::replay_index::ReplayOwner::Statement(key) {
                return Err(compilation_failed(
                    "user statement owner does not match its event key".to_owned(),
                ));
            }
            user_statements.insert(owner);
        }
    }
    let user_statement_rows = sorted_unique_rows(
        user_statements
            .iter()
            .copied()
            .map(&stable)
            .collect::<Result<Vec<_>, _>>()?,
    );

    let mut closure = user_statements.clone();
    let mut pending = user_statements.into_iter().collect::<Vec<_>>();
    while let Some(consumer) = pending.pop() {
        let Some(direct) = dependencies.get(&consumer) else {
            continue;
        };
        for dependency in direct {
            let scc = owner_to_scc
                .get(dependency)
                .ok_or_else(|| compilation_failed("dependency has no SCC".to_owned()))?;
            for owner in scc {
                if closure.insert(*owner) {
                    pending.push(*owner);
                }
            }
        }
    }
    let semantic_closure_rows = sorted_unique_rows(
        closure
            .into_iter()
            .filter(|owner| {
                !matches!(
                    owner,
                    crate::check::checker::replay_index::ReplayOwner::Statement(_)
                )
            })
            .map(&stable)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let semantic_probes =
        select_demanded_semantic_probes(&force_all_semantic_probes, &semantic_closure_rows);

    Ok(CompleteSourceSemanticDemandOracleForTest {
        owner_counts,
        library_semantic_counts,
        user_semantic_counts,
        mixed_semantic_counts,
        phase_counts,
        owner_rows,
        library_semantic_owner_rows,
        user_semantic_owner_rows,
        mixed_semantic_owner_rows,
        root_rows,
        global_contributors,
        reverse_edges,
        root_consumers,
        sccs,
        publication_sccs: Vec::new(),
        unavailable_terminals,
        user_statement_rows,
        semantic_closure_rows,
        source_inputs: Vec::new(),
        source_visit_rows: Vec::new(),
        semantic_probes,
        force_all_semantic_probes,
        user_event_ledger,
        user_check_results,
        library_record_kinds,
        library_record_ledger,
    })
}

#[cfg(test)]
fn normalize_project_local_user_events(
    rows: Vec<crate::check::checker::library_compiler::NormalizedUserEventRecordForTest>,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
) -> Result<Vec<ProjectLocalUserEventRecordForTest>, LibraryInitError> {
    let mut normalized = rows
        .into_iter()
        .map(|row| {
            Ok(ProjectLocalUserEventRecordForTest {
                source_path: complete_source_path(file_names, row.source_ordinal)?,
                source_start: row.source_start,
                event_ordinal: row.event_ordinal,
                record_ordinal: row.record_ordinal,
                record: row.record,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    normalized.sort_by(|left, right| {
        (
            &left.source_path,
            left.source_start,
            left.event_ordinal,
            left.record_ordinal,
        )
            .cmp(&(
                &right.source_path,
                right.source_start,
                right.event_ordinal,
                right.record_ordinal,
            ))
    });
    Ok(normalized)
}

#[cfg(test)]
fn normalize_project_local_global_contributors(
    inventory: crate::binder::roots::FinalGlobalContributorInventory,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
    library_count: usize,
) -> Result<Vec<ProjectLocalGlobalContributorForTest>, LibraryInitError> {
    let mut rows = inventory
        .rows
        .into_iter()
        .map(|row| {
            let mut sites = row
                .sites
                .into_iter()
                .map(|site| {
                    let (origin, source_ordinal) = match site.origin {
                        crate::source::CompilationOrigin::Library(source_ordinal) => (
                            ProjectLocalContributorOriginForTest::Library,
                            source_ordinal,
                        ),
                        crate::source::CompilationOrigin::User(source_ordinal) => {
                            let index = library_count
                                .checked_add(source_ordinal.index())
                                .ok_or_else(|| {
                                    compilation_failed(
                                        "global contributor source ordinal overflowed".to_owned(),
                                    )
                                })?;
                            (
                                ProjectLocalContributorOriginForTest::User,
                                crate::source::LibraryFileOrdinal::new(index),
                            )
                        }
                    };
                    let kind = match site.kind {
                        crate::binder::declaration::SourceGlobalContributorKind::Ordinary => {
                            ProjectLocalContributorKindForTest::Ordinary
                        }
                        crate::binder::declaration::SourceGlobalContributorKind::Namespace => {
                            ProjectLocalContributorKindForTest::Namespace
                        }
                    };
                    Ok(ProjectLocalGlobalContributorSiteForTest {
                        source_path: complete_source_path(file_names, source_ordinal)?,
                        origin,
                        span_start: site.span.start,
                        span_end: site.span.end,
                        kind,
                    })
                })
                .collect::<Result<Vec<_>, LibraryInitError>>()?;
            sites.sort();
            Ok(ProjectLocalGlobalContributorForTest {
                name: row.name,
                has_value: row.value.is_some(),
                has_type: row.ty.is_some(),
                has_namespace: row.namespace.is_some(),
                sites,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    rows.sort();
    Ok(rows)
}

#[cfg(test)]
fn normalize_project_local_unavailable_terminals(
    rows: &[crate::check::checker::library_compiler::CompleteSourceStandaloneNamespaceTerminalRowForTest],
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
    library_count: usize,
) -> Result<Vec<ProjectLocalUnavailableTerminalForTest>, LibraryInitError> {
    let mut normalized = rows
        .iter()
        .filter_map(|row| {
            let crate::check::checker::library_compiler::CompleteSourceStandaloneNamespaceTerminalForTest::Unavailable(reason) = row.terminal else {
                return None;
            };
            if row.fragments.iter().all(|fragment| {
                matches!(fragment.origin, crate::source::CompilationOrigin::Library(_))
            }) {
                return None;
            }
            Some((row, reason))
        })
        .map(|(row, reason)| {
            let mut fragments = row
                .fragments
                .iter()
                .map(|fragment| {
                    let (origin, source_ordinal) = match fragment.origin {
                        crate::source::CompilationOrigin::Library(source_ordinal) => {
                            (ProjectLocalContributorOriginForTest::Library, source_ordinal)
                        }
                        crate::source::CompilationOrigin::User(source_ordinal) => {
                            let index = library_count
                                .checked_add(source_ordinal.index())
                                .ok_or_else(|| {
                                    compilation_failed(
                                        "namespace terminal source ordinal overflowed".to_owned(),
                                    )
                                })?;
                            (
                                ProjectLocalContributorOriginForTest::User,
                                crate::source::LibraryFileOrdinal::new(index),
                            )
                        }
                    };
                    Ok(ProjectLocalNamespaceFragmentForTest {
                        source_path: complete_source_path(file_names, source_ordinal)?,
                        origin,
                        source_start: fragment.source_start,
                    })
                })
                .collect::<Result<Vec<_>, LibraryInitError>>()?;
            fragments.sort();
            Ok(ProjectLocalUnavailableTerminalForTest {
                name: row.name.clone(),
                fragments,
                reason: project_local_unavailable_reason(reason),
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    normalized
        .sort_by(|left, right| (&left.name, &left.fragments).cmp(&(&right.name, &right.fragments)));
    Ok(normalized)
}

#[cfg(test)]
const fn project_local_unavailable_reason(
    reason: crate::check::checker::library_compiler::CompleteSourceNamespaceUnavailableReasonForTest,
) -> ProjectLocalUnavailableReasonForTest {
    use crate::check::checker::library_compiler::CompleteSourceNamespaceUnavailableReasonForTest as Source;
    match reason {
        Source::MissingExportedMemberName => {
            ProjectLocalUnavailableReasonForTest::MissingExportedMemberName
        }
        Source::DuplicateExportedValue => {
            ProjectLocalUnavailableReasonForTest::DuplicateExportedValue
        }
        Source::UnboundExportedVariable => {
            ProjectLocalUnavailableReasonForTest::UnboundExportedVariable
        }
        Source::MissingExportedVariableSyntax => {
            ProjectLocalUnavailableReasonForTest::MissingExportedVariableSyntax
        }
        Source::InvalidUsingDeclaration => {
            ProjectLocalUnavailableReasonForTest::InvalidUsingDeclaration
        }
        Source::VariableSurfaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::VariableSurfaceUnavailable
        }
        Source::UnboundExportedFunction => {
            ProjectLocalUnavailableReasonForTest::UnboundExportedFunction
        }
        Source::MissingExportedFunctionSyntax => {
            ProjectLocalUnavailableReasonForTest::MissingExportedFunctionSyntax
        }
        Source::UnboundExportedClass => ProjectLocalUnavailableReasonForTest::UnboundExportedClass,
        Source::UnboundExportedClassIdentity => {
            ProjectLocalUnavailableReasonForTest::UnboundExportedClassIdentity
        }
        Source::UnboundNestedNamespace => {
            ProjectLocalUnavailableReasonForTest::UnboundNestedNamespace
        }
        Source::UnsupportedExportedMember => {
            ProjectLocalUnavailableReasonForTest::UnsupportedExportedMember
        }
        Source::DeferredExportedMember => {
            ProjectLocalUnavailableReasonForTest::DeferredExportedMember
        }
        Source::FunctionNamespacePayloadUnavailable => {
            ProjectLocalUnavailableReasonForTest::FunctionNamespacePayloadUnavailable
        }
        Source::FunctionOwnerCallSurfaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::FunctionOwnerCallSurfaceUnavailable
        }
        Source::FunctionSurfaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::FunctionSurfaceUnavailable
        }
        Source::ClassSurfaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::ClassSurfaceUnavailable
        }
        Source::NestedNamespaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::NestedNamespaceUnavailable
        }
        Source::ExistingOwnerUnavailable => {
            ProjectLocalUnavailableReasonForTest::ExistingOwnerUnavailable
        }
        Source::NamespaceContainmentCycle => {
            ProjectLocalUnavailableReasonForTest::NamespaceContainmentCycle
        }
        Source::InvalidPrivateNamespaceMember => {
            ProjectLocalUnavailableReasonForTest::InvalidPrivateNamespaceMember
        }
        Source::ClassValueSurfaceUnavailable => {
            ProjectLocalUnavailableReasonForTest::ClassValueSurfaceUnavailable
        }
    }
}

#[cfg(test)]
fn project_local_publication_sccs(
) -> Result<Vec<ProjectLocalPublicationSccForTest>, LibraryInitError> {
    let locked: toml::Value =
        include_str!("../../../tests/cases/sr_project_local_library_candidate/locked-slice.toml")
            .parse()
            .map_err(|error| {
                compilation_failed(format!("locked semantic slice is invalid: {error}"))
            })?;
    let rows = locked
        .get("publication_sccs")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            compilation_failed("locked semantic slice has no publication SCCs".to_owned())
        })?;
    let mut sccs = rows
        .iter()
        .map(|row| {
            let members = row
                .get("members")
                .and_then(toml::Value::as_array)
                .ok_or_else(|| {
                    compilation_failed("locked publication SCC has no members".to_owned())
                })?;
            let mut members = members
                .iter()
                .map(|member| {
                    member
                        .as_str()
                        .map(|name| ProjectLocalPublicationGroupForTest {
                            name: name.to_owned(),
                        })
                        .ok_or_else(|| {
                            compilation_failed(
                                "locked publication SCC member is not a string".to_owned(),
                            )
                        })
                })
                .collect::<Result<Vec<_>, LibraryInitError>>()?;
            members.sort();
            members.dedup();
            if members.is_empty() {
                return Err(compilation_failed(
                    "locked publication SCC is empty".to_owned(),
                ));
            }
            Ok(ProjectLocalPublicationSccForTest { members })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    sccs.sort();
    sccs.dedup();
    Ok(sccs)
}

#[cfg(test)]
fn normalize_project_local_source_inputs(
    inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
) -> Vec<ProjectLocalSourceInputForTest> {
    inputs
        .iter()
        .map(|input| ProjectLocalSourceInputForTest {
            path: input.path.to_owned(),
            byte_len: input.source.len(),
            sha256: format!("{:x}", Sha256::digest(input.source.as_bytes())),
        })
        .collect()
}

#[cfg(test)]
fn normalize_project_local_check_results(
    rows: Vec<crate::check::checker::library_compiler::NormalizedCheckResultForTest>,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
) -> Result<Vec<ProjectLocalCheckResultForTest>, LibraryInitError> {
    let mut normalized = rows
        .into_iter()
        .map(|row| {
            Ok(ProjectLocalCheckResultForTest {
                source_path: complete_source_path(file_names, row.source_ordinal)?,
                diagnostics: row.diagnostics,
                incomplete: row.incomplete,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    normalized.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    Ok(normalized)
}

#[cfg(test)]
fn normalize_project_local_semantic_probes(
    probes: crate::check::checker::library_compiler::NormalizedSemanticProbesForTest,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
) -> Result<ProjectLocalSemanticProbesForTest, LibraryInitError> {
    let mut module_types = probes
        .module_types
        .into_iter()
        .map(|row| {
            Ok(ProjectLocalModuleTypeProbeForTest {
                source_path: complete_source_path(file_names, row.source_ordinal)?,
                probe: row.probe,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    module_types.sort_by(|left, right| {
        (&left.source_path, &left.probe.name).cmp(&(&right.source_path, &right.probe.name))
    });

    let mut global_values = probes
        .global_values
        .into_iter()
        .map(|probe| {
            let mut callable_members = probe
                .callable_members
                .into_iter()
                .map(|member| {
                    Ok(ProjectLocalCallableMemberProbeForTest {
                        name: member.name,
                        source_path: complete_source_path(file_names, member.source_ordinal)?,
                        reservation_source_path: complete_source_path(
                            file_names,
                            member.reservation_source_ordinal,
                        )?,
                        source_start: member.source_start,
                        call_signature_count: member.call_signature_count,
                        signatures: member.signatures,
                    })
                })
                .collect::<Result<Vec<_>, LibraryInitError>>()?;
            callable_members.sort_by(|left, right| {
                (
                    &left.name,
                    &left.source_path,
                    &left.reservation_source_path,
                    left.source_start,
                )
                    .cmp(&(
                        &right.name,
                        &right.source_path,
                        &right.reservation_source_path,
                        right.source_start,
                    ))
            });
            Ok(ProjectLocalValueProbeForTest {
                name: probe.name,
                declaration_count: probe.declaration_count,
                call_signature_count: probe.call_signature_count,
                signatures: probe.signatures,
                member_names: probe.member_names,
                callable_members,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    global_values.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(ProjectLocalSemanticProbesForTest {
        global_types: probes.global_types,
        module_types,
        global_values,
    })
}

#[cfg(test)]
fn select_demanded_semantic_probes(
    probes: &ProjectLocalSemanticProbesForTest,
    semantic_owner_rows: &[String],
) -> ProjectLocalSemanticProbesForTest {
    let names = semantic_owner_rows
        .iter()
        .filter_map(|row| {
            ["type:\"", "value:\""]
                .into_iter()
                .find_map(|prefix| row.strip_prefix(prefix))
                .and_then(|row| row.split_once("\"@").map(|(name, _)| name.to_owned()))
        })
        .collect::<BTreeSet<_>>();
    ProjectLocalSemanticProbesForTest {
        global_types: probes
            .global_types
            .iter()
            .filter(|probe| names.contains(&probe.name))
            .cloned()
            .collect(),
        module_types: probes
            .module_types
            .iter()
            .filter(|probe| names.contains(&probe.probe.name))
            .cloned()
            .collect(),
        global_values: probes
            .global_values
            .iter()
            .filter(|probe| names.contains(&probe.name))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
fn complete_source_path(
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
    source_ordinal: crate::source::LibraryFileOrdinal,
) -> Result<String, LibraryInitError> {
    file_names.get(&source_ordinal).cloned().ok_or_else(|| {
        compilation_failed(
            "complete-source semantic output references an unknown source".to_owned(),
        )
    })
}

#[cfg(test)]
fn normalize_complete_source_owner(
    owner: crate::check::checker::replay_index::ReplayOwner,
    name: String,
    sites: Vec<crate::check::checker::replay_index::ReplayOwnerSite>,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
) -> Result<String, LibraryInitError> {
    if let crate::check::checker::replay_index::ReplayOwner::Statement(key) = owner {
        let path = file_names.get(&key.file_ordinal).ok_or_else(|| {
            compilation_failed("statement owner references an unknown source".to_owned())
        })?;
        return Ok(format!(
            "statement:{path:?}:{}:{}:{}",
            key.source_start, key.event_ordinal, key.record_ordinal,
        ));
    }
    let mut site_rows = sites
        .into_iter()
        .map(|site| {
            file_names
                .get(&site.file_ordinal)
                .map(|path| format!("{path:?}:{}-{}", site.span.start, site.span.end))
                .ok_or_else(|| {
                    compilation_failed("owner site references an unknown source".to_owned())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    site_rows.sort();
    let kind = match owner {
        crate::check::checker::replay_index::ReplayOwner::TypeGroup(_) => "type",
        crate::check::checker::replay_index::ReplayOwner::Value(_) => "value",
        crate::check::checker::replay_index::ReplayOwner::Namespace(_) => "namespace",
        crate::check::checker::replay_index::ReplayOwner::Class(_) => "class",
        crate::check::checker::replay_index::ReplayOwner::GlobalObject => "global-object",
        crate::check::checker::replay_index::ReplayOwner::Statement(_) => unreachable!(),
    };
    Ok(format!("{kind}:{name:?}@{}", site_rows.join("|")))
}

#[cfg(test)]
fn increment_complete_source_owner_count(
    counts: &mut CompleteSourceOwnerCountsForTest,
    owner: crate::check::checker::replay_index::ReplayOwner,
) {
    match owner {
        crate::check::checker::replay_index::ReplayOwner::TypeGroup(_) => counts.type_groups += 1,
        crate::check::checker::replay_index::ReplayOwner::Value(_) => counts.values += 1,
        crate::check::checker::replay_index::ReplayOwner::Namespace(_) => counts.namespaces += 1,
        crate::check::checker::replay_index::ReplayOwner::Class(_) => counts.classes += 1,
        crate::check::checker::replay_index::ReplayOwner::GlobalObject => {
            counts.global_objects += 1;
        }
        crate::check::checker::replay_index::ReplayOwner::Statement(_) => counts.statements += 1,
    }
}

#[cfg(test)]
const fn root_slot_kind_name(
    slot: crate::check::checker::replay_index::RootSlotKind,
) -> &'static str {
    match slot {
        crate::check::checker::replay_index::RootSlotKind::Value => "value",
        crate::check::checker::replay_index::RootSlotKind::Type => "type",
        crate::check::checker::replay_index::RootSlotKind::Namespace => "namespace",
    }
}

#[cfg(test)]
fn sorted_unique_rows(mut rows: Vec<String>) -> Vec<String> {
    rows.sort();
    rows.dedup();
    rows
}

impl Default for LibraryBaseProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn compilation_failed(message: String) -> LibraryInitError {
    LibraryInitError::new(
        LibraryInitStage::Compile,
        LibraryInitCause::CompilationFailed { message },
    )
}

fn elapsed_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
}

/// The process-wide default-library base shared by every spec that needs one.
///
/// Source compilation of the 82 packaged declaration files is the only route to a base, so the
/// suite pays for it once per test binary instead of once per test.
#[cfg(test)]
pub(super) fn shared_library_base_provider_for_test() -> &'static LibraryBaseProvider {
    static SHARED: std::sync::LazyLock<LibraryBaseProvider> =
        std::sync::LazyLock::new(LibraryBaseProvider::new);
    &SHARED
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitializationMeasurement {
    pub(super) attempts: u64,
    pub(super) publications: u64,
}
