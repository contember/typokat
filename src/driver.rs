//! Pipeline orchestration: source → parse → check → diagnostics.
//!
//! The driver owns the per-run allocator that backs the AST and the type
//! `Interner`, keeping borrowed parser data inside the parse/check call.

use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
#[cfg(test)]
use crate::check::checker::{
    check_project_programs_with_binding_inspector,
    check_project_programs_with_namespace_value_inspector,
};
use crate::check::{
    check_program, check_project_programs, CheckResult, ProjectImport, ProjectImportSource,
    ProjectProgram,
};
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::library::{FrozenLibraryBase, LibraryBaseProvider};
use crate::source::{CompilationOrigin, ModuleOrdinal, OriginalModuleOrdinal, UnitSlot};
use crate::span::Span;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement,
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};

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
        !self.incomplete.is_empty()
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
pub fn check_source(source: &str) -> CheckOutput {
    // Run on a large-stack worker so deep input meets the checker's nesting budget rather
    // than a native parser stack overflow (see `CHECK_STACK_SIZE`). The oxc AST is
    // `!Send`, so the whole parse→check stays inside; only the owned `CheckOutput` (Send)
    // crosses back out of the scope.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(CHECK_STACK_SIZE)
            .spawn_scoped(scope, || check_source_inner(source))
            .expect("spawn check worker")
            .join()
            .expect("check worker panicked")
    })
}

fn check_source_inner(source: &str) -> CheckOutput {
    let allocator = Allocator::default();
    // TypeScript, non-JSX, module semantics.
    let source_type = SourceType::ts();

    let parsed = Parser::new(&allocator, source, source_type).parse();

    // Collect parser diagnostics as strings (their own types borrow elsewhere;
    // we only need them rendered for the CLI).
    let parse_errors: Vec<String> = parsed.diagnostics.iter().map(|d| d.to_string()).collect();

    // If the parser bailed entirely, the AST is empty — there is nothing to
    // check, and the parse errors carry the explanation.
    if parsed.panicked {
        return CheckOutput {
            diagnostics: Vec::new(),
            parse_errors,
            incomplete: Vec::new(),
        };
    }

    let mut interner = Interner::with_intrinsics();
    let CheckResult {
        diagnostics,
        incomplete,
        ..
    } = check_program(&mut interner, &parsed.program);

    CheckOutput {
        diagnostics,
        parse_errors,
        incomplete,
    }
}

/// One file handed to [`check_files`]. Owned strings let the parse→check pipeline
/// stay pinned to one worker because the oxc AST is `!Send + !Sync`.
pub struct FileInput {
    pub name: String,
    pub source: String,
}

/// Result for one [`FileInput`]. `name` and `source` move through the pipeline so
/// diagnostics can render without a side table; `reports[i]` matches `inputs[i]`.
pub struct FileReport {
    pub name: String,
    pub source: String,
    pub output: CheckOutput,
}

/// Check many files in parallel, with an independent allocator/interner per file.
/// There is no cross-file resolution on this API, so per-file pipelines are
/// lossless and keep the `!Send + !Sync` AST on its parser thread. Order is
/// preserved: `reports[i]` corresponds to `inputs[i]`.
pub fn check_files(inputs: Vec<FileInput>) -> Vec<FileReport> {
    inputs
        .into_par_iter()
        .map(|input| {
            let output = check_source(&input.source);
            FileReport {
                name: input.name,
                source: input.source,
                output,
            }
        })
        .collect()
}

/// Check a local relative-module project in one serial type universe, resolving
/// only `./` / `../` specifiers among the provided `.ts` files. Runs on a large-stack
/// worker for the same reason as [`check_source`] (deep input meets the checker's nesting
/// budget rather than a native parser stack overflow); `inputs` and the returned reports
/// are owned/`Send`, so they cross the scope cleanly.
pub fn check_project(inputs: Vec<FileInput>) -> Vec<FileReport> {
    #[cfg(test)]
    {
        let (reports, receipt) = std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(CHECK_STACK_SIZE)
                .spawn_scoped(scope, || {
                    let reports = check_project_inner(inputs);
                    let receipt = crate::check::checker::project_binding_thread_receipt_for_test();
                    (reports, receipt)
                })
                .expect("spawn check worker")
                .join()
                .expect("check worker panicked")
        });
        crate::check::checker::merge_project_binding_thread_receipt_for_test(receipt);
        reports
    }
    #[cfg(not(test))]
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(CHECK_STACK_SIZE)
            .spawn_scoped(scope, || check_project_inner(inputs))
            .expect("spawn check worker")
            .join()
            .expect("check worker panicked")
    })
}

fn check_project_inner(inputs: Vec<FileInput>) -> Vec<FileReport> {
    check_project_inner_with_checker(inputs, check_project_programs)
}

// --- Default-library siblings (backlog 14 scaffolding) -----------------------
//
// [`check_source_with_library`] and [`check_project_with_library`] are a deliberate, temporary
// SECOND ENTRY POINT — not a second loader. There is exactly one [`FrozenLibraryBase`] and one
// `LibraryCompiler` in the process; the siblings only choose which base a check forks from, so
// backlog 14's prohibition on forking a second *ambient-loading path* is not engaged. They exist
// so the backlog-14 acceptance corpus can run before production moves off `src/prelude.ts`, and
// the WU7 cutover deletes them once `check_source`/`check_project` fork from the library base
// themselves.

/// The process-wide default-library base. Publication happens once, on the first caller's thread,
/// and never inside a rayon fan-out: [`check_files`] is the crate's only rayon site and does not
/// reach here.
fn library_base() -> Result<Arc<FrozenLibraryBase>, String> {
    static PROVIDER: LazyLock<LibraryBaseProvider> = LazyLock::new(LibraryBaseProvider::new);
    PROVIDER.get().map_err(|error| error.to_string())
}

/// Run `work` on a large-stack worker, for the reason given at [`CHECK_STACK_SIZE`]. A panic in
/// the worker is re-raised on the caller's thread rather than converted to an error.
fn on_check_worker<T, W>(work: W) -> Result<T, String>
where
    T: Send,
    W: FnOnce() -> T + Send,
{
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .stack_size(CHECK_STACK_SIZE)
            .spawn_scoped(scope, work)
            .map_err(|error| format!("cannot spawn the check worker: {error}"))?;
        match worker.join() {
            Ok(product) => Ok(product),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

/// [`check_source`] against the full default library instead of `src/prelude.ts`.
///
/// Temporary backlog-14 scaffolding — see the section comment above. `Err` means the library base
/// itself could not be published or forked; ordinary type/parse problems ride in the
/// [`CheckOutput`] exactly as on the prelude path.
pub fn check_source_with_library(source: &str) -> Result<CheckOutput, String> {
    let base = library_base()?;
    on_check_worker(|| check_source_with_library_inner(&base, source))?
}

fn check_source_with_library_inner(
    base: &FrozenLibraryBase,
    source: &str,
) -> Result<CheckOutput, String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let parse_errors: Vec<String> = parsed.diagnostics.iter().map(|d| d.to_string()).collect();
    if parsed.panicked {
        return Ok(CheckOutput {
            diagnostics: Vec::new(),
            parse_errors,
            incomplete: Vec::new(),
        });
    }

    let state = base.fork_user_delta().map_err(str::to_owned)?;
    let CheckResult {
        diagnostics,
        incomplete,
        ..
    } = crate::check::checker::check_program_with_owned_library(state, &parsed.program)
        .map_err(str::to_owned)?;

    Ok(CheckOutput {
        diagnostics,
        parse_errors,
        incomplete,
    })
}

/// [`check_project`] against the full default library instead of `src/prelude.ts`.
///
/// Temporary backlog-14 scaffolding — see the section comment above. The module graph, dependency
/// order, and per-input report order are the production ones; only the base differs.
pub fn check_project_with_library(inputs: Vec<FileInput>) -> Result<Vec<FileReport>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let base = library_base()?;
    on_check_worker(|| check_project_with_library_inner(&base, inputs))?
}

fn check_project_with_library_inner(
    base: &FrozenLibraryBase,
    inputs: Vec<FileInput>,
) -> Result<Vec<FileReport>, String> {
    let state = base.fork_user_delta().map_err(str::to_owned)?;
    Ok(check_project_inner_with_checker(inputs, |_, units| {
        crate::check::checker::check_project_programs_with_library(state, units)
    }))
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

fn check_project_inner_with_checker<F>(inputs: Vec<FileInput>, check_project: F) -> Vec<FileReport>
where
    F: for<'ast> FnOnce(&mut Interner, &[ProjectProgram<'ast>]) -> Vec<CheckResult>,
{
    if inputs.is_empty() {
        return Vec::new();
    }

    let ProjectFrontendRun {
        inputs,
        parse_errors,
        product: (project_units_by_slot, ordered_results),
    } = run_project_frontend(inputs, |interner, units| {
        let project_units_by_slot = units
            .iter()
            .map(|unit| unit.module_ordinal)
            .collect::<Vec<_>>();
        (project_units_by_slot, check_project(interner, units))
    });
    let mut diagnostics_by_original: Vec<Vec<Diagnostic>> =
        (0..inputs.len()).map(|_| Vec::new()).collect();
    let mut incomplete_by_original: Vec<Vec<IncompleteSurface>> =
        (0..inputs.len()).map(|_| Vec::new()).collect();
    for result in ordered_results {
        let original = result.module_ordinal.index();
        debug_assert_eq!(
            project_units_by_slot.get(result.unit_slot.index()).copied(),
            Some(result.module_ordinal)
        );
        if let Some(slot) = diagnostics_by_original.get_mut(original) {
            *slot = result.diagnostics;
        }
        if let Some(slot) = incomplete_by_original.get_mut(original) {
            *slot = result.incomplete;
        }
    }

    inputs
        .into_iter()
        .enumerate()
        .map(|(index, input)| FileReport {
            name: input.name,
            source: input.source,
            output: CheckOutput {
                diagnostics: diagnostics_by_original
                    .get_mut(index)
                    .map(std::mem::take)
                    .unwrap_or_default(),
                parse_errors: parse_errors.get(index).cloned().unwrap_or_default(),
                incomplete: incomplete_by_original
                    .get_mut(index)
                    .map(std::mem::take)
                    .unwrap_or_default(),
            },
        })
        .collect()
}

pub(crate) struct ProjectFrontendRun<Product> {
    inputs: Vec<FileInput>,
    parse_errors: Vec<Vec<String>>,
    product: Product,
}

impl<Product> ProjectFrontendRun<Product> {
    pub(crate) fn into_product(self) -> Product {
        self.product
    }
}

pub(crate) fn run_project_frontend<Product>(
    inputs: Vec<FileInput>,
    consume: impl for<'ast> FnOnce(&mut Interner, &[ProjectProgram<'ast>]) -> Product,
) -> ProjectFrontendRun<Product> {
    let allocators: Vec<Allocator> = (0..inputs.len()).map(|_| Allocator::default()).collect();
    let parsed: Vec<_> = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, SourceType::ts()).parse())
        .collect();

    let parse_errors: Vec<Vec<String>> = parsed
        .iter()
        .map(|parsed| parsed.diagnostics.iter().map(|d| d.to_string()).collect())
        .collect();

    let paths = normalized_input_paths(&inputs);
    let source_keys = stable_source_keys(&paths);
    let path_to_index: BTreeMap<PathBuf, usize> = paths
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect();
    let raw_imports: Vec<Vec<RawImport>> = parsed
        .iter()
        .zip(&paths)
        .map(|(parsed, path)| scan_imports(&parsed.program, path, &path_to_index))
        .collect();
    let order = dependency_order(&raw_imports);
    let mut ordered_index = vec![0usize; inputs.len()];
    for (position, &original) in order.iter().enumerate() {
        if let Some(slot) = ordered_index.get_mut(original) {
            *slot = position;
        }
    }

    let project_units: Vec<ProjectProgram<'_>> = order
        .iter()
        .enumerate()
        .map(|(unit_slot, &original)| ProjectProgram {
            module_ordinal: ModuleOrdinal::new(original),
            unit_slot: UnitSlot::new(unit_slot),
            normalized_path: paths[original].to_string_lossy().into_owned(),
            program: &parsed[original].program,
            compilation_unit: CompilationUnit {
                source: source_keys[original],
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original)),
                binding: ModuleBindingContext::for_program(
                    &parsed[original].program,
                    source_file_kind(&inputs[original].name),
                ),
            },
            imports: raw_imports[original]
                .iter()
                .map(|import| ProjectImport {
                    local: import.local.clone(),
                    imported: import.imported.clone(),
                    module: import.specifier.clone(),
                    source: match import.source {
                        RawImportSource::Resolved(target) => {
                            ProjectImportSource::Resolved(ordered_index[target])
                        }
                        RawImportSource::Missing => {
                            ProjectImportSource::Missing(import.specifier.clone())
                        }
                    },
                    type_only: import.type_only,
                    local_span: import.local_span,
                    span: import.span,
                    owner_start: import.owner_start,
                })
                .collect(),
        })
        .collect();

    let mut interner = Interner::with_intrinsics();
    let product = consume(&mut interner, &project_units);
    ProjectFrontendRun {
        inputs,
        parse_errors,
        product,
    }
}

#[cfg(test)]
pub(crate) fn check_project_with_owned_checker_for_test<F>(
    inputs: Vec<FileInput>,
    check_project: F,
) -> Vec<FileReport>
where
    F: for<'ast> FnOnce(&[ProjectProgram<'ast>]) -> Vec<CheckResult>,
{
    check_project_inner_with_checker(inputs, |_, units| {
        crate::check::checker::library_compiler::record_user_source_parses_for_test(units.len());
        check_project(units)
    })
}

#[derive(Clone)]
struct RawImport {
    local: String,
    imported: String,
    specifier: String,
    source: RawImportSource,
    type_only: bool,
    local_span: Span,
    span: Span,
    owner_start: u32,
}

#[derive(Clone, Copy)]
enum RawImportSource {
    Resolved(usize),
    Missing,
}

fn scan_imports(
    program: &Program<'_>,
    importer_path: &Path,
    path_to_index: &BTreeMap<PathBuf, usize>,
) -> Vec<RawImport> {
    let mut imports = Vec::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        let specifier = import.source.value.as_str().to_string();
        if !is_local_relative(&specifier) {
            continue;
        }
        let source = resolve_local_import(importer_path, &specifier)
            .and_then(|path| path_to_index.get(&path).copied())
            .map(RawImportSource::Resolved)
            .unwrap_or(RawImportSource::Missing);
        let outer_type_only = import.import_kind == ImportOrExportKind::Type;
        if let Some(specifiers) = &import.specifiers {
            for spec in specifiers {
                let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                    continue;
                };
                let Some(imported) = module_export_name(&named.imported) else {
                    continue;
                };
                let type_only = outer_type_only || named.import_kind == ImportOrExportKind::Type;
                imports.push(RawImport {
                    local: named.local.name.to_string(),
                    imported: imported.to_string(),
                    specifier: specifier.clone(),
                    source,
                    type_only,
                    local_span: Span::from_oxc(named.local.span),
                    span: Span::from_oxc(named.span),
                    owner_start: import.span.start,
                });
            }
        }
    }
    imports
}

fn module_export_name<'ast>(name: &'ast ModuleExportName<'ast>) -> Option<&'ast str> {
    match name {
        ModuleExportName::IdentifierName(id) => Some(id.name.as_str()),
        ModuleExportName::IdentifierReference(id) => Some(id.name.as_str()),
        ModuleExportName::StringLiteral(_) => None,
    }
}

fn dependency_order(imports: &[Vec<RawImport>]) -> Vec<usize> {
    let mut state = vec![VisitState::Unseen; imports.len()];
    let mut order = Vec::with_capacity(imports.len());
    for index in 0..imports.len() {
        visit(index, imports, &mut state, &mut order);
    }
    order
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Visiting,
    Done,
}

fn visit(
    index: usize,
    imports: &[Vec<RawImport>],
    state: &mut [VisitState],
    order: &mut Vec<usize>,
) {
    match state.get(index).copied() {
        Some(VisitState::Done | VisitState::Visiting) | None => return,
        Some(VisitState::Unseen) => {}
    }
    if let Some(slot) = state.get_mut(index) {
        *slot = VisitState::Visiting;
    }
    if let Some(module_imports) = imports.get(index) {
        for import in module_imports {
            if let RawImportSource::Resolved(dep) = import.source {
                visit(dep, imports, state, order);
            }
        }
    }
    if let Some(slot) = state.get_mut(index) {
        *slot = VisitState::Done;
    }
    order.push(index);
}

fn normalized_input_paths(inputs: &[FileInput]) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    inputs
        .iter()
        .map(|input| {
            let path = Path::new(&input.name);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            normalize_path(&absolute)
        })
        .collect()
}

fn stable_source_keys(paths: &[PathBuf]) -> Vec<SourceUnitKey> {
    let mut ranked: Vec<_> = paths.iter().enumerate().collect();
    ranked.sort_by_key(|(_, path)| (*path).clone());
    let mut keys = vec![SourceUnitKey::SINGLE_SOURCE; paths.len()];
    for (rank, (original, _)) in ranked.into_iter().enumerate() {
        if let Some(key) = keys.get_mut(original) {
            *key =
                SourceUnitKey(u32::try_from(rank + 1).expect("project source path rank fits u32"));
        }
    }
    keys
}

fn resolve_local_import(importer_path: &Path, specifier: &str) -> Option<PathBuf> {
    if !is_local_relative(specifier) {
        return None;
    }
    let base = importer_path.parent()?;
    let mut path = normalize_path(&base.join(specifier));
    if path.extension().is_none() {
        path.set_extension("ts");
    }
    Some(path)
}

fn is_local_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostics derive `Debug` but not `PartialEq`, so compare their debug
    /// renderings — enough to assert two checks produced the *same* diagnostics.
    fn debug_diags(output: &CheckOutput) -> String {
        format!("{:?}", output.diagnostics)
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
        assert_eq!(
            output
                .incomplete
                .iter()
                .map(|surface| surface.id.as_str())
                .collect::<Vec<_>>(),
            ["decl/global-declaration/self"]
        );
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
}
