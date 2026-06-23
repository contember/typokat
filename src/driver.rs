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
