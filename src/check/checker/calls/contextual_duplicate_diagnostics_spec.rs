//! RED contract for duplicate diagnostics under nested contextual arguments
//! (backlog 92).
//!
//! One unresolved name nested `d` levels deep inside contextually typed arguments must
//! produce exactly ONE diagnostic, whatever `d` is. Today it produces `2^d`
//! byte-identical copies — same code, same message, same span — because two of the
//! walks an argument gets per nesting level retain their effects (the raw argument walk
//! and the committed `check_call_arguments` walk, which passes
//! `retain_contextual_arrow_checks = true`).
//!
//! Counts are pinned as EQUALITY, not presence: a checker that merely keeps reporting
//! the error is not enough, and a checker that drops it is worse. The sibling corpus
//! `tests/cases/b92_contextual_duplicate_diagnostics/` pins the same contract through
//! the conformance markers; this spec exists because it can sweep every depth without
//! one fixture per depth, and because its failure message carries the observed count.
//!
//! Every expectation here is `tsc 6.0.3 --strict --noEmit`: exactly one `TS2304` at
//! every depth from 1 to 12, on all four shapes.

use crate::check::checker::check_program;
use crate::diagnostics::DiagnosticCode;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// The deepest nesting the contract covers. Depth 10 is an ordinary callback or promise
/// chain and already yields 1 024 copies; schema builders routinely reach 12.
const MAX_DEPTH: usize = 12;

/// The single unresolved name every shape nests. Distinct enough that no rendered
/// diagnostic can contain it by accident.
const MISSING_NAME: &str = "undeclaredThing";

/// Every level is a contextually typed arrow — the shape of a nested callback chain.
/// `run`'s parameter STRUCTURALLY contains the type variable, so candidate inference
/// re-walks too and the argument is walked three times per level.
fn nested_arrows(depth: usize) -> String {
    let mut body = String::from(MISSING_NAME);
    for index in (0..depth).rev() {
        body = format!("run(v{index} => {body})");
    }
    format!("declare function run<T>(step: (value: number) => T): T;\nconst nested = {body};\n")
}

/// Every level is a fresh object literal passed through a generic call — the
/// fresh-literal contextual path rather than the arrow path, also structurally generic.
fn nested_object_literals(depth: usize) -> String {
    let mut body = String::from(MISSING_NAME);
    for _ in 0..depth {
        body = format!("wrap({{ inner: {body} }})");
    }
    format!("declare function wrap<T>(value: {{ inner: T }}): T;\nconst nested = {body};\n")
}

/// The bare-type-variable discriminator (real `zod`'s `object<T>(shape: T)`). A
/// parameter that IS a type variable never re-walks during candidate inference, so this
/// shape costs two walks per level rather than three — but both of those walks retain,
/// so the duplicate count is the same `2^depth`. One of the two shapes that hang in the
/// wild, so it is pinned independently.
fn bare_type_variable(depth: usize) -> String {
    let mut body = String::from(MISSING_NAME);
    for _ in 0..depth {
        body = format!("shapeOf({{ inner: {body} }})");
    }
    format!("declare function shapeOf<T>(shape: T): T;\nconst nested = {body};\n")
}

/// The non-generic-callback discriminator (real `describe`/`it`). No inference phase at
/// all, so again two walks per level and again both retain.
fn non_generic_callback(depth: usize) -> String {
    let mut body = format!("{MISSING_NAME};");
    for _ in 0..depth {
        body = format!("describe(() => {{ {body} }});");
    }
    format!("declare function describe(fn: () => void): void;\n{body}\n")
}

/// Check one generated source and return every diagnostic as `(code, rendered text)`.
fn check_diagnostics(source: &str) -> Vec<(DiagnosticCode, String)> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    let checked = check_program(&mut interner, &parsed.program);
    assert!(checked.incomplete.is_empty(), "{:?}", checked.incomplete);
    checked
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.rendered_text()))
        .collect()
}

/// Assert that `shape` reports the nested unresolved name exactly once at every depth
/// from 1 to [`MAX_DEPTH`], collecting every depth before failing so the growth law is
/// visible in one run.
fn reports_the_nested_error_exactly_once(shape: &str, source: fn(usize) -> String) {
    let mut violations = Vec::new();
    for depth in 1..=MAX_DEPTH {
        let diagnostics = check_diagnostics(&source(depth));

        // The shape must produce nothing but the one unresolved-name error, so a fix
        // cannot trade duplicates for a different diagnostic.
        let foreign: Vec<&str> = diagnostics
            .iter()
            .filter(|(code, text)| *code != DiagnosticCode::TK2304 || !text.contains(MISSING_NAME))
            .map(|(_, text)| text.as_str())
            .collect();
        if !foreign.is_empty() {
            violations.push(format!(
                "{shape} at depth {depth}: expected only the nested `{MISSING_NAME}` \
                 TK2304, also got {foreign:?}"
            ));
            continue;
        }

        // The contract: exactly one. `tsc 6.0.3 --strict` reports one TS2304 here at
        // every depth; the copies typokat adds are byte-identical, so nothing but the
        // count distinguishes them.
        if diagnostics.len() != 1 {
            violations.push(format!(
                "{shape} at depth {depth}: expected exactly 1 TK2304, got {} \
                 (2^{depth} = {}); tsc 6.0.3 --strict reports 1",
                diagnostics.len(),
                1usize << depth,
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "a single error nested inside contextual arguments is reported more than once:\n{}",
        violations.join("\n"),
    );
}

#[test]
fn nested_contextual_arrows_report_one_error_once() {
    reports_the_nested_error_exactly_once("nested contextual arrows", nested_arrows);
}

#[test]
fn nested_object_literals_report_one_error_once() {
    reports_the_nested_error_exactly_once("nested object literals", nested_object_literals);
}

#[test]
fn nested_bare_type_variable_arguments_report_one_error_once() {
    reports_the_nested_error_exactly_once(
        "nested bare-type-variable arguments",
        bare_type_variable,
    );
}

#[test]
fn nested_non_generic_callbacks_report_one_error_once() {
    reports_the_nested_error_exactly_once("nested non-generic callbacks", non_generic_callback);
}
