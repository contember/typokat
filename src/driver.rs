//! Pipeline orchestration (mvp-plan §3): source → parse → check → diagnostics.
//!
//! The driver owns the per-run state — the bumpalo `Allocator` that backs the
//! AST and the type `Interner` — and runs the vertical slice end-to-end. Every
//! milestone keeps this end-to-end shape (mvp-plan §1.1: "vertical slice,
//! always").

use crate::check::check_program;
use crate::diagnostics::Diagnostic;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rayon::prelude::*;

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
