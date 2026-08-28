//! Instance-scoped, failure-caching publication of the source-compiled library base.

use super::base::FrozenLibraryBase;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::BTreeMap;

#[cfg(test)]
use crate::check::checker::project_local::{
    ProjectLocalCallableMemberProbeForTest, ProjectLocalCheckResultForTest,
    ProjectLocalContributorKindForTest, ProjectLocalContributorOriginForTest,
    ProjectLocalGlobalContributorForTest, ProjectLocalGlobalContributorSiteForTest,
    ProjectLocalLibraryRecordForTest, ProjectLocalModuleTypeProbeForTest,
    ProjectLocalSemanticProbesForTest, ProjectLocalSourceInputForTest,
    ProjectLocalUserEventRecordForTest, ProjectLocalValueProbeForTest,
};

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
    #[cfg(feature = "project-local-driver-shadow")]
    shadow_declaration_product: OnceLock<
        Result<Arc<typokat_library_product::LibraryDeclarationProduct>, Arc<LibraryInitError>>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CompleteSourceBlackBoxOracleForTest {
    pub(super) source_inputs: Vec<ProjectLocalSourceInputForTest>,
    pub(super) source_visit_rows: Vec<String>,
    pub(super) global_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub(super) force_all_semantic_probes: ProjectLocalSemanticProbesForTest,
    pub(super) user_event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    pub(super) user_check_results: Vec<ProjectLocalCheckResultForTest>,
    pub(super) library_records: Vec<ProjectLocalLibraryRecordForTest>,
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
            #[cfg(feature = "project-local-driver-shadow")]
            shadow_declaration_product: OnceLock::new(),
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

    #[cfg(feature = "project-local-driver-shadow")]
    #[doc(hidden)]
    pub fn declaration_product_for_shadow(
        &self,
    ) -> Result<Arc<typokat_library_product::LibraryDeclarationProduct>, Arc<LibraryInitError>>
    {
        self.shadow_declaration_product
            .get_or_init(|| {
                self.declaration_product().map_err(|error| {
                    let (stage, cause) = match error.as_ref() {
                        super::declaration_product::LibraryDeclarationProductBuildError::Profile(
                            _,
                        ) => (
                            LibraryInitStage::ProfileLoad,
                            LibraryInitCause::ProfileRejected {
                                message: error.to_string(),
                            },
                        ),
                        _ => (
                            LibraryInitStage::Compile,
                            LibraryInitCause::CompilationFailed {
                                message: error.to_string(),
                            },
                        ),
                    };
                    Arc::new(LibraryInitError::new(stage, cause))
                })
            })
            .clone()
    }

    /// Load the immutable declaration product for the isolated-session benchmark.
    #[cfg(feature = "session-bench")]
    #[doc(hidden)]
    pub fn declaration_product_for_session_bench(
        &self,
    ) -> Result<Arc<typokat_library_product::LibraryDeclarationProduct>, String> {
        self.declaration_product()
            .map_err(|error| error.to_string())
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
    pub(super) fn complete_source_black_box_oracle_for_test(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
    ) -> Result<CompleteSourceBlackBoxOracleForTest, LibraryInitError> {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
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
        let library_records = normalize_project_local_library_records(
            run.normalized_library_records_for_test(),
            &file_names,
        )?;
        let global_contributors = normalize_project_local_global_contributors(
            runtime
                .final_global_contributors_for_test()
                .map_err(|error| compilation_failed(error.to_owned()))?,
            &file_names,
            library_count,
        )?;
        Ok(CompleteSourceBlackBoxOracleForTest {
            source_inputs: normalize_project_local_source_inputs(inputs),
            source_visit_rows: inputs.iter().map(|input| input.path.to_owned()).collect(),
            global_contributors,
            force_all_semantic_probes,
            user_event_ledger,
            user_check_results,
            library_records,
        })
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
fn normalize_project_local_library_records(
    rows: Vec<crate::check::checker::library_compiler::NormalizedLibraryRecordForTest>,
    file_names: &BTreeMap<crate::source::LibraryFileOrdinal, String>,
) -> Result<Vec<ProjectLocalLibraryRecordForTest>, LibraryInitError> {
    let mut normalized = rows
        .into_iter()
        .map(|row| {
            Ok(ProjectLocalLibraryRecordForTest {
                source_path: complete_source_path(file_names, row.source_ordinal)?,
                owner_start: row.owner_start,
                record: row.record,
            })
        })
        .collect::<Result<Vec<_>, LibraryInitError>>()?;
    normalized.sort_by(|left, right| {
        (
            &left.source_path,
            left.owner_start,
            format!("{:?}", left.record),
        )
            .cmp(&(
                &right.source_path,
                right.owner_start,
                format!("{:?}", right.record),
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
