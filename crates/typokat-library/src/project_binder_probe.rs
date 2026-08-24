//! Measurement-only seam for the project-local declaration binder.

use std::fmt;
use std::sync::Arc;

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
    _product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    _library_units: Vec<crate::binder::bind::LibraryBinderUnit>,
    _bound: crate::check::checker::BoundProjectBinder,
}

impl ProjectBinderProbeSample {
    pub const fn phase_receipt(&self) -> ProjectBinderPhaseReceiptV1 {
        self.phase
    }

    /// Normalize the retained binder only after the measurement interval has ended.
    pub fn finish_result(&self) -> Result<ProjectBinderProbeResult, ProjectBinderProbeError> {
        Err(ProjectBinderProbeError(
            "project-local binder result normalization is not implemented".to_owned(),
        ))
    }
}

/// Parsed user programs and the retained declaration product scoped to the frontend callback.
pub struct ProjectBinderProbe<'ast> {
    setup: ProjectBinderProbeSetupReceiptV1,
    _product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    _units: &'ast [crate::frontend::ProjectProgram<'ast>],
    _source_reexports: &'ast crate::frontend::AdmittedSourceReexports,
    _default_modules: &'ast crate::frontend::DefaultModuleCandidates,
    _setup_control: Option<Vec<u8>>,
}

impl ProjectBinderProbe<'_> {
    pub const fn setup_receipt(&self) -> ProjectBinderProbeSetupReceiptV1 {
        self.setup
    }

    pub fn bind_one(&self) -> Result<ProjectBinderProbeSample, ProjectBinderProbeError> {
        Err(ProjectBinderProbeError(
            "project-local binder measurement probe is not implemented".to_owned(),
        ))
    }
}

/// Own preparation until `consume` returns, so no parsed program can escape its allocator.
pub fn with_project_binder_probe<T>(
    _control: ProbeSetupControl,
    _consume: impl for<'ast> FnOnce(ProjectBinderProbe<'ast>) -> T,
) -> Result<T, ProjectBinderProbeError> {
    Err(ProjectBinderProbeError(
        "project-local binder measurement probe is not implemented".to_owned(),
    ))
}
