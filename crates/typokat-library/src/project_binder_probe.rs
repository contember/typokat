//! Measurement-only seam for the project-local declaration binder.

use super::compiler::LibraryCompilerWorkScopeForTest;
use super::project_binder_projection::normalize_project_binder;
use super::provider::{LibraryBaseProvider, ProjectBinderInitializationScopeForProbe};
use crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::{
    CanonicalLibraryFrontendWorkScopeForTest, UserDeltaForkScopeForTest, UserSourceWorkScopeForTest,
};
use crate::check::checker::ProjectBindingMeasurementScope;
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use crate::types::layered::LocalRowAllocationScopeForTest;
use std::fmt;
use std::sync::Arc;

const RETAINED_CONTROL_BYTES: usize = 1024 * 1024;
const USER_SOURCE_PATH: &str = "/candidate/reflect_packaged.ts";
const USER_SOURCE: &str =
    include_str!("../../../tests/cases/sr_project_local_library_candidate/reflect_packaged.ts");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProbeSetupControl {
    #[default]
    None,
    RetainedOneMiB,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectBinderProbeError(String);

impl fmt::Display for ProjectBinderProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ProjectBinderProbeError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectBinderProbeSetupCountersV1 {
    pub declaration_product_initializations: u64,
    pub semantic_base_initializations: u64,
    pub retained_setup_control_bytes: u64,
    pub user_parses: u64,
    pub user_binds: u64,
    pub user_checks: u64,
}

/// Opaque fixed-size setup receipt owned by `typokat-library`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectBinderProbeSetupReceiptV1 {
    counters: ProjectBinderProbeSetupCountersV1,
}

impl ProjectBinderProbeSetupReceiptV1 {
    pub const fn counters(self) -> ProjectBinderProbeSetupCountersV1 {
        self.counters
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectBinderQueryCacheWritesV1 {
    pub projection: u64,
    pub evaluator: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectBinderPhaseCountersV1 {
    pub constructions: u64,
    pub declaration_product_initializations: u64,
    pub semantic_base_initializations: u64,
    pub product_consumptions: u64,
    pub fresh_binder_entries: u64,
    pub bound_library_units: u64,
    pub bound_user_units: u64,
    pub typed_binding_products: u64,
    pub heavy_binding_projections: u64,
    pub user_parses: u64,
    pub user_binds: u64,
    pub user_checks: u64,
    pub source_manifest_builds: u64,
    pub library_frontend_entries: u64,
    pub library_parse_units: u64,
    pub library_bind_batches: u64,
    pub library_ast_bind_units: u64,
    pub library_full_source_products_consumed: u64,
    pub library_checkpoint_products_consumed: u64,
    pub compiler_compiles: u64,
    pub compiler_parses: u64,
    pub compiler_binds: u64,
    pub compiler_checks: u64,
    pub user_delta_forks: u64,
    pub event_reservations: u64,
    pub query_cache_writes: ProjectBinderQueryCacheWritesV1,
    pub relation_cache_writes: u64,
    pub binder_layered_rows: u64,
}

impl ProjectBinderPhaseCountersV1 {
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            constructions: self.constructions.saturating_add(other.constructions),
            declaration_product_initializations: self
                .declaration_product_initializations
                .saturating_add(other.declaration_product_initializations),
            semantic_base_initializations: self
                .semantic_base_initializations
                .saturating_add(other.semantic_base_initializations),
            product_consumptions: self
                .product_consumptions
                .saturating_add(other.product_consumptions),
            fresh_binder_entries: self
                .fresh_binder_entries
                .saturating_add(other.fresh_binder_entries),
            bound_library_units: self
                .bound_library_units
                .saturating_add(other.bound_library_units),
            bound_user_units: self.bound_user_units.saturating_add(other.bound_user_units),
            typed_binding_products: self
                .typed_binding_products
                .saturating_add(other.typed_binding_products),
            heavy_binding_projections: self
                .heavy_binding_projections
                .saturating_add(other.heavy_binding_projections),
            user_parses: self.user_parses.saturating_add(other.user_parses),
            user_binds: self.user_binds.saturating_add(other.user_binds),
            user_checks: self.user_checks.saturating_add(other.user_checks),
            source_manifest_builds: self
                .source_manifest_builds
                .saturating_add(other.source_manifest_builds),
            library_frontend_entries: self
                .library_frontend_entries
                .saturating_add(other.library_frontend_entries),
            library_parse_units: self
                .library_parse_units
                .saturating_add(other.library_parse_units),
            library_bind_batches: self
                .library_bind_batches
                .saturating_add(other.library_bind_batches),
            library_ast_bind_units: self
                .library_ast_bind_units
                .saturating_add(other.library_ast_bind_units),
            library_full_source_products_consumed: self
                .library_full_source_products_consumed
                .saturating_add(other.library_full_source_products_consumed),
            library_checkpoint_products_consumed: self
                .library_checkpoint_products_consumed
                .saturating_add(other.library_checkpoint_products_consumed),
            compiler_compiles: self
                .compiler_compiles
                .saturating_add(other.compiler_compiles),
            compiler_parses: self.compiler_parses.saturating_add(other.compiler_parses),
            compiler_binds: self.compiler_binds.saturating_add(other.compiler_binds),
            compiler_checks: self.compiler_checks.saturating_add(other.compiler_checks),
            user_delta_forks: self.user_delta_forks.saturating_add(other.user_delta_forks),
            event_reservations: self
                .event_reservations
                .saturating_add(other.event_reservations),
            query_cache_writes: ProjectBinderQueryCacheWritesV1 {
                projection: self
                    .query_cache_writes
                    .projection
                    .saturating_add(other.query_cache_writes.projection),
                evaluator: self
                    .query_cache_writes
                    .evaluator
                    .saturating_add(other.query_cache_writes.evaluator),
            },
            relation_cache_writes: self
                .relation_cache_writes
                .saturating_add(other.relation_cache_writes),
            binder_layered_rows: self
                .binder_layered_rows
                .saturating_add(other.binder_layered_rows),
        }
    }
}

/// Opaque allocation-free phase receipt owned by `typokat-library`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectBinderPhaseReceiptV1 {
    counters: ProjectBinderPhaseCountersV1,
}

impl ProjectBinderPhaseReceiptV1 {
    pub const fn counters(self) -> ProjectBinderPhaseCountersV1 {
        self.counters
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectBinderProbeResult {
    pub product_arc_reused: bool,
    pub binder_digest: String,
}

/// One fresh binder and product kept alive until the caller finishes measurement.
pub struct ProjectBinderProbeSample {
    phase: ProjectBinderPhaseReceiptV1,
    product_arc_reused: bool,
    _product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    library_units: Vec<crate::binder::bind::LibraryBinderUnit>,
    bound: crate::check::checker::BoundProjectBinder,
}

impl ProjectBinderProbeSample {
    pub const fn phase_receipt(&self) -> ProjectBinderPhaseReceiptV1 {
        self.phase
    }

    /// Normalize the retained binder only after the measurement interval has ended.
    pub fn finish_result(&self) -> Result<ProjectBinderProbeResult, ProjectBinderProbeError> {
        Ok(ProjectBinderProbeResult {
            product_arc_reused: self.product_arc_reused,
            binder_digest: normalize_project_binder(&self.library_units, &self.bound).digest(),
        })
    }
}

/// Parsed user programs and the retained declaration product scoped to the frontend callback.
pub struct ProjectBinderProbe<'ast> {
    setup: ProjectBinderProbeSetupReceiptV1,
    product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    product_digest: String,
    units: &'ast [crate::frontend::ProjectProgram<'ast>],
    source_reexports: &'ast crate::frontend::AdmittedSourceReexports,
    default_modules: &'ast crate::frontend::DefaultModuleCandidates,
    _setup_control: Option<Vec<u8>>,
}

impl ProjectBinderProbe<'_> {
    pub const fn setup_receipt(&self) -> ProjectBinderProbeSetupReceiptV1 {
        self.setup
    }

    pub fn product_digest(&self) -> &str {
        &self.product_digest
    }

    pub fn bind_one(&self) -> Result<ProjectBinderProbeSample, ProjectBinderProbeError> {
        let initializations = ProjectBinderInitializationScopeForProbe::start();
        let binding = ProjectBindingMeasurementScope::start();
        let manifests = SourceDeclarationManifestBuildScopeForTest::start();
        let frontend = CanonicalLibraryFrontendWorkScopeForTest::start();
        let compiler = LibraryCompilerWorkScopeForTest::start();
        let user = UserSourceWorkScopeForTest::start();
        let delta = UserDeltaForkScopeForTest::start();
        let events = UserEventReservationScopeForTest::start();
        let queries = QueryCacheWriteScopeForTest::start();
        let relations = RelationCacheWriteScopeForTest::start();
        let layered_rows = LocalRowAllocationScopeForTest::start();

        let product = Arc::clone(&self.product);
        let (library_units, bound) = crate::check::checker::bind_library_product_project_programs(
            &product,
            self.units,
            self.source_reexports,
            self.default_modules,
        )
        .map_err(|error| ProjectBinderProbeError(error.to_string()))?;
        crate::check::checker::library_compiler::record_user_source_binds_for_test(
            self.units.len(),
        );

        let layered_rows = layered_rows.finish();
        let relations = relations.finish();
        let queries = queries.finish();
        let events = events.finish();
        let delta = delta.finish();
        let user = user.finish();
        let compiler = compiler.finish();
        let frontend = frontend.finish();
        let manifests = manifests.finish();
        let binding = binding.finish();
        let initializations = initializations.finish();
        let product_arc_reused = Arc::ptr_eq(&self.product, &product);
        let phase = ProjectBinderPhaseReceiptV1 {
            counters: ProjectBinderPhaseCountersV1 {
                constructions: binding.entries,
                declaration_product_initializations: initializations.declaration_products,
                semantic_base_initializations: initializations.semantic_bases,
                product_consumptions: u64::from(product_arc_reused),
                fresh_binder_entries: binding.entries,
                bound_library_units: u64::try_from(library_units.len()).unwrap_or(u64::MAX),
                bound_user_units: binding.bound_units,
                typed_binding_products: binding.typed_products,
                heavy_binding_projections: binding.heavy_binding_projections,
                user_parses: user.parses,
                user_binds: user.binds,
                user_checks: user.checks,
                source_manifest_builds: manifests,
                library_frontend_entries: frontend.entries,
                library_parse_units: frontend.parse_units,
                library_bind_batches: frontend.bind_batches,
                library_ast_bind_units: frontend.bind_units,
                library_full_source_products_consumed: frontend.full_source_products_consumed,
                library_checkpoint_products_consumed: frontend.checkpoint_products_consumed,
                compiler_compiles: compiler.compiles,
                compiler_parses: compiler.parses,
                compiler_binds: compiler.binds,
                compiler_checks: compiler.checks,
                user_delta_forks: delta,
                event_reservations: events,
                query_cache_writes: ProjectBinderQueryCacheWritesV1 {
                    projection: queries.projection,
                    evaluator: queries.evaluator,
                },
                relation_cache_writes: relations,
                binder_layered_rows: layered_rows,
            },
        };
        Ok(ProjectBinderProbeSample {
            phase,
            product_arc_reused,
            _product: product,
            library_units,
            bound,
        })
    }
}

/// Own preparation until `consume` returns, so no parsed program can escape its allocator.
pub fn with_project_binder_probe<T>(
    control: ProbeSetupControl,
    consume: impl for<'ast> FnOnce(ProjectBinderProbe<'ast>) -> T,
) -> Result<T, ProjectBinderProbeError> {
    let initializations = ProjectBinderInitializationScopeForProbe::start();
    let provider = LibraryBaseProvider::new();
    let user = UserSourceWorkScopeForTest::start();
    let product = provider
        .declaration_product()
        .map_err(|error| ProjectBinderProbeError(error.to_string()))?;
    let product_digest = product.audit_receipt().canonical_digest;
    let setup_control = match control {
        ProbeSetupControl::None => None,
        ProbeSetupControl::RetainedOneMiB => Some(vec![0_u8; RETAINED_CONTROL_BYTES]),
    };
    let run = crate::frontend::run_prepared_project_frontend_with_parse_work(
        vec![crate::frontend::FileInput {
            name: USER_SOURCE_PATH.to_owned(),
            source: USER_SOURCE.to_owned(),
        }],
        |units, source_reexports, default_modules, parse_work| {
            let initializations = initializations.finish();
            let user = user.finish();
            let setup = ProjectBinderProbeSetupReceiptV1 {
                counters: ProjectBinderProbeSetupCountersV1 {
                    declaration_product_initializations: initializations.declaration_products,
                    semantic_base_initializations: initializations.semantic_bases,
                    retained_setup_control_bytes: setup_control
                        .as_ref()
                        .map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                    user_parses: parse_work.parser_invocations,
                    user_binds: user.binds,
                    user_checks: user.checks,
                },
            };
            consume(ProjectBinderProbe {
                setup,
                product,
                product_digest,
                units,
                source_reexports,
                default_modules,
                _setup_control: setup_control,
            })
        },
    );
    if let Some(error) = run.preparation_error {
        return Err(ProjectBinderProbeError(format!(
            "probe project preparation failed: {error}"
        )));
    }
    run.product.ok_or_else(|| {
        ProjectBinderProbeError(format!(
            "probe project parse failed: {:?}",
            run.parse_errors
        ))
    })
}
