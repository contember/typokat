//! Pipeline orchestration: source → parse → check → diagnostics.
//!
//! The frontend owns each per-run parser allocation; the driver keeps borrowed
//! parser data inside the parse/check continuation.

use crate::check::checker::CheckResult;
#[cfg(test)]
use crate::check::checker::{
    check_project_programs_with_binding_inspector,
    check_project_programs_with_namespace_value_inspector,
};
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::frontend::{
    parse_source_errors, run_clean_bundler_project_frontend_with_default_modules,
    run_clean_project_frontend_with_deferred_auxiliary, run_project_frontend,
    run_project_frontend_with_auxiliary, run_project_parse_only, run_source_frontend,
    AccountedProjectProduct, AuxiliarySourceInput, DeferredProjectFrontendError, FileInput,
    ProjectFrontendRun, ProjectModuleInventory, ProjectProgram, ProjectResolutionMode, ProjectRoot,
};
use crate::library::{
    FrozenLibraryBase, LibraryBaseProvider, LibraryInitError, RoutedLibraryProject,
    RoutedPrivateExecution, RoutedPrivateLibraryProject,
};
#[cfg(test)]
use crate::span::Span;
use crate::types::Interner;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverInfrastructureKind {
    WorkerSpawn,
    WorkerJoin,
    Check,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverInfrastructureError {
    kind: DriverInfrastructureKind,
    message: String,
}

impl DriverInfrastructureError {
    fn new(kind: DriverInfrastructureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> DriverInfrastructureKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for DriverInfrastructureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "driver infrastructure failed at {:?}: {}",
            self.kind, self.message
        )
    }
}

impl std::error::Error for DriverInfrastructureError {}

#[derive(Clone, Debug)]
pub enum DriverError {
    LibraryInitialization(Arc<LibraryInitError>),
    Infrastructure(DriverInfrastructureError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LibraryInitialization(error) => error.fmt(formatter),
            Self::Infrastructure(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LibraryInitialization(error) => Some(error.as_ref()),
            Self::Infrastructure(error) => Some(error),
        }
    }
}

impl From<Arc<LibraryInitError>> for DriverError {
    fn from(error: Arc<LibraryInitError>) -> Self {
        Self::LibraryInitialization(error)
    }
}

impl From<DriverInfrastructureError> for DriverError {
    fn from(error: DriverInfrastructureError) -> Self {
        Self::Infrastructure(error)
    }
}

/// The outcome of checking one source file. Three independent channels: type
/// diagnostics, parser errors, and the incomplete-surface channel (WU2) — an empty
/// triple is clean. Incomplete takes precedence over diagnostics for the CLI exit code.
pub struct CheckOutput {
    /// Type diagnostics produced by the checker (empty == clean).
    pub diagnostics: Vec<Diagnostic>,
    /// Parser/syntax errors rendered to strings so the CLI can report malformed input.
    pub parse_errors: Vec<String>,
    /// In-scope AST positions the checker skipped (sprint 2026-07-10, WU2). Nothing
    /// emits into this yet; when populated it drives exit `3` (incomplete).
    pub incomplete: Vec<IncompleteSurface>,
    /// Project/module forms that were inventoried but are outside the admitted profile.
    pub project_notices: Vec<String>,
}

impl CheckOutput {
    /// Whether the run found any problem (type or parse). Drives the CLI exit
    /// code.
    pub fn has_errors(&self) -> bool {
        !self.parse_errors.is_empty() || self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Whether the run recorded any incomplete surface. Exit `3` takes precedence
    /// over exit `1`, so the CLI checks this before [`has_errors`](CheckOutput::has_errors).
    pub fn is_incomplete(&self) -> bool {
        !self.incomplete.is_empty() || !self.project_notices.is_empty()
    }
}

/// The worker-thread stack for a single parse→check. oxc's recursive-descent parser has
/// no nesting limit, so a pathologically deep annotation (e.g. a 4000-deep type literal)
/// would overflow a default 2 MiB test-thread / 8 MiB main stack in the PARSER, before
/// the checker's own graceful nesting budget (backlog 63k, `MAX_ANNOTATION_DEPTH`) can
/// report `TK2589`. Running the pipeline on a large fixed stack lets such input parse far
/// enough to hit that budget and emit a stable diagnostic instead of aborting the
/// process. The reservation is lazily committed (virtual), so idle RSS is unaffected.
/// Arbitrarily deeper adversarial input still overflows — a residual owned by 63k until
/// the parser gains its own limit.
const CHECK_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Parse and check one TypeScript source. The local `Allocator` owns the AST only
/// for this call; owned diagnostics are extracted before it drops.
pub fn check_source(source: &str) -> Result<CheckOutput, DriverError> {
    let base = library_base()?;
    let output =
        on_check_worker(&base, || check_source_inner(&base, source))?.map_err(check_failure)?;
    #[cfg(test)]
    test_support::record_reports_exposed_for_test(1);
    Ok(output)
}

fn check_source_inner(base: &FrozenLibraryBase, source: &str) -> Result<CheckOutput, String> {
    check_source_against_library(base, source)
}

/// Result for one [`FileInput`]. `name` and `source` move through the pipeline so
/// diagnostics can render without a side table; `reports[i]` matches `inputs[i]`.
pub struct FileReport {
    pub name: String,
    pub source: String,
    pub output: CheckOutput,
}

/// Production project reports plus the frontend-owned pre-semantic inventory.
pub struct ProjectCheckRun {
    pub reports: Vec<FileReport>,
    pub inventory: ProjectModuleInventory,
}

/// Check many files in parallel, with an independent allocator/interner per file.
/// There is no cross-file resolution on this API, so per-file pipelines are
/// lossless and keep the `!Send + !Sync` AST on its parser thread. Order is
/// preserved: `reports[i]` corresponds to `inputs[i]`.
pub fn check_files(inputs: Vec<FileInput>) -> Result<Vec<FileReport>, DriverError> {
    let base = library_base()?;
    #[cfg(test)]
    test_support::record_provider_acquired_before_rayon_for_test();
    #[cfg(test)]
    let trace_context = test_support::current_trace_context_for_test();
    let reports = inputs
        .into_par_iter()
        .map(|input| -> Result<FileReport, DriverError> {
            #[cfg(test)]
            let _trace_enrollment = trace_context
                .as_ref()
                .map(test_support::ProductionDriverFaultTraceContextForTest::enroll);
            #[cfg(test)]
            test_support::record_rayon_worker_start_for_test();
            let worker_base = base.clone();
            let checked_base = worker_base.clone();
            on_check_worker(&worker_base, move || {
                check_one_file_against_library(&checked_base, input)
            })?
            .map_err(check_failure)
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(test)]
    test_support::record_reports_exposed_for_test(reports.len());
    Ok(reports)
}

/// Check a local relative-module project in one serial type universe, resolving
/// only `./` / `../` specifiers among the provided `.ts` files. Runs on a large-stack
/// worker for the same reason as [`check_source`] (deep input meets the checker's nesting
/// budget rather than a native parser stack overflow); `inputs` and the returned reports
/// are owned/`Send`, so they cross the scope cleanly.
pub fn check_project(inputs: Vec<FileInput>) -> Result<Vec<FileReport>, DriverError> {
    let base = library_base()?;
    #[cfg(test)]
    {
        let (reports, receipt) = on_check_worker(&base, || {
            let reports = check_project_inner(&base, inputs);
            let receipt = crate::check::checker::project_binding_thread_receipt_for_test();
            (reports, receipt)
        })?;
        crate::check::checker::merge_project_binding_thread_receipt_for_test(receipt);
        let reports = reports.map_err(check_failure)?;
        test_support::record_reports_exposed_for_test(reports.len());
        Ok(reports)
    }
    #[cfg(not(test))]
    {
        on_check_worker(&base, || check_project_inner(&base, inputs))?.map_err(check_failure)
    }
}

/// Check one standalone project through a complete library-plus-project source publication.
pub fn check_project_once(inputs: Vec<FileInput>) -> Result<Vec<FileReport>, DriverError> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(test)]
    {
        let (reports, receipt, complete_source_receipt) = on_standalone_check_worker(|| {
            let reports = check_project_once_inner(inputs, ProjectResolutionMode::ExplicitFileList)
                .map(|run| run.reports);
            let receipt = crate::check::checker::project_binding_thread_receipt_for_test();
            let complete_source_receipt = crate::check::checker::library_compiler::complete_source_route_thread_receipt_for_test();
            (reports, receipt, complete_source_receipt)
        })?;
        crate::check::checker::merge_project_binding_thread_receipt_for_test(receipt);
        crate::check::checker::library_compiler::merge_complete_source_route_thread_receipt_for_test(
            complete_source_receipt,
        );
        let reports = reports.map_err(complete_source_driver_error)?;
        test_support::record_reports_exposed_for_test(reports.len());
        Ok(reports)
    }
    #[cfg(not(test))]
    {
        on_standalone_check_worker(|| {
            check_project_once_inner(inputs, ProjectResolutionMode::ExplicitFileList)
                .map(|run| run.reports)
        })?
        .map_err(complete_source_driver_error)
    }
}

/// Check one config-backed Bundler project through the same production lifecycle.
pub fn check_bundler_project_once(
    inputs: Vec<FileInput>,
    project_directory: PathBuf,
    roots: Vec<ProjectRoot>,
) -> Result<ProjectCheckRun, DriverError> {
    if inputs.is_empty() {
        return Ok(ProjectCheckRun {
            reports: Vec::new(),
            inventory: ProjectModuleInventory::default(),
        });
    }
    on_standalone_check_worker(|| {
        check_project_once_inner(
            inputs,
            ProjectResolutionMode::BundlerProject {
                project_directory,
                roots,
            },
        )
    })?
    .map_err(complete_source_driver_error)
}

enum CompleteSourceCheckError {
    Library(LibraryInitError),
    Check(String),
}

fn complete_source_driver_error(error: CompleteSourceCheckError) -> DriverError {
    match error {
        CompleteSourceCheckError::Library(error) => {
            DriverError::LibraryInitialization(Arc::new(error))
        }
        CompleteSourceCheckError::Check(message) => check_failure(message),
    }
}

fn check_project_once_inner(
    inputs: Vec<FileInput>,
    resolution_mode: ProjectResolutionMode,
) -> Result<ProjectCheckRun, CompleteSourceCheckError> {
    let run = match resolution_mode {
        ProjectResolutionMode::ExplicitFileList => {
            run_clean_project_frontend_with_deferred_auxiliary(
                inputs,
                ProjectResolutionMode::ExplicitFileList,
                crate::library::packaged_library_source_inputs,
                move |_, source_specs, library_programs, units, _parse_work| {
                    record_complete_source_parse_work(_parse_work);
                    let injected = injected_library_sources(source_specs);
                    crate::check::checker::library_compiler::compile_complete_source_project_programs(
                        &injected,
                        library_programs,
                        units,
                    )
                },
            )
        }
        ProjectResolutionMode::BundlerProject {
            project_directory,
            roots,
        } => run_clean_bundler_project_frontend_with_default_modules(
            inputs,
            project_directory,
            roots,
            crate::library::packaged_library_source_inputs,
            move |_,
                  source_specs,
                  library_programs,
                  units,
                  source_reexports,
                  default_modules,
                  _parse_work| {
                record_complete_source_parse_work(_parse_work);
                let injected = injected_library_sources(source_specs);
                crate::check::checker::library_compiler::compile_complete_source_project_programs_with_default_modules(
                    &injected,
                    library_programs,
                    units,
                    source_reexports,
                    default_modules,
                )
            },
        ),
    };
    match run.product {
        Err(DeferredProjectFrontendError::Auxiliary(error)) => {
            Err(CompleteSourceCheckError::Library(error))
        }
        Err(DeferredProjectFrontendError::Inventory(error)) => Err(
            CompleteSourceCheckError::Check(format!("project inventory failed: {error}")),
        ),
        Ok(AccountedProjectProduct {
            inventory,
            product: None,
        }) => {
            let mut reports = parse_reports_from_frontend_run(run.inputs, run.parse_errors);
            attach_project_notices(&mut reports, &inventory.notices);
            Ok(ProjectCheckRun { reports, inventory })
        }
        Ok(AccountedProjectProduct {
            product: Some(Err(error)),
            ..
        }) => Err(CompleteSourceCheckError::Check(format!(
            "complete-source project compilation failed: {error:?}"
        ))),
        Ok(AccountedProjectProduct {
            inventory,
            product: Some(Ok(results)),
        }) => {
            let mut reports = project_reports_from_frontend_run(ProjectFrontendRun {
                inputs: run.inputs,
                parse_errors: run.parse_errors,
                product: (
                    results
                        .iter()
                        .map(|result| result.module_ordinal.index())
                        .collect(),
                    Ok(results),
                ),
            })
            .map_err(|message| CompleteSourceCheckError::Check(message.to_owned()))?;
            attach_project_notices(&mut reports, &inventory.notices);
            Ok(ProjectCheckRun { reports, inventory })
        }
    }
}

fn injected_library_sources(
    source_specs: &[AuxiliarySourceInput],
) -> Vec<crate::check::checker::library_compiler::InjectedLibrarySource<'_>> {
    source_specs
        .iter()
        .map(
            |source| crate::check::checker::library_compiler::InjectedLibrarySource {
                file_ordinal: crate::library::LibraryFileOrdinal::new(source.source_ordinal),
                name: &source.name,
                source: &source.source,
            },
        )
        .collect()
}

fn record_complete_source_parse_work(_parse_work: crate::frontend::AuxiliaryParseWork) {
    #[cfg(test)]
    crate::check::checker::library_compiler::record_complete_source_auxiliary_parse_work_for_test(
        _parse_work.parser_invocations,
        _parse_work.source_reparses,
    );
}

fn attach_project_notices(reports: &mut [FileReport], notices: &[String]) {
    if let Some(report) = reports.first_mut() {
        report.output.project_notices.extend_from_slice(notices);
    }
}

/// Stable attestation for the standalone CLI lifecycle.
pub const fn production_cli_route() -> &'static str {
    "production-complete-source-once"
}

fn check_project_inner(
    base: &FrozenLibraryBase,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    check_project_against_library(base, inputs)
}

/// The process-wide default-library base. Publication happens once, on the first caller's thread,
/// and never inside a rayon fan-out.
fn library_base() -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
    static PROVIDER: LazyLock<LibraryBaseProvider> = LazyLock::new(LibraryBaseProvider::new);
    #[cfg(test)]
    if let Some(result) = test_support::acquire_provider_for_test(&PROVIDER) {
        return result;
    }
    PROVIDER.get()
}

/// Stable attestation for every production consumer of the default-library singleton.
pub fn production_library_route() -> Result<&'static str, Arc<LibraryInitError>> {
    let _base = library_base()?;
    Ok("production-default-library")
}

/// Run `work` on a large-stack worker, for the reason given at [`CHECK_STACK_SIZE`]. Spawn and
/// join failures return through the typed driver-infrastructure boundary.
fn on_check_worker<T, W>(
    _base: &Arc<FrozenLibraryBase>,
    work: W,
) -> Result<T, DriverInfrastructureError>
where
    T: Send,
    W: FnOnce() -> T + Send,
{
    on_check_worker_with_identity(Some(Arc::as_ptr(_base).addr()), work)
}

fn on_standalone_check_worker<T, W>(work: W) -> Result<T, DriverInfrastructureError>
where
    T: Send,
    W: FnOnce() -> T + Send,
{
    on_check_worker_with_identity(None, work)
}

fn on_check_worker_with_identity<T, W>(
    _base_identity: Option<usize>,
    work: W,
) -> Result<T, DriverInfrastructureError>
where
    T: Send,
    W: FnOnce() -> T + Send,
{
    #[cfg(test)]
    if test_support::worker_fault_for_test() == Some(DriverInfrastructureKind::WorkerSpawn) {
        return Err(driver_failure(
            DriverInfrastructureKind::WorkerSpawn,
            "injected check-worker spawn failure",
        ));
    }
    #[cfg(test)]
    test_support::record_worker_start_for_test(_base_identity.unwrap_or_default());
    #[cfg(test)]
    let work = {
        let trace_context = test_support::current_trace_context_for_test();
        move || {
            let _trace_enrollment = trace_context
                .as_ref()
                .map(test_support::ProductionDriverFaultTraceContextForTest::enroll);
            work()
        }
    };
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .stack_size(CHECK_STACK_SIZE)
            .spawn_scoped(scope, work)
            .map_err(|error| {
                driver_failure(
                    DriverInfrastructureKind::WorkerSpawn,
                    format!("cannot spawn the check worker: {error}"),
                )
            })?;
        let product = worker.join().map_err(|_| {
            driver_failure(
                DriverInfrastructureKind::WorkerJoin,
                "the check worker terminated unexpectedly",
            )
        })?;
        #[cfg(test)]
        if test_support::worker_fault_for_test() == Some(DriverInfrastructureKind::WorkerJoin) {
            return Err(driver_failure(
                DriverInfrastructureKind::WorkerJoin,
                "injected check-worker join failure",
            ));
        }
        Ok(product)
    })
}

fn check_source_against_library(
    base: &FrozenLibraryBase,
    source: &str,
) -> Result<CheckOutput, String> {
    let route_input = FileInput {
        name: "input.ts".to_owned(),
        source: source.to_owned(),
    };
    match base.route_user_project(std::slice::from_ref(&route_input)) {
        Err(crate::library::LibraryProjectRouteError::UserParseRejected) => {
            Ok(parse_only_source_output(source))
        }
        Err(error) => Err(error.to_string()),
        Ok(RoutedLibraryProject::Shared(state)) => {
            continue_source_with_library_runtime(state, source)
        }
        Ok(RoutedLibraryProject::Private(private)) => {
            #[cfg(test)]
            crate::check::checker::library_compiler::record_private_replay_route_invocation_for_test();
            continue_private_source_with_library(private, source)
        }
        Ok(RoutedLibraryProject::CompleteSourceFallback(fallback)) => {
            #[cfg(test)]
            crate::check::checker::library_compiler::record_private_replay_route_invocation_for_test();
            continue_complete_source_with_library(
                *fallback,
                vec![FileInput {
                    name: "input.ts".to_owned(),
                    source: source.to_owned(),
                }],
            )
            .and_then(single_private_source_output)
        }
    }
}

fn parse_only_source_output(source: &str) -> CheckOutput {
    CheckOutput {
        diagnostics: Vec::new(),
        parse_errors: parse_source_errors(source),
        incomplete: Vec::new(),
        project_notices: Vec::new(),
    }
}

fn continue_private_source_with_library(
    private: RoutedPrivateLibraryProject,
    source: &str,
) -> Result<CheckOutput, String> {
    let reports = continue_private_project_with_library(
        private,
        vec![FileInput {
            name: "input.ts".to_owned(),
            source: source.to_owned(),
        }],
    )?;
    single_private_source_output(reports)
}

fn single_private_source_output(reports: Vec<FileReport>) -> Result<CheckOutput, String> {
    if reports.len() != 1 {
        return Err("private single-source check produced an invalid report count".to_owned());
    }
    reports
        .into_iter()
        .next()
        .map(|report| report.output)
        .ok_or_else(|| "private single-source check produced no report".to_owned())
}

fn continue_source_with_library_runtime(
    state: crate::check::checker::library_compiler::OwnedLibraryRuntimeState,
    source: &str,
) -> Result<CheckOutput, String> {
    let run = run_source_frontend(source, |program| {
        crate::check::checker::check_program_with_owned_library(state, program)
            .map_err(str::to_owned)
    });
    match run.product {
        Some(result) => {
            let CheckResult {
                diagnostics,
                incomplete,
                ..
            } = result?;
            Ok(CheckOutput {
                diagnostics,
                parse_errors: run.parse_errors,
                incomplete,
                project_notices: Vec::new(),
            })
        }
        None => Ok(CheckOutput {
            diagnostics: Vec::new(),
            parse_errors: run.parse_errors,
            incomplete: Vec::new(),
            project_notices: Vec::new(),
        }),
    }
}

fn check_project_against_library(
    base: &FrozenLibraryBase,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    match base.route_user_project(&inputs) {
        Err(crate::library::LibraryProjectRouteError::UserParseRejected) => {
            Ok(parse_only_project_reports(inputs))
        }
        Err(error) => Err(error.to_string()),
        Ok(RoutedLibraryProject::Shared(state)) => {
            continue_project_with_library_runtime(state, inputs)
        }
        Ok(RoutedLibraryProject::Private(private)) => {
            #[cfg(test)]
            crate::check::checker::library_compiler::record_private_replay_route_invocation_for_test();
            continue_private_project_with_library(private, inputs)
        }
        Ok(RoutedLibraryProject::CompleteSourceFallback(fallback)) => {
            #[cfg(test)]
            crate::check::checker::library_compiler::record_private_replay_route_invocation_for_test();
            continue_complete_source_with_library(*fallback, inputs)
        }
    }
}

fn parse_only_project_reports(inputs: Vec<FileInput>) -> Vec<FileReport> {
    let run = run_project_parse_only(inputs);
    parse_reports_from_frontend_run(run.inputs, run.parse_errors)
}

fn parse_reports_from_frontend_run(
    inputs: Vec<FileInput>,
    parse_errors: Vec<Vec<String>>,
) -> Vec<FileReport> {
    debug_assert_eq!(inputs.len(), parse_errors.len());
    inputs
        .into_iter()
        .zip(parse_errors)
        .map(|(input, parse_errors)| FileReport {
            name: input.name,
            source: input.source,
            output: CheckOutput {
                diagnostics: Vec::new(),
                parse_errors,
                incomplete: Vec::new(),
                project_notices: Vec::new(),
            },
        })
        .collect()
}

fn check_one_file_against_library(
    base: &FrozenLibraryBase,
    input: FileInput,
) -> Result<FileReport, String> {
    let mut reports = check_project_against_library(base, vec![input])?;
    if reports.len() != 1 {
        return Err("single-file check produced an invalid report count".to_owned());
    }
    reports
        .pop()
        .ok_or_else(|| "single-file check produced no report".to_owned())
}

fn check_failure(message: String) -> DriverError {
    driver_failure(DriverInfrastructureKind::Check, message).into()
}

fn driver_failure(
    kind: DriverInfrastructureKind,
    message: impl Into<String>,
) -> DriverInfrastructureError {
    DriverInfrastructureError::new(kind, message)
}

fn continue_private_project_with_library(
    private: RoutedPrivateLibraryProject,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    let runtime = match private.into_runtime_or_complete_source_fallback()? {
        RoutedPrivateExecution::Sparse(runtime) => *runtime,
        RoutedPrivateExecution::CompleteSourceFallback(fallback) => {
            return continue_complete_source_with_library(*fallback, inputs);
        }
    };
    let mut state = runtime.state;
    let permit = runtime.permit;
    let fallback_seeds = runtime.fallback_seeds;
    let fallback_inputs = inputs.clone();
    let auxiliary = match state.take_private_collision_sources() {
        Ok(sources) => sources
            .into_iter()
            .map(|source| AuxiliarySourceInput {
                source_ordinal: source.file_ordinal.index(),
                name: source.name,
                source: source.source,
            })
            .collect(),
        Err(_) => {
            let fallback =
                typokat_library::compile_complete_source_fallback_runtime(permit, &fallback_seeds)?;
            return continue_complete_source_with_library(fallback, fallback_inputs);
        }
    };
    match check_private_project_reports(state, inputs, auxiliary) {
        Ok(reports) => Ok(reports),
        Err(_) => {
            let fallback =
                typokat_library::compile_complete_source_fallback_runtime(permit, &fallback_seeds)?;
            continue_complete_source_with_library(fallback, fallback_inputs)
        }
    }
}

fn continue_complete_source_with_library(
    fallback: typokat_library::CompleteSourceFallbackRuntime,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    #[cfg(test)]
    crate::check::checker::library_compiler::record_private_replay_fallback_invocation_for_test();
    let auxiliary = fallback
        .sources
        .iter()
        .cloned()
        .map(|source| AuxiliarySourceInput {
            source_ordinal: source.file_ordinal.index(),
            name: source.name,
            source: source.source,
        })
        .collect();
    check_complete_source_project_reports(fallback, inputs, auxiliary).map_err(str::to_owned)
}

fn check_complete_source_project_reports(
    fallback: typokat_library::CompleteSourceFallbackRuntime,
    inputs: Vec<FileInput>,
    auxiliary: Vec<AuxiliarySourceInput>,
) -> Result<Vec<FileReport>, &'static str> {
    let typokat_library::CompleteSourceFallbackRuntime {
        state,
        checkpoint,
        sources: _,
    } = fallback;
    let run = run_project_frontend_with_auxiliary(
        inputs,
        auxiliary,
        move |_, library_programs, units| {
            let project_units_by_slot = units
                .iter()
                .map(|unit| unit.module_ordinal.index())
                .collect::<Vec<_>>();
            (
                project_units_by_slot,
                crate::check::checker::check_complete_source_project_programs_with_library(
                    state,
                    checkpoint,
                    library_programs,
                    units,
                ),
            )
        },
    );
    project_reports_from_frontend_run(run)
}

fn continue_project_with_library_runtime(
    state: crate::check::checker::library_compiler::OwnedLibraryRuntimeState,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    check_project_reports(inputs, |_, units| {
        crate::check::checker::check_project_programs_with_library(state, units)
    })
    .map_err(str::to_owned)
}

fn check_private_project_reports(
    state: crate::check::checker::library_compiler::OwnedLibraryRuntimeState,
    inputs: Vec<FileInput>,
    auxiliary: Vec<AuxiliarySourceInput>,
) -> Result<Vec<FileReport>, &'static str> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let run = run_project_frontend_with_auxiliary(
        inputs,
        auxiliary,
        move |_, library_programs, units| {
            let project_units_by_slot = units
                .iter()
                .map(|unit| unit.module_ordinal.index())
                .collect::<Vec<_>>();
            (
                project_units_by_slot,
                crate::check::checker::check_private_project_programs_with_library(
                    state,
                    library_programs,
                    units,
                ),
            )
        },
    );
    project_reports_from_frontend_run(run)
}

#[cfg(test)]
fn check_project_inner_with_binding_inspector<F>(
    inputs: Vec<FileInput>,
    inspect: F,
) -> Vec<FileReport>
where
    F: FnOnce(
        &crate::binder::Binder,
        &crate::check::checker::lexical_events::LexicalReservations,
        &[crate::binder::scope::ScopeId],
    ),
{
    check_project_inner_with_checker(inputs, |interner, units| {
        check_project_programs_with_binding_inspector(interner, units, inspect)
    })
}

#[cfg(test)]
fn check_project_inner_with_namespace_value_inspector<F>(
    inputs: Vec<FileInput>,
    inspect: F,
) -> Vec<FileReport>
where
    F: FnOnce(&crate::check::checker::ProjectNamespaceValueInspection),
{
    check_project_inner_with_checker(inputs, |interner, units| {
        check_project_programs_with_namespace_value_inspector(interner, units, inspect)
    })
}

#[cfg(test)]
fn check_project_inner_with_checker<F>(inputs: Vec<FileInput>, check_project: F) -> Vec<FileReport>
where
    F: for<'ast> FnOnce(&mut Interner, &[ProjectProgram<'ast>]) -> Vec<CheckResult>,
{
    check_project_reports(inputs, |interner, units| Ok(check_project(interner, units)))
        .expect("test checker returns complete project result coverage")
}

/// [`check_project_inner_with_checker`] for a checker that can refuse the run outright. A refusal
/// yields `Err` before any [`FileReport`] is built, so no partial user output precedes it.
fn check_project_reports<F>(
    inputs: Vec<FileInput>,
    check_project: F,
) -> Result<Vec<FileReport>, &'static str>
where
    F: for<'ast> FnOnce(
        &mut Interner,
        &[ProjectProgram<'ast>],
    ) -> Result<Vec<CheckResult>, &'static str>,
{
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let run = run_project_frontend(inputs, |interner, units| {
        let project_units_by_slot = units
            .iter()
            .map(|unit| unit.module_ordinal.index())
            .collect::<Vec<_>>();
        (project_units_by_slot, check_project(interner, units))
    });
    project_reports_from_frontend_run(run)
}

type CheckedProjectFrontendProduct = (Vec<usize>, Result<Vec<CheckResult>, &'static str>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectReportAssemblyError {
    InputCoverage,
    ParseCoverage,
    ProjectUnitOutOfRange,
    DuplicateProjectUnit,
    ResultCoverage,
    ResultOrder,
    ResultOutOfRange,
    ResultMisindexed,
    DuplicateResult,
    MissingResult,
}

impl ProjectReportAssemblyError {
    const fn message(self) -> &'static str {
        match self {
            Self::InputCoverage => "project unit coverage does not match the input count",
            Self::ParseCoverage => "project parse coverage does not match the input count",
            Self::ProjectUnitOutOfRange => "project unit references an out-of-range input",
            Self::DuplicateProjectUnit => "project units contain a duplicate input",
            Self::ResultCoverage => "checker result count does not match the input count",
            Self::ResultOrder => "checker results are not in dependency order",
            Self::ResultOutOfRange => "checker result references an out-of-range input",
            Self::ResultMisindexed => "checker result does not match its project unit",
            Self::DuplicateResult => "checker results contain a duplicate input",
            Self::MissingResult => "checker results do not cover every input",
        }
    }
}

fn project_reports_from_frontend_run(
    run: ProjectFrontendRun<CheckedProjectFrontendProduct>,
) -> Result<Vec<FileReport>, &'static str> {
    let ProjectFrontendRun {
        inputs,
        parse_errors,
        product: (project_units_by_slot, ordered_results),
    } = run;
    let ordered_results = ordered_results?;
    assemble_project_reports(inputs, parse_errors, project_units_by_slot, ordered_results)
        .map_err(ProjectReportAssemblyError::message)
}

fn assemble_project_reports(
    inputs: Vec<FileInput>,
    parse_errors: Vec<Vec<String>>,
    project_units_by_slot: Vec<usize>,
    ordered_results: Vec<CheckResult>,
) -> Result<Vec<FileReport>, ProjectReportAssemblyError> {
    let input_count = inputs.len();
    if project_units_by_slot.len() != input_count {
        return Err(ProjectReportAssemblyError::InputCoverage);
    }
    if parse_errors.len() != input_count {
        return Err(ProjectReportAssemblyError::ParseCoverage);
    }

    let mut expected_inputs = vec![false; input_count];
    for &original in &project_units_by_slot {
        let Some(seen) = expected_inputs.get_mut(original) else {
            return Err(ProjectReportAssemblyError::ProjectUnitOutOfRange);
        };
        if std::mem::replace(seen, true) {
            return Err(ProjectReportAssemblyError::DuplicateProjectUnit);
        }
    }
    if ordered_results.len() != input_count {
        return Err(ProjectReportAssemblyError::ResultCoverage);
    }

    let mut channels_by_original: Vec<Option<(Vec<Diagnostic>, Vec<IncompleteSurface>)>> =
        (0..input_count).map(|_| None).collect();
    for (ordered_slot, result) in ordered_results.into_iter().enumerate() {
        let original = result.module_ordinal.index();
        if result.unit_slot.index() != ordered_slot {
            return Err(ProjectReportAssemblyError::ResultOrder);
        }
        if original >= input_count {
            return Err(ProjectReportAssemblyError::ResultOutOfRange);
        }
        if project_units_by_slot.get(ordered_slot).copied() != Some(original) {
            return Err(ProjectReportAssemblyError::ResultMisindexed);
        }
        let Some(slot) = channels_by_original.get_mut(original) else {
            return Err(ProjectReportAssemblyError::ResultOutOfRange);
        };
        if slot.is_some() {
            return Err(ProjectReportAssemblyError::DuplicateResult);
        }
        *slot = Some((result.diagnostics, result.incomplete));
    }

    inputs
        .into_iter()
        .zip(parse_errors)
        .zip(channels_by_original)
        .map(|((input, parse_errors), channels)| {
            let Some((diagnostics, incomplete)) = channels else {
                return Err(ProjectReportAssemblyError::MissingResult);
            };
            Ok(FileReport {
                name: input.name,
                source: input.source,
                output: CheckOutput {
                    diagnostics,
                    parse_errors,
                    incomplete,
                    project_notices: Vec::new(),
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod test_support {
    use super::*;
    use std::cell::RefCell;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) enum ProductionDriverFaultForTest {
        None,
        ProviderInitialization(String),
        WorkerSpawn,
        WorkerJoin,
        ProviderTrace(ProviderTraceFaultForTest),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum ProviderTraceFaultForTest {
        AcquireInsideRayon,
        ReplaceOneWorkerBase,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum DriverFailureKindForTest {
        WorkerSpawn,
        WorkerJoin,
        Other,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct ProductionDriverTraceForTest {
        pub provider_initialization_attempts: usize,
        pub provider_publications: usize,
        pub provider_acquisitions: usize,
        pub worker_starts: usize,
        pub rayon_worker_starts: usize,
        pub reports_exposed: usize,
        pub provider_acquired_before_rayon: bool,
        pub provider_instance_identity: usize,
        pub published_base_identity: usize,
        pub worker_base_identities: Vec<usize>,
    }

    struct ActiveScope {
        fault: ProductionDriverFaultForTest,
        provider_result: OnceLock<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>>,
        trace: Mutex<ProductionDriverTraceForTest>,
    }

    static SCOPE_GATE: Mutex<()> = Mutex::new(());
    thread_local! {
        static ENROLLED_SCOPE: RefCell<Option<Arc<ActiveScope>>> = const { RefCell::new(None) };
    }

    #[derive(Clone)]
    pub(super) struct ProductionDriverFaultTraceContextForTest {
        active: Arc<ActiveScope>,
    }

    pub(super) struct ProductionDriverFaultTraceEnrollmentForTest {
        previous: Option<Arc<ActiveScope>>,
    }

    impl ProductionDriverFaultTraceContextForTest {
        pub(super) fn enroll(&self) -> ProductionDriverFaultTraceEnrollmentForTest {
            let previous = ENROLLED_SCOPE.with(|scope| scope.replace(Some(self.active.clone())));
            ProductionDriverFaultTraceEnrollmentForTest { previous }
        }

        pub(super) fn run<Output>(&self, work: impl FnOnce() -> Output) -> Output {
            let _enrollment = self.enroll();
            work()
        }
    }

    impl Drop for ProductionDriverFaultTraceEnrollmentForTest {
        fn drop(&mut self) {
            ENROLLED_SCOPE.with(|scope| {
                scope.replace(self.previous.take());
            });
        }
    }

    pub(super) struct ProductionDriverFaultTraceScopeForTest {
        active: Arc<ActiveScope>,
        _owner_enrollment: ProductionDriverFaultTraceEnrollmentForTest,
        _gate: MutexGuard<'static, ()>,
    }

    impl ProductionDriverFaultTraceScopeForTest {
        pub(super) fn install(fault: ProductionDriverFaultForTest) -> Self {
            let gate = lock(&SCOPE_GATE);
            let active = Arc::new(ActiveScope {
                fault,
                provider_result: OnceLock::new(),
                trace: Mutex::new(ProductionDriverTraceForTest::default()),
            });
            let owner_context = ProductionDriverFaultTraceContextForTest {
                active: active.clone(),
            };
            Self {
                active,
                _owner_enrollment: owner_context.enroll(),
                _gate: gate,
            }
        }

        pub(super) fn finish(self) -> ProductionDriverTraceForTest {
            let trace = lock(&self.active.trace).clone();
            drop(self);
            trace
        }

        pub(super) fn context(&self) -> ProductionDriverFaultTraceContextForTest {
            ProductionDriverFaultTraceContextForTest {
                active: self.active.clone(),
            }
        }
    }

    pub(super) fn current_trace_context_for_test(
    ) -> Option<ProductionDriverFaultTraceContextForTest> {
        active_scope().map(|active| ProductionDriverFaultTraceContextForTest { active })
    }

    pub(super) fn acquire_provider_for_test(
        provider: &'static LazyLock<LibraryBaseProvider>,
    ) -> Option<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>> {
        let active = active_scope()?;
        {
            let mut trace = lock(&active.trace);
            trace.provider_acquisitions += 1;
            trace.provider_instance_identity = std::ptr::from_ref(&**provider).addr();
        }
        Some(
            active
                .provider_result
                .get_or_init(|| {
                    lock(&active.trace).provider_initialization_attempts += 1;
                    if let ProductionDriverFaultForTest::ProviderInitialization(message) =
                        &active.fault
                    {
                        return Err(Arc::new(LibraryInitError::injected_initialization_failure(
                            message.clone(),
                        )));
                    }
                    let base = provider.get()?;
                    let mut trace = lock(&active.trace);
                    trace.provider_publications += 1;
                    trace.published_base_identity = Arc::as_ptr(&base).addr();
                    Ok(base)
                })
                .clone(),
        )
    }

    pub(super) fn worker_fault_for_test() -> Option<DriverInfrastructureKind> {
        match active_scope().as_deref().map(|scope| &scope.fault) {
            Some(ProductionDriverFaultForTest::WorkerSpawn) => {
                Some(DriverInfrastructureKind::WorkerSpawn)
            }
            Some(ProductionDriverFaultForTest::WorkerJoin) => {
                Some(DriverInfrastructureKind::WorkerJoin)
            }
            Some(
                ProductionDriverFaultForTest::None
                | ProductionDriverFaultForTest::ProviderInitialization(_)
                | ProductionDriverFaultForTest::ProviderTrace(_),
            )
            | None => None,
        }
    }

    pub(super) fn record_provider_acquired_before_rayon_for_test() {
        let Some(active) = active_scope() else {
            return;
        };
        let acquired_before = !matches!(
            active.fault,
            ProductionDriverFaultForTest::ProviderTrace(
                ProviderTraceFaultForTest::AcquireInsideRayon
            )
        );
        lock(&active.trace).provider_acquired_before_rayon = acquired_before;
    }

    pub(super) fn record_rayon_worker_start_for_test() {
        if let Some(active) = active_scope() {
            lock(&active.trace).rayon_worker_starts += 1;
        }
    }

    pub(super) fn record_worker_start_for_test(base_identity: usize) {
        let Some(active) = active_scope() else {
            return;
        };
        let mut trace = lock(&active.trace);
        trace.worker_starts += 1;
        let replace = matches!(
            active.fault,
            ProductionDriverFaultForTest::ProviderTrace(
                ProviderTraceFaultForTest::ReplaceOneWorkerBase
            )
        ) && trace.worker_base_identities.is_empty();
        trace.worker_base_identities.push(if replace {
            base_identity.wrapping_add(1)
        } else {
            base_identity
        });
    }

    pub(super) fn record_reports_exposed_for_test(count: usize) {
        if let Some(active) = active_scope() {
            lock(&active.trace).reports_exposed += count;
        }
    }

    pub(super) fn classify_driver_failure_for_test(
        error: &DriverError,
    ) -> DriverFailureKindForTest {
        match error {
            DriverError::Infrastructure(error)
                if error.kind() == DriverInfrastructureKind::WorkerSpawn =>
            {
                DriverFailureKindForTest::WorkerSpawn
            }
            DriverError::Infrastructure(error)
                if error.kind() == DriverInfrastructureKind::WorkerJoin =>
            {
                DriverFailureKindForTest::WorkerJoin
            }
            _ => DriverFailureKindForTest::Other,
        }
    }

    fn active_scope() -> Option<Arc<ActiveScope>> {
        ENROLLED_SCOPE.with(|scope| scope.borrow().clone())
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod project_report_coverage_spec;

#[cfg(test)]
mod wu6_benchmark_route_spec;

#[cfg(test)]
mod wu7_provider_lifecycle_spec;

#[cfg(test)]
mod tests {
    use super::*;

    fn check_source(source: &str) -> CheckOutput {
        super::check_source(source).expect("production default library initializes")
    }

    fn check_project(inputs: Vec<FileInput>) -> Vec<FileReport> {
        super::check_project(inputs).expect("production default library initializes")
    }

    fn check_files(inputs: Vec<FileInput>) -> Vec<FileReport> {
        super::check_files(inputs).expect("production default library initializes")
    }

    /// Diagnostics derive `Debug` but not `PartialEq`, so compare their debug
    /// renderings — enough to assert two checks produced the *same* diagnostics.
    fn debug_diags(output: &CheckOutput) -> String {
        format!("{:?}", output.diagnostics)
    }

    #[test]
    fn production_library_nested_set_inference_keeps_string_candidate() {
        let output = check_source(
            "declare const values: Set<Set<string>>;\n\
             declare function take<T>(values: Iterable<Set<T>>): T[];\n\
             const result: string[] = take(values);\n",
        );

        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn production_library_warm_set_inference_keeps_both_string_candidates() {
        let output = check_source(
            "declare const values: Set<string>;\n\
             declare const nested: Set<Set<string>>;\n\
             declare function take<T>(values: Iterable<T>): T[];\n\
             declare function takeNested<T>(values: Iterable<Set<T>>): T[];\n\
             const firstClean: string[] = take(values);\n\
             const firstWrong: number[] = take(values);\n\
             const nestedClean: string[] = takeNested(nested);\n\
             const nestedWrong: number[] = takeNested(nested);\n",
        );

        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            vec!["TK2322", "TK2322"],
            "warm inference must not replace either string candidate with unknown"
        );
    }

    #[test]
    fn production_library_promise_then_keeps_callback_result() {
        let output = check_source(
            "const directWrong: Promise<string> = Promise.resolve(1);\n\
             const incremented = Promise.resolve(1).then((value) => value + 1);\n\
             const correct: Promise<number> = incremented;\n\
             const thenWrong: Promise<string> = incremented;\n",
        );

        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            vec!["TK2322", "TK2322"],
            "Promise.then must preserve the callback TResult candidate"
        );
    }

    /// Pins the contract that parallel multi-file checking is per-file-independent
    /// and order-preserving.
    #[test]
    fn check_files_matches_per_file_check_source_in_order() {
        let clean = "const x: number = 1;";
        let type_err = "const y: number = \"nope\";";
        let parse_err = "const z: number = ;";

        let inputs = vec![
            FileInput {
                name: "a.ts".into(),
                source: clean.into(),
            },
            FileInput {
                name: "b.ts".into(),
                source: type_err.into(),
            },
            FileInput {
                name: "c.ts".into(),
                source: parse_err.into(),
            },
        ];

        let reports = check_files(inputs);

        assert_eq!(reports.len(), 3);
        // Order preserved.
        assert_eq!(reports[0].name, "a.ts");
        assert_eq!(reports[1].name, "b.ts");
        assert_eq!(reports[2].name, "c.ts");

        // Each file's result equals checking that source on its own — including the
        // parse-error file, whatever oxc's recovery does with it.
        for (report, source) in reports.iter().zip([clean, type_err, parse_err]) {
            let solo = check_source(source);
            assert_eq!(debug_diags(&report.output), debug_diags(&solo));
            assert_eq!(report.output.parse_errors, solo.parse_errors);
            assert_eq!(report.source, source);
        }

        // The clean file is clean; the type-error file reports a problem.
        assert!(!reports[0].output.has_errors());
        assert!(reports[1].output.has_errors());
    }

    /// The same inputs checked twice yield identical diagnostics — the per-file
    /// interners and order-preserving collect leave no room for worker-scheduling
    /// nondeterminism to leak in.
    #[test]
    fn check_files_is_deterministic() {
        let sources = [
            "const a: string = 1;",
            "let b = 2; b = \"x\";",
            "type T = number; const c: T = 3;",
        ];
        let build = || {
            sources
                .iter()
                .map(|s| FileInput {
                    name: "f.ts".into(),
                    source: (*s).into(),
                })
                .collect::<Vec<_>>()
        };

        let first = check_files(build());
        let second = check_files(build());

        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(debug_diags(&a.output), debug_diags(&b.output));
            assert_eq!(a.output.parse_errors, b.output.parse_errors);
        }
    }

    /// WU3 / backlog 63k — the annotation nesting budget fires gracefully: a type
    /// literal nested past `MAX_ANNOTATION_DEPTH` reports `TK2589` instead of overflowing
    /// the native stack, while a shallow one lowers cleanly. A very deep witness (past the
    /// oxc parser's own stack limit on a default stack) still returns a diagnostic — the
    /// large-stack worker (`check_source`) keeps the parse alive to reach the budget.
    #[test]
    fn deep_annotation_reports_tk2589_without_overflow() {
        let deep = |depth: usize| {
            format!(
                "type Deep = {}number{};",
                "{ a: ".repeat(depth),
                " }".repeat(depth)
            )
        };
        // Shallow: no depth diagnostic.
        let shallow = check_source(&deep(5));
        assert!(
            !codes(&shallow).contains(&"TK2589"),
            "a shallow annotation must not trip the budget"
        );
        // Just past the budget: TK2589, no crash.
        let over = check_source(&deep(400));
        assert!(
            codes(&over).contains(&"TK2589"),
            "an over-budget annotation reports TK2589"
        );
        // Far past the parser's default-stack limit: still a controlled diagnostic.
        let very_deep = check_source(&deep(4000));
        assert!(
            codes(&very_deep).contains(&"TK2589"),
            "a pathologically deep annotation is bounded, not a native overflow"
        );
    }

    /// The diagnostic codes emitted for `source`, in order.
    fn codes(output: &CheckOutput) -> Vec<&'static str> {
        output.diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn annotated_initializer_assignment_replays_at_declaration_name_span() {
        use std::cell::RefCell;

        let source = "const value: number = \"wrong\";";
        let assignment_keys = RefCell::new(Vec::new());
        let reports = check_project_inner_with_namespace_value_inspector(
            vec![FileInput {
                name: "input.ts".into(),
                source: source.into(),
            }],
            |inspection| {
                *assignment_keys.borrow_mut() = inspection
                    .replay
                    .iter()
                    .filter_map(|record| match &record.record {
                        crate::check::checker::ProjectReplayRecordInspection::Diagnostic(code)
                            if code == "TK2322" =>
                        {
                            Some(record.key.source_start)
                        }
                        crate::check::checker::ProjectReplayRecordInspection::Diagnostic(_)
                        | crate::check::checker::ProjectReplayRecordInspection::Incomplete(_) => {
                            None
                        }
                    })
                    .collect();
            },
        );

        assert_eq!(reports.len(), 1);
        assert!(reports[0].output.parse_errors.is_empty());
        assert!(reports[0].output.incomplete.is_empty());
        let [diagnostic] = reports[0].output.diagnostics.as_slice() else {
            panic!("expected one initializer assignment diagnostic");
        };
        assert_eq!(diagnostic.code.as_str(), "TK2322");
        assert_eq!(
            assignment_keys.into_inner(),
            [
                u32::try_from(source.find("\"wrong\"").expect("initializer literal"))
                    .expect("source offset fits u32")
            ],
            "event ownership remains attached to the initializer",
        );
        assert_eq!(
            diagnostic.span.start,
            u32::try_from(source.find("value").expect("declaration name"))
                .expect("source offset fits u32"),
        );
    }

    #[test]
    fn annotated_object_binding_initializer_reports_at_offending_property() {
        let source = "const { value }: { value: string } = { value: 1 };";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("expected one object-binding initializer diagnostic");
        };
        assert_eq!(diagnostic.code.as_str(), "TK2322");
        assert_eq!(
            diagnostic.span.start,
            u32::try_from(source.rfind("value").expect("initializer property"))
                .expect("source offset fits u32"),
        );
    }

    #[test]
    fn annotated_array_binding_initializer_reports_at_offending_element() {
        let source = "const [value]: [string] = [1];";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("expected one array-binding initializer diagnostic");
        };
        assert_eq!(diagnostic.code.as_str(), "TK2322");
        assert_eq!(
            diagnostic.span.start,
            u32::try_from(source.find('1').expect("initializer element"))
                .expect("source offset fits u32"),
        );
    }

    #[test]
    fn annotated_class_field_initializer_reports_at_field_name() {
        let source = "class Example { field: number = \"wrong\"; }";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("expected one class-field initializer diagnostic");
        };
        assert_eq!(diagnostic.code.as_str(), "TK2322");
        assert_eq!(
            diagnostic.span.start,
            u32::try_from(source.find("field").expect("field name"))
                .expect("source offset fits u32"),
        );
    }

    #[test]
    fn annotated_parameter_default_reports_at_parameter_name() {
        let source = "function example(value: number = \"wrong\"): void {}";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        let [diagnostic] = output.diagnostics.as_slice() else {
            panic!("expected one parameter-default initializer diagnostic");
        };
        assert_eq!(diagnostic.code.as_str(), "TK2322");
        assert_eq!(
            diagnostic.span.start,
            u32::try_from(source.find("value").expect("parameter name"))
                .expect("source offset fits u32"),
        );
    }

    /// Backlog 74 review regression: signature diagnostics discovered during the
    /// reservation prepass still render at their declaration's source position.
    #[test]
    fn function_reservation_preserves_diagnostic_order() {
        let out = check_source(
            "missingBefore;\n\
             function late(value: Missing): void {}\n\
             missingAfter;",
        );
        assert_eq!(codes(&out), vec!["TK2304", "TK2304", "TK2304"]);
        assert!(
            out.diagnostics
                .windows(2)
                .all(|pair| pair[0].span.start < pair[1].span.start),
            "diagnostics must remain in source order: {:?}",
            out.diagnostics
        );
    }

    /// WU2 local overloads: a signature-only local declaration must NOT fire the
    /// spurious `TK2391`, and calls resolve against the *declared* overloads
    /// (a non-matching argument is `TK2769`, tsc's `TS2769`).
    #[test]
    fn local_overload_set_groups_and_selects_declared_signatures() {
        let out = check_source(
            "function outer(): void {\n\
               function ov(x: number): number;\n\
               function ov(x: string): string;\n\
               function ov(x: number | string): number | string { return x; }\n\
               const okNum: number = ov(1);\n\
               const okStr: string = ov(\"a\");\n\
               ov(true);\n\
             }",
        );
        // Only the bad call reports, and it reports the overload code — no TK2391,
        // no spurious TK2322 on the well-typed calls.
        assert_eq!(codes(&out), vec!["TK2769"]);
    }

    /// A local overload set inside a nested `{ }` block groups the same way.
    #[test]
    fn local_overload_nested_in_block() {
        let out = check_source(
            "function outer(): void {\n\
               {\n\
                 function ov(x: number): number;\n\
                 function ov(x: string): string;\n\
                 function ov(x: number | string): number | string { return x; }\n\
                 const okNum: number = ov(1);\n\
                 ov(true);\n\
               }\n\
             }",
        );
        assert_eq!(codes(&out), vec!["TK2769"]);
    }

    /// A local overload set inside a loop body (WU1 added loop-body walkers) also
    /// routes through the shared grouping walker.
    #[test]
    fn local_overload_nested_in_loop_body() {
        let out = check_source(
            "function outer(): void {\n\
               for (let i = 0; i < 1; i++) {\n\
                 function ov(x: number): number;\n\
                 function ov(x: string): string;\n\
                 function ov(x: number | string): number | string { return x; }\n\
                 const okNum: number = ov(1);\n\
                 ov(true);\n\
               }\n\
             }",
        );
        assert_eq!(codes(&out), vec!["TK2769"]);
    }

    /// Regression control: top-level overloads keep working unchanged.
    #[test]
    fn top_level_overloads_regression_control() {
        let out = check_source(
            "function ov(x: number): number;\n\
             function ov(x: string): string;\n\
             function ov(x: number | string): number | string { return x; }\n\
             const okNum: number = ov(1);\n\
             const okStr: string = ov(\"a\");\n\
             ov(true);",
        );
        assert_eq!(codes(&out), vec!["TK2769"]);
    }

    /// WU2 export space: a type-only specifier export (`export type { C }` /
    /// `export { type v }`) suppresses the value slot — a plain import cannot use
    /// the name as a runtime value (M29 stand-in for TS1362 is `TK2304`) — while
    /// the type side still resolves.
    #[test]
    fn type_only_specifier_export_suppresses_value_slot() {
        let files = vec![
            FileInput {
                name: "a.ts".into(),
                source: "class C {}\n\
                         export type { C };\n\
                         const v = 1;\n\
                         export { type v };"
                    .into(),
            },
            FileInput {
                name: "use.ts".into(),
                source: "import { C, v } from \"./a\";\n\
                         const asType: C = new C();\n\
                         const asVal: number = v;"
                    .into(),
            },
        ];
        let reports = check_project(files);
        // The exporter is clean (a value-only local is a valid `export type` target).
        assert!(reports[0].output.diagnostics.is_empty(), "exporter clean");
        // The importer: `new C()` (value use of a type-only export) and `v`
        // (value use of a type-only export) each miss the value slot → TK2304.
        // `C` as a *type* still resolves, so no TK2304 there.
        assert_eq!(codes(&reports[1].output), vec!["TK2304", "TK2304"]);
    }

    /// Control: a regular `export { x }` still provides both the value and type
    /// slots — value and type uses both resolve.
    #[test]
    fn regular_specifier_export_provides_both_slots() {
        let files = vec![
            FileInput {
                name: "a.ts".into(),
                source: "class C {}\n\
                         export { C };"
                    .into(),
            },
            FileInput {
                name: "use.ts".into(),
                source: "import { C } from \"./a\";\n\
                         const asType: C = new C();"
                    .into(),
            },
        ];
        let reports = check_project(files);
        assert!(reports[0].output.diagnostics.is_empty());
        assert!(
            reports[1].output.diagnostics.is_empty(),
            "value + type both resolve: {:?}",
            codes(&reports[1].output)
        );
    }

    #[test]
    fn project_results_follow_input_order_not_dependency_order() {
        let reports = check_project(vec![
            FileInput {
                name: "use.ts".into(),
                source: "import { x } from './dep'; missingUse; const ok: number = x;".into(),
            },
            FileInput {
                name: "dep.ts".into(),
                source: "export const x = 1; const bad: number = 'bad';".into(),
            },
        ]);

        assert_eq!(reports[0].name, "use.ts");
        assert_eq!(codes(&reports[0].output), ["TK2304"]);
        assert_eq!(reports[1].name, "dep.ts");
        assert_eq!(codes(&reports[1].output), ["TK2322"]);
    }

    #[test]
    fn project_imports_attach_to_preallocated_local_declarations_in_dependency_order() {
        fn files(use_first: bool) -> Vec<FileInput> {
            let use_file = FileInput {
                name: "use.ts".into(),
                source: "import { value as first, value as second, type Both as TypeOnly } from './dep'; import { Missing as MissingLocal, Other as OtherMissing } from './absent'; const one: number = first; const two: number = second; const typed: TypeOnly = new TypeOnly();".into(),
            };
            let dep_file = FileInput {
                name: "dep.ts".into(),
                source: "export class Both {} export const value = 1;".into(),
            };
            if use_first {
                vec![use_file, dep_file]
            } else {
                vec![dep_file, use_file]
            }
        }

        fn inspect(
            binder: &crate::binder::Binder,
            reservations: &crate::check::checker::lexical_events::LexicalReservations,
            scopes: &[crate::binder::scope::ScopeId],
        ) {
            let dep_scope = scopes[0];
            let use_scope = scopes[1];
            let source = "import { value as first, value as second, type Both as TypeOnly } from './dep'; import { Missing as MissingLocal, Other as OtherMissing } from './absent'; const one: number = first; const two: number = second; const typed: TypeOnly = new TypeOnly();";
            let imports: Vec<_> = binder
                .declarations
                .iter()
                .filter(|declaration| {
                    declaration.site.module == use_scope
                        && declaration.kind == crate::binder::declaration::DeclarationKind::Import
                })
                .collect();
            assert_eq!(imports.len(), 5);
            assert!(imports
                .iter()
                .enumerate()
                .all(|(index, declaration)| imports
                    .iter()
                    .skip(index + 1)
                    .all(|other| declaration.id != other.id)));
            assert_eq!(
                imports
                    .iter()
                    .map(|declaration| &source[declaration.site.declaration_span.range()])
                    .collect::<Vec<_>>(),
                vec![
                    "import { value as first, value as second, type Both as TypeOnly } from './dep';",
                    "import { value as first, value as second, type Both as TypeOnly } from './dep';",
                    "import { value as first, value as second, type Both as TypeOnly } from './dep';",
                    "import { Missing as MissingLocal, Other as OtherMissing } from './absent';",
                    "import { Missing as MissingLocal, Other as OtherMissing } from './absent';",
                ]
            );
            assert_eq!(
                imports
                    .iter()
                    .map(|declaration| &source[declaration.site.binding_span.range()])
                    .collect::<Vec<_>>(),
                vec![
                    "first",
                    "second",
                    "TypeOnly",
                    "MissingLocal",
                    "OtherMissing"
                ]
            );
            assert!(imports
                .iter()
                .all(|declaration| declaration.site.scope == Some(use_scope)));

            let symbol = |scope, name: &str| {
                binder
                    .graph
                    .get(scope)
                    .and_then(|scope| scope.lookup_local(name))
                    .and_then(|symbol| binder.symbols.get(symbol))
                    .expect("project symbol")
            };
            let remote_value = symbol(dep_scope, "value")
                .value
                .expect("exported value storage");
            assert_eq!(imports[0].value_storage, Some(remote_value));
            assert_eq!(imports[1].value_storage, Some(remote_value));
            assert_eq!(symbol(use_scope, "first").value, Some(remote_value));
            assert_eq!(symbol(use_scope, "second").value, Some(remote_value));
            assert_eq!(symbol(use_scope, "first").declarations, vec![imports[0].id]);
            assert_eq!(
                symbol(use_scope, "second").declarations,
                vec![imports[1].id]
            );

            let remote_type = symbol(dep_scope, "Both").ty.expect("exported type storage");
            assert_eq!(imports[2].value_storage, None);
            assert_eq!(imports[2].type_group, Some(remote_type));
            assert_eq!(symbol(use_scope, "TypeOnly").value, None);
            assert_eq!(symbol(use_scope, "TypeOnly").ty, Some(remote_type));
            assert!(symbol(use_scope, "TypeOnly").blocks_value_lookup);

            assert!(imports[3].value_storage.is_some());
            assert!(imports[3].type_group.is_none());
            assert!(imports[4].value_storage.is_some());
            assert!(imports[4].type_group.is_none());
            assert_ne!(imports[3].value_storage, imports[4].value_storage);
            assert_eq!(
                symbol(use_scope, "MissingLocal").value,
                imports[3].value_storage
            );
            assert_eq!(symbol(use_scope, "MissingLocal").ty, imports[3].type_group);
            assert_eq!(
                symbol(use_scope, "OtherMissing").value,
                imports[4].value_storage
            );
            assert_eq!(symbol(use_scope, "OtherMissing").ty, imports[4].type_group);

            let owners: Vec<_> = imports
                .iter()
                .map(|declaration| {
                    let reservation = reservations
                        .declaration_reservation(declaration.id)
                        .expect("exact project import reservation");
                    assert_eq!(
                        reservation.declaration_span,
                        declaration.site.declaration_span
                    );
                    assert_eq!(reservation.binding_span, declaration.site.binding_span);
                    reservations
                        .declaration_owner(declaration.id)
                        .expect("project import owner")
                })
                .collect();
            assert!(owners.iter().enumerate().all(|(index, owner)| owners
                .iter()
                .skip(index + 1)
                .all(|other| owner.ticket != other.ticket)));
        }

        let use_first = check_project_inner_with_binding_inspector(files(true), inspect);
        let dep_first = check_project_inner_with_binding_inspector(files(false), inspect);
        for name in ["use.ts", "dep.ts"] {
            let first = use_first
                .iter()
                .find(|report| report.name == name)
                .expect("first-order report");
            let second = dep_first
                .iter()
                .find(|report| report.name == name)
                .expect("opposite-order report");
            assert_eq!(debug_diags(&first.output), debug_diags(&second.output));
            assert_eq!(first.output.parse_errors, second.output.parse_errors);
            assert_eq!(first.output.incomplete, second.output.incomplete);
        }
    }

    #[test]
    fn project_namespace_metadata_uses_one_canonical_global_and_path_stable_projection() {
        fn files(reverse_input: bool) -> Vec<FileInput> {
            let a = FileInput {
                name: "types/a.d.ts".into(),
                source: "import type { Z } from './z.d.ts'; export {}; export as namespace UmdA; interface Shared { localA: true } declare global { interface Shared { globalA: true } namespace GlobalN { interface PrivateA {} export { PrivateA as AliasA }; export interface PublicA {} } } declare module 'pkg-a' { export {}; global { interface Shared { moduleA: true } } }"
                    .into(),
            };
            let z = FileInput {
                name: "types/z.d.ts".into(),
                source: "import type {} from './a.d.ts'; export as namespace UmdZ; export interface Z {} interface Shared { localZ: true } declare global { interface Shared { globalZ: true } namespace GlobalN { interface PrivateZ {} export { PrivateZ as AliasZ }; export interface PublicZ {} } } declare module 'pkg-z' { export {}; global { interface Shared { moduleZ: true } } }"
                    .into(),
            };
            if reverse_input {
                vec![z, a]
            } else {
                vec![a, z]
            }
        }

        fn inspect(
            binder: &crate::binder::Binder,
            reservations: &crate::check::checker::lexical_events::LexicalReservations,
            scopes: &[crate::binder::scope::ScopeId],
        ) -> Vec<String> {
            use crate::binder::namespace::{
                DeclarationOwner, DeferredModuleId, ExportContextOwner, GlobalAugmentationId,
                GlobalOwner, NamespaceFragmentId, NamespaceId, NamespaceMemberOwner,
                NamespaceOwner, SourceUnitKey,
            };
            use crate::binder::scope::{ScopeId, ScopeKind};
            use crate::binder::symbol::SymbolId;

            fn namespace_identity(binder: &crate::binder::Binder, id: NamespaceId) -> String {
                let namespace = binder.namespaces.get(id).expect("namespace identity");
                let fragments = namespace
                    .fragments
                    .iter()
                    .filter_map(|fragment| binder.namespaces.fragment(*fragment))
                    .map(|fragment| (fragment.source, fragment.source_start))
                    .collect::<Vec<_>>();
                format!("{}@{fragments:?}", namespace.name)
            }

            fn fragment_identity(
                binder: &crate::binder::Binder,
                id: NamespaceFragmentId,
            ) -> String {
                let fragment = binder.namespaces.fragment(id).expect("fragment identity");
                format!(
                    "{}#{:?}:{}",
                    namespace_identity(binder, fragment.namespace),
                    fragment.source,
                    fragment.source_start
                )
            }

            fn global_identity(binder: &crate::binder::Binder, id: GlobalAugmentationId) -> String {
                let global = binder
                    .namespaces
                    .globals()
                    .find(|global| global.id == id)
                    .expect("global identity");
                format!("{:?}:{}", global.source, global.diagnostic_span.start)
            }

            fn deferred_identity(binder: &crate::binder::Binder, id: DeferredModuleId) -> String {
                let module = binder
                    .namespaces
                    .deferred_modules()
                    .find(|module| module.id == id)
                    .expect("deferred module identity");
                format!(
                    "{:?}:{}:{}",
                    module.source, module.span.start, module.specifier
                )
            }

            fn scope_identity(binder: &crate::binder::Binder, id: ScopeId) -> String {
                if id == binder.prelude_module {
                    return "prelude".to_string();
                }
                if id == binder.compilation_global {
                    return "compilation-global".to_string();
                }
                if let Some(global) = binder
                    .namespaces
                    .globals()
                    .find(|global| global.overlay_scope == id)
                {
                    return format!(
                        "global-overlay:{:?}:{}",
                        global.source, global.diagnostic_span.start
                    );
                }
                if let Some(unit) = binder
                    .namespaces
                    .source_units()
                    .find(|unit| unit.module == id)
                {
                    return format!("root:{:?}", unit.source);
                }
                for namespace in binder.namespaces.namespaces() {
                    if namespace.public_scope == id {
                        return format!("public:{}", namespace_identity(binder, namespace.id));
                    }
                    for fragment in &namespace.fragments {
                        let fragment_record = binder
                            .namespaces
                            .fragment(*fragment)
                            .expect("canonical namespace fragment");
                        if fragment_record.private_scope == id {
                            return format!("private:{}", fragment_identity(binder, *fragment));
                        }
                    }
                }
                panic!("scope outside the WU1b canonical projection")
            }

            fn namespace_owner_identity(
                binder: &crate::binder::Binder,
                owner: NamespaceOwner,
            ) -> String {
                match owner {
                    NamespaceOwner::Lexical(scope) => scope_identity(binder, scope),
                    NamespaceOwner::NamespacePublic(namespace) => {
                        format!("public:{}", namespace_identity(binder, namespace))
                    }
                    NamespaceOwner::FragmentPrivate(fragment) => {
                        format!("private:{}", fragment_identity(binder, fragment))
                    }
                    NamespaceOwner::CompilationGlobal => "compilation-global".to_string(),
                }
            }

            fn declaration_owner_identity(
                binder: &crate::binder::Binder,
                owner: DeclarationOwner,
            ) -> String {
                match owner {
                    DeclarationOwner::Lexical(scope) => scope_identity(binder, scope),
                    DeclarationOwner::NamespacePublic(namespace) => {
                        format!("public:{}", namespace_identity(binder, namespace))
                    }
                    DeclarationOwner::NamespacePrivate(fragment) => {
                        format!("private:{}", fragment_identity(binder, fragment))
                    }
                    DeclarationOwner::CompilationGlobal => "compilation-global".to_string(),
                    DeclarationOwner::DeferredAmbientModule(module) => {
                        format!("deferred:{}", deferred_identity(binder, module))
                    }
                }
            }

            fn member_owner_identity(
                binder: &crate::binder::Binder,
                owner: NamespaceMemberOwner,
            ) -> String {
                match owner {
                    NamespaceMemberOwner::Fragment(fragment) => {
                        format!("fragment:{}", fragment_identity(binder, fragment))
                    }
                    NamespaceMemberOwner::GlobalAugmentation(global) => {
                        format!("global:{}", global_identity(binder, global))
                    }
                    NamespaceMemberOwner::DeferredAmbientModule(module) => {
                        format!("deferred:{}", deferred_identity(binder, module))
                    }
                }
            }

            fn global_owner_identity(binder: &crate::binder::Binder, owner: GlobalOwner) -> String {
                match owner {
                    GlobalOwner::Lexical(scope) => scope_identity(binder, scope),
                    GlobalOwner::NamespaceFragment(fragment) => {
                        format!("fragment:{}", fragment_identity(binder, fragment))
                    }
                    GlobalOwner::DeferredAmbientModule(module) => {
                        format!("deferred:{}", deferred_identity(binder, module))
                    }
                }
            }

            fn export_owner_identity(
                binder: &crate::binder::Binder,
                owner: ExportContextOwner,
            ) -> String {
                match owner {
                    ExportContextOwner::NamespaceFragment(fragment) => {
                        format!("fragment:{}", fragment_identity(binder, fragment))
                    }
                    ExportContextOwner::GlobalAugmentation(global) => {
                        format!("global:{}", global_identity(binder, global))
                    }
                    ExportContextOwner::DeferredAmbientModule(module) => {
                        format!("deferred:{}", deferred_identity(binder, module))
                    }
                }
            }

            fn declaration_projection(
                binder: &crate::binder::Binder,
                id: crate::binder::declaration::DeclId,
            ) -> String {
                let declaration = binder.declarations.get(id).expect("projected declaration");
                let source = binder
                    .namespaces
                    .source_units()
                    .find(|unit| unit.module == declaration.site.module)
                    .map(|unit| unit.source)
                    .expect("declaration source identity");
                let namespace = declaration
                    .namespace
                    .map(|namespace| namespace_identity(binder, namespace));
                format!(
                    "{source:?}:{:?}:{}:{}:scope={}:namespace={namespace:?}",
                    declaration.kind,
                    declaration.site.declaration_span.start,
                    declaration.site.binding_span.start,
                    declaration
                        .site
                        .scope
                        .map(|scope| scope_identity(binder, scope))
                        .unwrap_or_else(|| "none".to_string())
                )
            }

            fn symbol_projection(binder: &crate::binder::Binder, id: SymbolId) -> String {
                let symbol = binder.symbols.get(id).expect("projected symbol");
                let declarations = symbol
                    .declarations
                    .iter()
                    .map(|declaration| declaration_projection(binder, *declaration))
                    .collect::<Vec<_>>();
                let namespace = symbol
                    .ns
                    .map(|namespace| namespace_identity(binder, namespace));
                format!(
                    "{}:value={}:type={}:group={}:namespace={namespace:?}:functions={}:declarations={declarations:?}",
                    symbol.name,
                    symbol.value.is_some(),
                    symbol.ty.is_some(),
                    symbol.ty.is_some(),
                    symbol.function_values.len()
                )
            }

            fn scope_symbols_projection(
                binder: &crate::binder::Binder,
                scope: ScopeId,
            ) -> Vec<String> {
                let scope = binder.graph.get(scope).expect("projected scope");
                let mut symbols = scope.symbols.iter().collect::<Vec<_>>();
                // Scope storage is a hash map, not a canonical iterator contract.
                symbols.sort_by(|left, right| left.0.cmp(right.0));
                symbols
                    .into_iter()
                    .map(|(name, symbol)| format!("{name}={}", symbol_projection(binder, *symbol)))
                    .collect()
            }

            fn member_projection(
                binder: &crate::binder::Binder,
                id: crate::binder::namespace::NamespaceMemberId,
            ) -> String {
                let member = binder.namespaces.member(id).expect("projected member");
                let export_context = member.export_context.and_then(|id| {
                    binder
                        .namespaces
                        .export_contexts()
                        .find(|context| context.id == id)
                        .map(|context| (context.source, context.span.start))
                });
                format!(
                    "owner={}:target={}:declaration={:?}:symbol={:?}:local-symbol={:?}:name={:?}:local={:?}:exported={:?}:decl={}:spec={:?}:binding={}:source={:?}:module={:?}:type-only={}/{}:alias={:?}/{:?}/{:?}:export-context={export_context:?}:syntax={:?}:spaces={:?}:kind={:?}:publication={:?}",
                    member_owner_identity(binder, member.owner),
                    declaration_owner_identity(binder, member.target),
                    member
                        .declaration
                        .map(|declaration| declaration_projection(binder, declaration)),
                    member.symbol.map(|symbol| symbol_projection(binder, symbol)),
                    member
                        .local_symbol
                        .map(|symbol| symbol_projection(binder, symbol)),
                    member.name,
                    member.local_name,
                    member.exported_name,
                    member.declaration_span.start,
                    member.specifier_span.map(|span| span.start),
                    member.binding_span.start,
                    member.source,
                    member.module_specifier,
                    member.outer_type_only,
                    member.specifier_type_only,
                    member.alias_context,
                    member.alias_resolution,
                    member.alias_space_intent,
                    member.syntax,
                    member.spaces,
                    member.kind,
                    member.publication,
                )
            }

            assert_eq!(scopes.len(), 2);
            let global = binder
                .graph
                .get(binder.compilation_global)
                .expect("one compilation-global scope");
            assert_eq!(global.kind, ScopeKind::CompilationGlobal);
            assert_eq!(global.parent, Some(binder.prelude_module));
            assert_eq!(global.symbols.len(), 2);
            for name in ["Shared", "GlobalN"] {
                let symbol = global
                    .lookup_local(name)
                    .and_then(|symbol| binder.symbols.get(symbol))
                    .expect("published global anchor");
                assert_eq!(symbol.value, None);
                if name == "Shared" {
                    assert!(symbol.ty.is_some());
                } else {
                    assert!(symbol.ns.is_some());
                }
            }
            assert!(scopes.iter().all(|scope| binder
                .graph
                .get(*scope)
                .is_some_and(|scope| scope.parent == Some(binder.script_namespace_root))));
            assert!(binder
                .graph
                .get(binder.script_namespace_root)
                .is_some_and(|scope| {
                    scope.kind == ScopeKind::ScriptNamespaceRoot
                        && scope.parent == Some(binder.compilation_global)
                }));
            assert!(binder
                .namespaces
                .globals()
                .all(|record| if record.issues.is_empty() {
                    record.target_scope == binder.compilation_global
                } else {
                    record.target_scope == record.overlay_scope
                }));

            let global_group = binder
                .namespaces
                .merges()
                .find(|record| {
                    record.owner == DeclarationOwner::CompilationGlobal
                        && record.name.as_ref() == "Shared"
                })
                .expect("cross-file dormant global group");
            assert_eq!(
                global_group
                    .declarations
                    .iter()
                    .map(|declaration| declaration.source)
                    .collect::<Vec<_>>(),
                vec![SourceUnitKey(1), SourceUnitKey(2)]
            );
            let source_keys: Vec<_> = binder
                .namespaces
                .source_units()
                .map(|unit| unit.source)
                .collect();
            assert!(source_keys.contains(&SourceUnitKey(1)));
            assert!(source_keys.contains(&SourceUnitKey(2)));
            assert!(binder
                .namespaces
                .source_units()
                .all(|unit| unit.context.declaration_file() && unit.context.external_module));
            assert_eq!(binder.namespaces.globals().count(), 4);
            assert_eq!(binder.namespaces.deferred_modules().count(), 2);
            assert_eq!(binder.namespaces.umd_exports().count(), 2);

            let local_symbols: Vec<_> = scopes
                .iter()
                .map(|scope| {
                    binder
                        .graph
                        .get(*scope)
                        .and_then(|scope| scope.lookup_local("Shared"))
                        .expect("module-local Shared")
                })
                .collect();
            assert_ne!(local_symbols[0], local_symbols[1]);
            assert!(local_symbols.iter().all(|symbol| binder
                .symbols
                .get(*symbol)
                .is_some_and(|symbol| symbol.ty.is_some() && symbol.ns.is_none())));

            for declaration in binder.declarations.iter().filter(|declaration| {
                matches!(
                    declaration.kind,
                    crate::binder::declaration::DeclarationKind::Namespace
                        | crate::binder::declaration::DeclarationKind::Global
                )
            }) {
                assert!(reservations.declaration_owner(declaration.id).is_some());
                assert!(reservations
                    .declaration_reservation(declaration.id)
                    .is_some());
            }

            let mut projection = Vec::new();
            for (index, unit) in binder.namespaces.source_units().enumerate() {
                projection.push(format!(
                    "source[{index}]:{:?}:scope={}:kind={:?}:external={}",
                    unit.source,
                    scope_identity(binder, unit.module),
                    unit.context.source_file_kind,
                    unit.context.external_module
                ));
            }
            for (index, record) in binder.namespaces.merges().enumerate() {
                let declarations = record
                    .declarations
                    .iter()
                    .map(|participant| {
                        format!(
                            "declaration={}:kind={:?}:source={:?}:span={}:ambient={}:spaces={:?}:syntax={:?}:fragment={:?}:instance={:?}",
                            declaration_projection(binder, participant.declaration),
                            participant.kind,
                            participant.source,
                            participant.span.start,
                            participant.ambient,
                            participant.spaces,
                            participant.syntax,
                            participant
                                .namespace_fragment
                                .map(|fragment| fragment_identity(binder, fragment)),
                            participant.namespace_instance,
                        )
                    })
                    .collect::<Vec<_>>();
                let placement_issues = record
                    .placement_issues
                    .iter()
                    .map(|issue| {
                        format!(
                            "{:?}:{}:{}",
                            issue.kind,
                            declaration_projection(binder, issue.owner),
                            issue.span.start
                        )
                    })
                    .collect::<Vec<_>>();
                projection.push(format!(
                    "merge[{index}]:owner={}:name={}:classification={:?}:declarations={declarations:?}:placement={placement_issues:?}",
                    declaration_owner_identity(binder, record.owner),
                    record.name,
                    record.classification
                ));
            }
            for (index, record) in binder.namespaces.globals().enumerate() {
                let members = record
                    .members
                    .iter()
                    .map(|member| member_projection(binder, *member))
                    .collect::<Vec<_>>();
                projection.push(format!(
                    "global[{index}]:identity={}:declaration={}:module={}:owner={}:body={}:target={}:placement={:?}:issues={:?}:declared={}:members={members:?}",
                    global_identity(binder, record.id),
                    declaration_projection(binder, record.declaration),
                    scope_identity(binder, record.module),
                    global_owner_identity(binder, record.owner),
                    record.body_span.start,
                    scope_identity(binder, record.target_scope),
                    record.placement,
                    record.issues,
                    record.declared,
                ));
            }
            for (index, module) in binder.namespaces.deferred_modules().enumerate() {
                projection.push(format!(
                    "deferred[{index}]:identity={}:declaration={}:module={}:owner={}:kind={:?}",
                    deferred_identity(binder, module.id),
                    declaration_projection(binder, module.declaration),
                    scope_identity(binder, module.module),
                    declaration_owner_identity(binder, module.owner),
                    module.kind
                ));
            }
            for (index, child) in binder.namespaces.deferred_children().enumerate() {
                projection.push(format!(
                    "child[{index}]:module={}:declaration={:?}:kind={:?}:name={:?}:span={}:binding={:?}:source={:?}",
                    deferred_identity(binder, child.module),
                    child
                        .declaration
                        .map(|declaration| declaration_projection(binder, declaration)),
                    child.kind,
                    child.name,
                    child.span.start,
                    child.binding_span.map(|span| span.start),
                    child.source,
                ));
            }
            for (index, export) in binder.namespaces.umd_exports().enumerate() {
                projection.push(format!(
                    "umd[{index}]:declaration={}:source={:?}:module={}:owner={}:name={}:span={}:context={:?}",
                    declaration_projection(binder, export.declaration),
                    export.source,
                    scope_identity(binder, export.module),
                    declaration_owner_identity(binder, export.owner),
                    export.name,
                    export.span.start,
                    export.context
                ));
            }
            for (index, context) in binder.namespaces.export_contexts().enumerate() {
                let members = context
                    .members
                    .iter()
                    .map(|member| member_projection(binder, *member))
                    .collect::<Vec<_>>();
                projection.push(format!(
                    "export[{index}]:source={:?}:span={}:owner={}:kind={:?}:syntax={:?}:resolution={:?}:module-specifier={}:members={members:?}",
                    context.source,
                    context.span.start,
                    export_owner_identity(binder, context.owner),
                    context.kind,
                    context.syntax,
                    context.resolution,
                    context.has_module_specifier
                ));
            }
            for (index, namespace) in binder.namespaces.namespaces().enumerate() {
                let fragments = namespace
                    .fragments
                    .iter()
                    .map(|fragment| {
                        let fragment_record = binder
                            .namespaces
                            .fragment(*fragment)
                            .expect("projected namespace fragment");
                        let members = fragment_record
                            .members
                            .iter()
                            .map(|member| member_projection(binder, *member))
                            .collect::<Vec<_>>();
                        format!(
                            "identity={}:declaration={}:module={}:private={}:parent={}:public={}:ambient={}:publication={:?}:instance={:?}:members={members:?}",
                            fragment_identity(binder, *fragment),
                            declaration_projection(binder, fragment_record.declaration),
                            scope_identity(binder, fragment_record.module),
                            scope_identity(binder, fragment_record.private_scope),
                            scope_identity(binder, fragment_record.lexical_parent),
                            scope_identity(binder, fragment_record.public_scope),
                            fragment_record.ambient,
                            fragment_record.publication,
                            fragment_record.instance_state,
                        )
                    })
                    .collect::<Vec<_>>();
                projection.push(format!(
                    "namespace[{index}]:identity={}:owner={}:name={}:public={}:symbol={}:fragments={fragments:?}",
                    namespace_identity(binder, namespace.id),
                    namespace_owner_identity(binder, namespace.owner),
                    namespace.name,
                    scope_identity(binder, namespace.public_scope),
                    symbol_projection(binder, namespace.symbol),
                ));
            }
            for (index, unit) in binder.namespaces.source_units().enumerate() {
                projection.push(format!(
                    "root-symbols[{index}]:{:?}:{:?}",
                    unit.source,
                    scope_symbols_projection(binder, unit.module)
                ));
            }
            projection.push(format!(
                "global-symbols:{:?}",
                scope_symbols_projection(binder, binder.compilation_global)
            ));
            for (index, namespace) in binder.namespaces.namespaces().enumerate() {
                projection.push(format!(
                    "public-symbols[{index}]:{}:{:?}",
                    namespace_identity(binder, namespace.id),
                    scope_symbols_projection(binder, namespace.public_scope)
                ));
                for (fragment_index, fragment) in namespace.fragments.iter().enumerate() {
                    let fragment_record = binder
                        .namespaces
                        .fragment(*fragment)
                        .expect("projected private symbol scope");
                    projection.push(format!(
                        "private-symbols[{index}:{fragment_index}]:{}:{:?}",
                        fragment_identity(binder, *fragment),
                        scope_symbols_projection(binder, fragment_record.private_scope)
                    ));
                }
            }
            projection
        }

        fn run(reverse: bool) -> (Vec<FileReport>, Vec<String>) {
            use std::cell::RefCell;
            let projection = RefCell::new(Vec::new());
            let reports = check_project_inner_with_binding_inspector(
                files(reverse),
                |binder, reservations, scopes| {
                    *projection.borrow_mut() = inspect(binder, reservations, scopes);
                },
            );
            (reports, projection.into_inner())
        }

        let (first, first_projection) = run(false);
        let (second, second_projection) = run(true);
        assert_eq!(first_projection, second_projection);
        for name in ["types/a.d.ts", "types/z.d.ts"] {
            let left = first
                .iter()
                .find(|report| report.name == name)
                .expect("first report");
            let right = second
                .iter()
                .find(|report| report.name == name)
                .expect("second report");
            assert_eq!(debug_diags(&left.output), debug_diags(&right.output));
            assert_eq!(left.output.parse_errors, right.output.parse_errors);
            assert_eq!(left.output.incomplete, right.output.incomplete);
        }
    }

    #[test]
    fn project_standalone_namespace_roots_and_event_keys_are_input_order_stable() {
        use std::cell::RefCell;

        let types = "interface Wu6aCrossInterface { interfaceSide: number; }\n\
                     type Wu6aCrossAlias = { aliasSide: string };\n\
                     const interfaceValue: number = Wu6aCrossInterface.value;\n\
                     const aliasValue: string = Wu6aCrossAlias.value;\n\
                     const interfaceWrong: string = Wu6aCrossInterface.value;\n\
                     Wu6aCrossInterface();\n\
                     const aliasWrong: number = Wu6aCrossAlias.value;\n\
                     new Wu6aCrossAlias();";
        let values = "namespace Wu6aCrossInterface { export const value: number = 1; }\n\
                      namespace Wu6aCrossAlias { export const value: string = \"value\"; }";

        #[derive(Debug, PartialEq, Eq)]
        struct RootProjection {
            name: String,
            symbol: crate::binder::symbol::SymbolId,
            namespace_storage: Option<crate::binder::declaration::ValueStorageId>,
            terminal_storage: Option<crate::binder::declaration::ValueStorageId>,
            terminal: &'static str,
            ty: Option<crate::types::store::TypeId>,
            published: Option<crate::types::store::TypeId>,
        }

        type ReplayProjection = (usize, u32, usize, usize, String);

        fn run(
            types: &str,
            values: &str,
            reverse: bool,
        ) -> (Vec<FileReport>, Vec<RootProjection>, Vec<ReplayProjection>) {
            let inputs = if reverse {
                vec![
                    FileInput {
                        name: "values.ts".into(),
                        source: values.into(),
                    },
                    FileInput {
                        name: "types.ts".into(),
                        source: types.into(),
                    },
                ]
            } else {
                vec![
                    FileInput {
                        name: "types.ts".into(),
                        source: types.into(),
                    },
                    FileInput {
                        name: "values.ts".into(),
                        source: values.into(),
                    },
                ]
            };
            let roots = RefCell::new(Vec::new());
            let replay = RefCell::new(Vec::new());
            let reports =
                check_project_inner_with_namespace_value_inspector(inputs, |inspection| {
                    *roots.borrow_mut() = inspection
                        .roots
                        .iter()
                        .filter(|root| root.name.starts_with("Wu6aCross"))
                        .map(|root| RootProjection {
                            name: root.name.clone(),
                            symbol: root.symbol,
                            namespace_storage: root.namespace_storage,
                            terminal_storage: root.terminal_storage,
                            terminal: root.terminal,
                            ty: root.ty,
                            published: root.published,
                        })
                        .collect();
                    *replay.borrow_mut() = inspection
                        .replay
                        .iter()
                        .map(|record| {
                            let kind = match &record.record {
                                crate::check::checker::ProjectReplayRecordInspection::Diagnostic(
                                    code,
                                ) => format!("diagnostic:{code}"),
                                crate::check::checker::ProjectReplayRecordInspection::Incomplete(
                                    id,
                                ) => format!("incomplete:{id}"),
                            };
                            (
                                record.key.module_ordinal.index(),
                                record.key.source_start,
                                record.key.event_ordinal,
                                record.key.record_ordinal,
                                kind,
                            )
                        })
                        .collect();
                });
            (reports, roots.into_inner(), replay.into_inner())
        }

        let (forward_reports, forward, forward_replay) = run(types, values, false);
        let (reverse_reports, reverse, reverse_replay) = run(types, values, true);
        for reports in [&forward_reports, &reverse_reports] {
            assert!(reports
                .iter()
                .all(|report| report.output.parse_errors.is_empty()));
            assert!(reports
                .iter()
                .all(|report| report.output.incomplete.is_empty()));
            assert_eq!(
                reports
                    .iter()
                    .flat_map(|report| codes(&report.output))
                    .collect::<Vec<_>>(),
                ["TK2322", "TK2349", "TK2322", "TK2351"]
            );
        }

        assert_eq!(forward.len(), 2);
        assert_eq!(reverse.len(), 2);
        for (left, right) in forward.iter().zip(&reverse) {
            assert_eq!(left.name, right.name);
            assert_eq!(
                left.symbol, right.symbol,
                "root SymbolId is input-order stable"
            );
            assert_eq!(left.terminal, "ready");
            assert_eq!(right.terminal, "ready");
            assert_eq!(left.namespace_storage, left.terminal_storage);
            assert_eq!(right.namespace_storage, right.terminal_storage);
            assert_eq!(
                left.namespace_storage, right.namespace_storage,
                "namespace-owned ValueStorageId is input-order stable"
            );
            assert_eq!(left.ty, left.published, "forward root publishes atomically");
            assert_eq!(
                right.ty, right.published,
                "reverse root publishes atomically"
            );
            assert_eq!(left.ty, right.ty, "root TypeId is input-order stable");
        }
        let replay = |module| {
            vec![
                (module, 213, 17, 1, "diagnostic:TK2322".to_owned()),
                (module, 264, 19, 0, "diagnostic:TK2349".to_owned()),
                (module, 292, 21, 1, "diagnostic:TK2322".to_owned()),
                (module, 335, 23, 0, "diagnostic:TK2351".to_owned()),
            ]
        };
        assert_eq!(forward_replay, replay(0));
        assert_eq!(reverse_replay, replay(1));
    }

    #[test]
    fn unavailable_namespace_export_alias_replays_at_exact_local_owner() {
        use std::cell::RefCell;

        let source = "declare namespace ExactAliasOwner {\n\
                      enum Hidden { A }\n\
                      export { Hidden as PublicHidden };\n\
                      }";
        let local_start = source
            .find("Hidden as PublicHidden")
            .expect("alias local spelling");
        let local_start = u32::try_from(local_start).expect("source offset fits u32");
        let local_span = Span::new(local_start, local_start + 6);
        let replay = RefCell::new(Vec::new());
        let reports = check_project_inner_with_namespace_value_inspector(
            vec![FileInput {
                name: "alias.ts".into(),
                source: source.into(),
            }],
            |inspection| {
                *replay.borrow_mut() = inspection
                    .replay
                    .iter()
                    .filter_map(|record| match &record.record {
                        crate::check::checker::ProjectReplayRecordInspection::Incomplete(id)
                            if id == "decl/export-specifier/namespace-payload-unavailable" =>
                        {
                            Some((
                                record.key.module_ordinal.index(),
                                record.key.source_start,
                                record.key.event_ordinal,
                                record.key.record_ordinal,
                            ))
                        }
                        _ => None,
                    })
                    .collect();
            },
        );
        assert_eq!(reports.len(), 1);
        assert!(reports[0].output.parse_errors.is_empty());
        assert!(reports[0].output.diagnostics.is_empty());
        assert_eq!(
            reports[0]
                .output
                .incomplete
                .iter()
                .filter(|incomplete| {
                    incomplete.id == "decl/export-specifier/namespace-payload-unavailable"
                })
                .map(|incomplete| incomplete.span)
                .collect::<Vec<_>>(),
            [local_span]
        );
        assert_eq!(replay.into_inner(), [(0, local_start, 2, 0)]);
    }

    #[test]
    fn namespace_only_root_stays_invisible_to_production_resolution() {
        use crate::diagnostics::DiagnosticCode;

        let source = "namespace Dormant {} let typed: Dormant; Dormant;";
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(output.incomplete.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.span))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2304, Span::new(32, 39)),
                (DiagnosticCode::TK2708, Span::new(41, 48)),
            ]
        );
    }

    #[test]
    fn project_filename_matrix_keeps_parser_goal_and_binding_context_separate() {
        use crate::binder::namespace::SourceFileKind;
        use std::cell::RefCell;

        let contexts = RefCell::new(Vec::new());
        let reports = check_project_inner_with_binding_inspector(
            vec![
                FileInput {
                    name: "a.ts".into(),
                    source: "const value = 1;".into(),
                },
                FileInput {
                    name: "b.d.ts".into(),
                    source: "declare const value: number;".into(),
                },
                FileInput {
                    name: "c.mts".into(),
                    source: "const value = 1;".into(),
                },
                FileInput {
                    name: "d.cts".into(),
                    source: "const value = 1;".into(),
                },
                FileInput {
                    name: "e.d.mts".into(),
                    source: "declare const value: number;".into(),
                },
                FileInput {
                    name: "f.d.cts".into(),
                    source: "declare const value: number;".into(),
                },
            ],
            |binder, _, _| {
                *contexts.borrow_mut() = binder
                    .namespaces
                    .source_units()
                    .map(|unit| (unit.context.source_file_kind, unit.context.external_module))
                    .collect();
            },
        );
        assert_eq!(
            contexts.into_inner(),
            vec![
                (SourceFileKind::ImplementationTs, false),
                (SourceFileKind::DeclarationTs, false),
                (SourceFileKind::ImplementationMts, true),
                (SourceFileKind::ImplementationCts, true),
                (SourceFileKind::DeclarationMts, false),
                (SourceFileKind::DeclarationCts, false),
            ]
        );
        assert!(reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()));
    }

    #[test]
    fn extension_implied_external_context_excludes_declaration_mts_and_cts() {
        use crate::binder::namespace::{GlobalIssue, GlobalPlacement, SourceFileKind, UmdContext};
        use std::cell::RefCell;

        let metadata = RefCell::new(Vec::new());
        let source = |name: &str| FileInput {
            name: name.into(),
            source: format!(
                "global {{ interface FileGlobal {{}} }} export as namespace {};",
                name.replace('.', "_")
            ),
        };
        let reports = check_project_inner_with_binding_inspector(
            vec![
                source("a.ts"),
                source("b.mts"),
                source("c.cts"),
                source("d.d.mts"),
                source("e.d.cts"),
            ],
            |binder, _, _| {
                *metadata.borrow_mut() = binder
                    .namespaces
                    .source_units()
                    .map(|unit| {
                        let global = binder
                            .namespaces
                            .globals()
                            .find(|global| global.source == unit.source)
                            .expect("one global per source");
                        let umd = binder
                            .namespaces
                            .umd_exports()
                            .find(|export| export.source == unit.source)
                            .expect("one UMD export per source");
                        (
                            unit.context.source_file_kind,
                            unit.context.external_module,
                            global.placement,
                            global.issues.clone(),
                            umd.context,
                        )
                    })
                    .collect();
            },
        );
        assert_eq!(
            metadata.into_inner(),
            vec![
                (
                    SourceFileKind::ImplementationTs,
                    false,
                    GlobalPlacement::DirectScript,
                    vec![GlobalIssue::FutureTk2669, GlobalIssue::FutureTk2670],
                    UmdContext::FutureTk1314NonExternal,
                ),
                (
                    SourceFileKind::ImplementationMts,
                    true,
                    GlobalPlacement::DirectExternalModule,
                    vec![GlobalIssue::FutureTk2670],
                    UmdContext::FutureTk1315Implementation,
                ),
                (
                    SourceFileKind::ImplementationCts,
                    true,
                    GlobalPlacement::DirectExternalModule,
                    vec![GlobalIssue::FutureTk2670],
                    UmdContext::FutureTk1315Implementation,
                ),
                (
                    SourceFileKind::DeclarationMts,
                    false,
                    GlobalPlacement::DirectScript,
                    vec![GlobalIssue::FutureTk2669],
                    UmdContext::FutureTk1314NonExternal,
                ),
                (
                    SourceFileKind::DeclarationCts,
                    false,
                    GlobalPlacement::DirectScript,
                    vec![GlobalIssue::FutureTk2669],
                    UmdContext::FutureTk1314NonExternal,
                ),
            ]
        );
        assert!(reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()));
    }

    #[test]
    fn global_and_umd_context_diagnostics_keep_exact_event_order_and_source_kind() {
        use crate::diagnostics::DiagnosticCode;

        let check = |name: &str, source: &str| {
            check_project(vec![FileInput {
                name: name.into(),
                source: source.into(),
            }])
            .pop()
            .expect("one project report")
            .output
        };

        let script = check("script.ts", "global { interface Invalid {} }");
        assert_eq!(
            script
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.span))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2669, Span::new(0, 6)),
                (DiagnosticCode::TK2670, Span::new(0, 6)),
            ]
        );
        assert!(script.incomplete.is_empty());

        let implementation = check(
            "implementation.ts",
            "export as namespace Invalid; export {};",
        );
        assert_eq!(
            implementation
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK1315]
        );
        assert!(implementation.incomplete.is_empty());

        let non_module = check("non-module.ts", "export as namespace Invalid;");
        assert_eq!(
            non_module
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK1314]
        );
        assert!(non_module.incomplete.is_empty());

        for declaration_extension in ["invalid.d.mts", "invalid.d.cts"] {
            let quarantined = check(
                declaration_extension,
                "declare global { interface Hidden {} } declare const leak: Hidden;",
            );
            assert_eq!(
                quarantined
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>(),
                [DiagnosticCode::TK2669, DiagnosticCode::TK2304],
                "{declaration_extension}"
            );
            assert!(quarantined.incomplete.is_empty(), "{declaration_extension}");

            let umd = check(
                declaration_extension,
                "export as namespace InvalidDeclarationModule;",
            );
            assert_eq!(
                umd.diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>(),
                [DiagnosticCode::TK1314],
                "{declaration_extension}"
            );
            assert!(umd.incomplete.is_empty(), "{declaration_extension}");

            let syntactic_module = check(
                declaration_extension,
                "export {}; declare global { interface Visible {} } declare const visible: Visible;",
            );
            assert!(
                syntactic_module.diagnostics.is_empty(),
                "{declaration_extension}: {:?}",
                syntactic_module.diagnostics
            );
            assert!(
                syntactic_module.incomplete.is_empty(),
                "{declaration_extension}"
            );
        }

        let declaration = check(
            "valid.d.ts",
            "export as namespace Valid; export = Valid; declare function Valid(): void;",
        );
        assert!(declaration.diagnostics.is_empty());
        assert_eq!(
            declaration
                .incomplete
                .iter()
                .map(|surface| surface.id.as_str())
                .collect::<Vec<_>>(),
            ["decl/namespace-export/self", "decl/export-assignment/self"]
        );
    }

    #[test]
    fn mixed_global_block_publishes_independent_types_without_partial_class_pair() {
        use crate::diagnostics::DiagnosticCode;

        let output = check_source(
            r#"
export {};
class DeferredPair { local = 1 }
declare global {
    interface PublishedGlobal { value: number }
    interface DeferredPair { global: string }
    class DeferredPair { global: string }
}
const local = new DeferredPair();
const localValue: number = local.local;
const globalLeak = local.global;
declare const published: PublishedGlobal;
const publishedWrong: boolean = published.value;
"#,
        );

        assert!(output.parse_errors.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK2339, DiagnosticCode::TK2322]
        );
        assert!(output.incomplete.is_empty());
    }

    #[test]
    fn global_namespace_incomplete_is_owned_by_the_originating_value_fragment() {
        let type_only = FileInput {
            name: "type-only.ts".into(),
            source: "export {}; declare global { namespace SplitGlobal { interface TypeOnly { value: number } } }".into(),
        };
        let value_bearing = FileInput {
            name: "value-bearing.ts".into(),
            source:
                "export {}; declare global { namespace SplitGlobal { const runtime: number; } }"
                    .into(),
        };

        for reverse in [false, true] {
            let inputs = if reverse {
                vec![
                    FileInput {
                        name: value_bearing.name.clone(),
                        source: value_bearing.source.clone(),
                    },
                    FileInput {
                        name: type_only.name.clone(),
                        source: type_only.source.clone(),
                    },
                ]
            } else {
                vec![
                    FileInput {
                        name: type_only.name.clone(),
                        source: type_only.source.clone(),
                    },
                    FileInput {
                        name: value_bearing.name.clone(),
                        source: value_bearing.source.clone(),
                    },
                ]
            };
            let reports = check_project(inputs);
            assert!(reports.iter().all(|report| {
                report.output.parse_errors.is_empty() && report.output.diagnostics.is_empty()
            }));
            let type_report = reports
                .iter()
                .find(|report| report.name == "type-only.ts")
                .expect("type-only report");
            assert!(
                type_report.output.incomplete.is_empty(),
                "reverse={reverse}"
            );
            let value_report = reports
                .iter()
                .find(|report| report.name == "value-bearing.ts")
                .expect("value-bearing report");
            assert_eq!(
                value_report
                    .output
                    .incomplete
                    .iter()
                    .map(|surface| surface.id.as_str())
                    .collect::<Vec<_>>(),
                ["decl/global-declaration/self"],
                "reverse={reverse}"
            );
        }
    }

    #[test]
    fn a_declare_global_project_continues_the_frozen_library() {
        let cases: &[(&str, &str)] = &[
            (
                "a library-owned name",
                "export {};\ndeclare global {\n  interface Window { b103Flag: boolean }\n}\n",
            ),
            (
                "a fresh name",
                "export {};\ndeclare global {\n  interface B103Brand { tag: string }\n  var b103Counter: number;\n}\n",
            ),
        ];
        for (label, source) in cases {
            let inputs = vec![
                FileInput {
                    name: "augment.ts".to_string(),
                    source: (*source).to_string(),
                },
                FileInput {
                    name: "read.ts".to_string(),
                    source: "export const value: number = 1;\n".to_string(),
                },
            ];
            let result = super::check_project(inputs);
            assert!(
                result.is_ok(),
                "{label}: declare global must bind: {:?}",
                result.as_ref().err()
            );
            if let Ok(reports) = result {
                assert_eq!(reports.len(), 2);
                assert!(reports
                    .iter()
                    .all(|report| report.output.parse_errors.is_empty()));
            }
        }

        let result = super::check_source("export {};\ndeclare global { var b103Solo: number; }\n");
        assert!(
            result.is_ok(),
            "single-source declare global must bind: {:?}",
            result.as_ref().err()
        );
        if let Ok(output) = result {
            assert!(output.parse_errors.is_empty());
        }
    }

    #[test]
    fn sparse_library_class_collision_keeps_the_library_terminal() {
        let output = super::check_source("class Date {}\nconst value: Date = new Date();\n")
            .expect("class collision checks in a private epoch");
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn sparse_library_interface_collision_publishes_the_merged_surface() {
        let output = super::check_source(
            "interface Array<T> { b103Head(): T }\nconst head: number = [1].b103Head();\nconst wrong: string = [1].b103Head();\nconst wrongMapped: string[] = [1].map((value) => value + 1);\n",
        )
        .expect("interface collision checks in a private epoch");
        assert_eq!(output.diagnostics.len(), 2, "{:?}", output.diagnostics);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn sparse_library_interface_collision_replays_root_slot_consumers() {
        let output = super::check_source(
            "interface EventSourceInit { b103Required: string; }\nnew EventSource(\"https://example.invalid\", {});\n",
        )
        .expect("EventSourceInit collision checks in a private epoch");
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2345"],
            "{:?}",
            output.diagnostics
        );
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn sparse_library_alias_collision_keeps_the_library_terminal() {
        let output = super::check_source(
            "type Partial<T> = { b103: T };\nconst kept: Partial<{ value: number }> = {};\nconst rejected: Partial<number> = { b103: 1 };\n",
        )
        .expect("alias collision checks in a private epoch");
        assert_eq!(output.diagnostics.len(), 1);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn sparse_epoch_preserves_legal_class_interface_merges_in_both_orders() {
        let class = "class B103Merged {\n  fromClass(): string { return \"ok\"; }\n  static version: number = 1;\n}\n";
        let interface = "interface B103Merged { fromInterface(): number }\n";
        for declarations in [format!("{class}{interface}"), format!("{interface}{class}")] {
            let source = format!(
                "interface Array<T> {{ b103Collision(): T }}\n{declarations}const merged = new B103Merged();\nconst fromClass: string = merged.fromClass();\nconst fromInterface: number = merged.fromInterface();\nconst wrong: string = merged.fromInterface();\nconst version: number = B103Merged.version;\n"
            );
            let output = super::check_source(&source).expect("class-interface merge private epoch");
            assert_eq!(output.diagnostics.len(), 1, "{:?}", output.diagnostics);
            assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        }
    }

    #[test]
    fn sparse_library_interface_legally_augments_an_inherited_class_surface() {
        let output = super::check_source(
            "interface SafeArray<T = any> { b103Value(): T }\ndeclare const safe: SafeArray<number>;\nconst value: number = safe.b103Value();\nconst wrong: string = safe.b103Value();\nconst array: number[] = new VBArray(safe).toArray();\n",
        )
        .expect("inherited class-interface merge checks in a private epoch");
        assert_eq!(output.diagnostics.len(), 1, "{:?}", output.diagnostics);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    }

    #[test]
    fn sparse_global_object_replay_publishes_script_contributors() {
        let result = super::check_project(vec![
            FileInput {
                name: "00_global.ts".to_owned(),
                source: "declare var B103GlobalThisValue: { enabled: boolean };\nfunction B103GlobalThisCall(): number { return 1; }\n".to_owned(),
            },
            FileInput {
                name: "99_consume.ts".to_owned(),
                source: "const enabled: boolean = globalThis.B103GlobalThisValue.enabled;\nconst wrongEnabled: string = globalThis.B103GlobalThisValue.enabled;\nconst called: number = globalThis.B103GlobalThisCall();\nconst wrongCalled: string = globalThis.B103GlobalThisCall();\n".to_owned(),
            },
        ]);
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        if let Ok(reports) = result {
            assert_eq!(reports.len(), 2);
            assert!(reports[0].output.diagnostics.is_empty());
            assert_eq!(
                reports[1]
                    .output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code.as_str())
                    .collect::<Vec<_>>(),
                ["TK2322", "TK2322"]
            );
        }
    }

    #[test]
    fn sparse_generic_array_replacement_substitutes_fresh_parameters_in_both_file_orders() {
        let augmentation = "interface Array<T> {\n  b103Self(): Array<T>;\n  b103Cross<U>(value: U): Array<U>;\n}\n";
        let read = "const selfNumber: number = [1].b103Self()[0];\nconst crossString: string = [1].b103Cross(\"x\")[0];\nconst wrongCross: number = [1].b103Cross(\"x\")[0];\n";
        let orders = [
            [
                FileInput {
                    name: "00_augment.ts".to_string(),
                    source: augmentation.to_string(),
                },
                FileInput {
                    name: "99_read.ts".to_string(),
                    source: read.to_string(),
                },
            ],
            [
                FileInput {
                    name: "00_read.ts".to_string(),
                    source: read.to_string(),
                },
                FileInput {
                    name: "99_augment.ts".to_string(),
                    source: augmentation.to_string(),
                },
            ],
        ];
        for inputs in orders {
            let reports =
                super::check_project(Vec::from(inputs)).expect("generic collision project");
            let diagnostics = reports
                .iter()
                .flat_map(|report| &report.output.diagnostics)
                .collect::<Vec<_>>();
            let incomplete = reports
                .iter()
                .flat_map(|report| &report.output.incomplete)
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert!(incomplete.is_empty(), "{incomplete:?}");
        }
    }
}
