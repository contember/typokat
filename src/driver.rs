//! Pipeline orchestration: source → parse → check → diagnostics.
//!
//! The driver owns the per-run allocator that backs the AST and the type
//! `Interner`, keeping borrowed parser data inside the parse/check call.

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
    /// Parser/syntax errors rendered to strings so the CLI can report malformed input.
    pub parse_errors: Vec<String>,
}

impl CheckOutput {
    /// Whether the run found any problem (type or parse). Drives the CLI exit
    /// code.
    pub fn has_errors(&self) -> bool {
        !self.parse_errors.is_empty() || self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Parse and check one TypeScript source. The local `Allocator` owns the AST only
/// for this call; owned diagnostics are extracted before it drops.
pub fn check_source(source: &str) -> CheckOutput {
    let allocator = Allocator::default();
    // TypeScript, non-JSX, module semantics.
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
/// only `./` / `../` specifiers among the provided `.ts` files.
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

    /// The diagnostic codes emitted for `source`, in order.
    fn codes(output: &CheckOutput) -> Vec<&'static str> {
        output
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect()
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
}
