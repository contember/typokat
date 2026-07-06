//! Pipeline orchestration (mvp-plan §3): source → parse → check → diagnostics.
//!
//! The driver owns the per-run state — the bumpalo `Allocator` that backs the
//! AST and the type `Interner` — and runs the vertical slice end-to-end. Every
//! milestone keeps this end-to-end shape (mvp-plan §1.1: "vertical slice,
//! always").

use crate::check::{
    check_program, check_project_programs, ProjectImport, ProjectImportSource, ProjectProgram,
};
use crate::diagnostics::Diagnostic;
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

/// The outcome of checking one source file.
pub struct CheckOutput {
    /// Type diagnostics produced by the checker (empty == clean).
    pub diagnostics: Vec<Diagnostic>,
    /// Parser/syntax errors rendered to strings. M0 fixtures are syntactically
    /// valid, so this is normally empty; surfaced so the CLI can report a
    /// malformed file instead of silently checking an empty AST.
    pub parse_errors: Vec<String>,
}

impl CheckOutput {
    /// Whether the run found any problem (type or parse). Drives the CLI exit
    /// code.
    pub fn has_errors(&self) -> bool {
        !self.parse_errors.is_empty() || self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Parse and check `source` as a TypeScript file, returning its diagnostics.
///
/// The `Allocator` is created and owned locally; the AST it backs never escapes
/// this function (we extract owned `Diagnostic`s before it drops), so there are
/// no dangling-lifetime hazards. `strictNullChecks` is on by default — it is
/// encoded directly in the relation rules, not a parser flag.
pub fn check_source(source: &str) -> CheckOutput {
    let allocator = Allocator::default();
    // TypeScript, non-JSX, module semantics — the M0 subset.
    let source_type = SourceType::ts();

    let parsed = Parser::new(&allocator, source, source_type).parse();

    // Collect parser diagnostics as strings (their own types borrow elsewhere;
    // we only need them rendered for the CLI).
    let parse_errors: Vec<String> = parsed
        .diagnostics
        .iter()
        .map(|d| d.to_string())
        .collect();

    // If the parser bailed entirely, the AST is empty — there is nothing to
    // check, and the parse errors carry the explanation.
    if parsed.panicked {
        return CheckOutput {
            diagnostics: Vec::new(),
            parse_errors,
        };
    }

    let mut interner = Interner::with_intrinsics();
    let diagnostics = check_program(&mut interner, &parsed.program);

    CheckOutput {
        diagnostics,
        parse_errors,
    }
}

/// One file handed to [`check_files`]: a display `name` (used only for rendering
/// and for correlating results back to inputs) and its full `source` text.
///
/// Both are owned `String`s on purpose. The oxc AST is neither `Send` nor `Sync`
/// (an arena `Vec` is deliberately `!Send`, and AST nodes hold `Cell`s, so it is
/// `!Sync`) — it is pinned to the thread that parses it and can neither be moved
/// to another thread nor shared by reference. So the source must travel to the
/// worker as owned data, and the *entire* parse→check pipeline for a file has to
/// run on one thread (architecture §8).
pub struct FileInput {
    pub name: String,
    pub source: String,
}

/// The result of checking one [`FileInput`], carrying the input back alongside
/// its diagnostics.
///
/// `name` and `source` are *moved through* the pipeline (not re-cloned) and
/// returned so a caller can both render diagnostics — codespan needs the source —
/// and correlate each result to its input without a side table. In the `Vec`
/// returned by [`check_files`], `reports[i]` is the result of `inputs[i]`.
pub struct FileReport {
    pub name: String,
    pub source: String,
    pub output: CheckOutput,
}

/// Check many files in parallel — one fully independent pipeline per file.
///
/// Each file's whole parse→bind→check pipeline runs on its own rayon worker with
/// its own `Allocator` and its own `Interner`; this is exactly [`check_source`]
/// fanned out across files. Because the AST is `!Send + !Sync` it can never leave
/// the thread that parsed it, which makes the *per-file pipeline* — not a shared,
/// serially-checked interner — the natural unit of parallelism (architecture §8).
/// There is no cross-file name/type resolution today (modules are out of scope),
/// so per-file interners are not merely sound but lossless: the interner is a
/// per-run dedup + relation cache, and nothing observable crosses a file boundary.
///
/// Order is preserved (`reports[i]` ↔ `inputs[i]`), and only owned, `Send` data
/// (`FileReport`) crosses back from the workers — so the result is deterministic
/// regardless of the order in which workers happen to finish.
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

/// Check a local relative-module project in one serial type universe.
///
/// This is the M29 correctness-first API. It deliberately lives beside
/// [`check_files`]: the old multi-file API remains per-file independent and
/// parallel, while this path resolves only `./` / `../` specifiers among the
/// provided `.ts` files and checks them in dependency order with one `Interner`.
pub fn check_project(inputs: Vec<FileInput>) -> Vec<FileReport> {
    if inputs.is_empty() {
        return Vec::new();
    }

    let source_type = SourceType::ts();
    let allocators: Vec<Allocator> = (0..inputs.len()).map(|_| Allocator::default()).collect();
    let parsed: Vec<_> = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, source_type).parse())
        .collect();

    let parse_errors: Vec<Vec<String>> = parsed
        .iter()
        .map(|parsed| parsed.diagnostics.iter().map(|d| d.to_string()).collect())
        .collect();

    let paths = normalized_input_paths(&inputs);
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
        .map(|&original| ProjectProgram {
            program: &parsed[original].program,
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
                    span: import.span,
                })
                .collect(),
        })
        .collect();

    let mut interner = Interner::with_intrinsics();
    let ordered_diagnostics = check_project_programs(&mut interner, &project_units);
    let mut diagnostics_by_original: Vec<Vec<Diagnostic>> =
        (0..inputs.len()).map(|_| Vec::new()).collect();
    for (ordered, &original) in order.iter().enumerate() {
        if let Some(diagnostics) = ordered_diagnostics.get(ordered) {
            diagnostics_by_original[original] = diagnostics.clone();
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
            },
        })
        .collect()
}

#[derive(Clone)]
struct RawImport {
    local: String,
    imported: String,
    specifier: String,
    source: RawImportSource,
    type_only: bool,
    span: Span,
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
                    span: Span::from_oxc(named.span),
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

    /// A clean file, a type-error file, and a syntactically broken file — checked
    /// together — must each produce exactly what [`check_source`] produces for that
    /// source alone, in input order. This pins the core contract: parallel
    /// multi-file checking is per-file-independent (no cross-file leakage) and
    /// order-preserving.
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
}
