//! M28 end-to-end tests for the prelude compilation unit.
//! Pins clean prelude checking, user shadowing, and `Pick` modifier-source
//! preservation. Fixture acceptance lives in `m28_utility_types/`.

use super::super::{
    expected_trusted_prelude_incomplete, trusted_prelude_records_are_clean, TEST_AMBIENT_SOURCE,
};
use crate::binder::bind_module_with_prelude;
use crate::check::test_support::{check_project, check_source};
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::frontend::FileInput;
use crate::source::ModuleOrdinal;
use crate::span::Span;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::BTreeMap;

/// Run the checker and return the sorted `(1-based line, code)` of every
/// diagnostic, keyed on its primary-span start line (matching the conformance
/// harness's mapping).
fn diags(source: &str) -> Vec<(u32, String)> {
    let out = check_source(source);
    assert!(
        out.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        out.parse_errors
    );
    let index = crate::span::LineIndex::new(source);
    let mut v: Vec<(u32, String)> = out
        .diagnostics
        .iter()
        .map(|d| (index.line_of(d.span.start), d.code.as_str().to_string()))
        .collect();
    v.sort();
    v
}

/// Run one project module and preserve the single-file helper's exact line/code shape.
fn project_diags(source: &str) -> Vec<(u32, String)> {
    let reports = check_project(vec![FileInput {
        name: "module.ts".to_string(),
        source: source.to_string(),
    }]);
    assert_eq!(
        reports.len(),
        1,
        "one input must produce one project report"
    );
    let output = &reports[0].output;
    assert!(
        output.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        output.parse_errors
    );
    let index = crate::span::LineIndex::new(source);
    let mut diagnostics: Vec<(u32, String)> = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str().to_string(),
            )
        })
        .collect();
    diagnostics.sort();
    diagnostics
}

/// The prelude checks clean both as a trusted asset and as user source, proving
/// full user shadowing without duplicate-name or lowering diagnostics.
#[test]
fn prelude_checks_clean() {
    assert!(
        diags("").is_empty(),
        "an empty program must be clean (the prelude never leaks diagnostics)"
    );
    assert!(
        diags(TEST_AMBIENT_SOURCE).is_empty(),
        "the prelude source must also check clean as ordinary user code"
    );
}

#[test]
fn trusted_prelude_cleanliness_allowlist_is_exact() {
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, TEST_AMBIENT_SOURCE, SourceType::ts()).parse();
    let user_allocator = Allocator::default();
    let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
    let binder = bind_module_with_prelude(&prelude.program, &user.program);
    let expected = expected_trusted_prelude_incomplete(&binder, &prelude.program);
    assert!(
        expected.is_some(),
        "the trusted intrinsic aliases must have intrinsic bodies"
    );
    let Some(expected) = expected else {
        return;
    };
    assert_eq!(expected.len(), 6);

    let clean = BTreeMap::from([(ModuleOrdinal::new(0), (Vec::new(), expected.clone()))]);
    assert!(trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &clean
    ));

    let mut missing = clean.clone();
    if let Some((_, incomplete)) = missing.get_mut(&ModuleOrdinal::new(0)) {
        incomplete.pop();
    }
    assert!(!trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &missing
    ));

    let mut extra = clean.clone();
    if let Some((_, incomplete)) = extra.get_mut(&ModuleOrdinal::new(0)) {
        if let Some(record) = incomplete.first().cloned() {
            incomplete.push(record);
        }
    }
    assert!(!trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &extra
    ));

    let mut wrong_id = clean.clone();
    if let Some((_, incomplete)) = wrong_id.get_mut(&ModuleOrdinal::new(0)) {
        if let Some(record) = incomplete.first_mut() {
            record.id = "annotation-lower/intrinsic-keyword/other".to_string();
        }
    }
    assert!(!trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &wrong_id
    ));

    let mut wrong_span = clean.clone();
    if let Some((_, incomplete)) = wrong_span.get_mut(&ModuleOrdinal::new(0)) {
        if let Some(record) = incomplete.first_mut() {
            record.span = Span::new(0, 0);
        }
    }
    assert!(!trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &wrong_span
    ));

    let mut diagnostic = clean;
    if let Some((diagnostics, _)) = diagnostic.get_mut(&ModuleOrdinal::new(0)) {
        diagnostics.push(Diagnostic::cannot_find_name(Span::new(0, 0), "Broken"));
    }
    assert!(!trusted_prelude_records_are_clean(
        &binder,
        &prelude.program,
        &diagnostic
    ));
}

/// Single-file and one-module project bootstrap must expose identical trusted surfaces.
#[test]
fn single_file_and_project_trusted_prelude_surfaces_have_exact_diagnostic_parity() {
    let src = "\
const uppercaseOk: Uppercase<\"word\"> = \"WORD\";
const uppercaseWrong: Uppercase<\"word\"> = \"word\";
const lowercaseOk: Lowercase<\"WORD\"> = \"word\";
const lowercaseWrong: Lowercase<\"WORD\"> = \"WORD\";
const capitalizeOk: Capitalize<\"word\"> = \"Word\";
const capitalizeWrong: Capitalize<\"word\"> = \"word\";
const uncapitalizeOk: Uncapitalize<\"WordWORD\"> = \"wordWORD\";
const uncapitalizeWrong: Uncapitalize<\"WordWORD\"> = \"wordword\";
type Context = { n: number; check(): void } & ThisType<{ n: string }>;
const contextual: Context = {
  n: 1,
  check() {
    const wrongThis: number = this.n;
  },
};
type WithThis = (this: { tag: string }, value: number) => string;
declare const withoutThis: OmitThisParameter<WithThis>;
const omittedResult: string = withoutThis(1);
withoutThis(\"wrong\");
const mathResult: number = Math.abs(-1);
Math.abs(\"wrong\");
";
    let expected = vec![
        (2, "TK2322".to_string()),
        (4, "TK2322".to_string()),
        (6, "TK2322".to_string()),
        (8, "TK2322".to_string()),
        (13, "TK2322".to_string()),
        (19, "TK2345".to_string()),
        (21, "TK2345".to_string()),
    ];

    assert_eq!(diags(src), expected);
    assert_eq!(project_diags(src), expected);
}

#[test]
fn published_omit_this_parameter_marker_preserves_specialized_declaration_surface() {
    let source = r#"interface Holder {
  x: OmitThisParameter<string>;
}
declare const holder: Holder;
const accepted: string = holder.x;
const rejected: number = holder.x;
"#;
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [(
            DiagnosticCode::TK2322,
            "rejected",
            "Type 'string' is not assignable to type 'number'",
        )],
        "the frozen prelude's published marker must retain its applied argument"
    );
}

/// User type/value declarations shadow only their matching prelude spaces in both modes.
#[test]
fn single_file_and_project_prelude_shadowing_have_exact_diagnostic_parity() {
    let src = "\
type Uppercase<T> = number;
const shadowedUppercaseOk: Uppercase<\"x\"> = 1;
const shadowedUppercaseWrong: Uppercase<\"x\"> = \"wrong\";
declare const console: { log(value: number): void };
console.log(1);
console.log(\"wrong\");
";
    let expected = vec![(3, "TK2322".to_string()), (6, "TK2345".to_string())];

    assert_eq!(diags(src), expected);
    assert_eq!(project_diags(src), expected);
}

/// `ThisType<T>` is structurally transparent in its contextual intersection, but
/// its first source-order marker supplies `this` to object-literal methods even
/// through a transparent alias instantiation.
#[test]
fn this_type_context_uses_first_marker_through_alias() {
    let src = "\
type Marker<T> = ThisType<T>;
type Context = { n: number; check(): void } & Marker<{ n: string }> & ThisType<{ n: number }>;
const value: Context = {
  n: 1,
  check() {
    const wrong: number = this.n;
  },
};
";
    assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
}

/// Nested intersections must retain the first source-order marker even when it
/// is introduced by a transparent generic alias.
#[test]
fn this_type_context_first_marker_wins_across_nested_alias_intersections() {
    let src = "\
type A = { n: string };
type B = { n: number };
type AliasContainingThisType<T> = { check(): void } & ThisType<T>;
type FirstB = ThisType<B> & AliasContainingThisType<A> & { n: number };
const firstB: FirstB = { n: 1, check() { const wrong: string = this.n; } };
type FirstA = AliasContainingThisType<A> & ThisType<B> & { n: number };
const firstA: FirstA = { n: 1, check() { const wrong: number = this.n; } };
";
    assert_eq!(
        diags(src),
        vec![(5, "TK2322".to_string()), (7, "TK2322".to_string())]
    );
}

#[test]
fn generic_member_receivers_contribute_to_inference_without_becoming_arguments() {
    let src = "\
declare const generic: <T>(this: T) => T;
const holder = { generic };
const inferred: { generic: <T>(this: T) => T } = holder.generic();
declare const pair: <T>(this: T, value: T) => T;
const pairHolder = { pair, own: 1 };
const conflict = pairHolder.pair({ foreign: \"x\" });
type Overloaded = { <T>(this: T, value: number): T; (this: { tag: string }, value: string): string };
declare const overloaded: Overloaded;
const overloadHolder = { overloaded };
const inferredOverload: { overloaded: Overloaded } = overloadHolder.overloaded(1);
";
    assert_eq!(diags(src), vec![(6, "TK2684".to_string())]);
}

/// Computed member calls preserve the same receiver for `this` checks and generic
/// receiver inference as direct member calls, including parenthesized forms.
#[test]
fn computed_member_calls_preserve_receiver_context() {
    let src = "\
interface Good { n: number; method(this: { n: number }): void; }
declare const good: Good;
good[\"method\"]();
(good[\"method\"])();
((good)[\"method\"])();
interface Bad { method(this: { n: number }): void; }
declare const bad: Bad;
bad[\"method\"]();
(bad[\"method\"])();
((bad)[\"method\"])();
interface Generic { tag: \"holder\"; method<T>(this: T): T; }
declare const generic: Generic;
const computed: Generic = generic[\"method\"]();
const parenthesized: Generic = (generic[\"method\"])();
";
    assert_eq!(
        diags(src),
        vec![
            (8, "TK2684".to_string()),
            (9, "TK2684".to_string()),
            (10, "TK2684".to_string()),
        ]
    );
}

/// A user declaration **shadows** the prelude alias of the same name (tsc-like):
/// `Partial` here is the user's `number`-alias, so a string initializer is a plain
/// TK2322 against `number` — and there is exactly ONE diagnostic (no duplicate-name
/// noise from the shadowing itself, no prelude mapped semantics bleeding through).
#[test]
fn user_declaration_shadows_prelude_alias() {
    let src = "\
type Partial<T> = number;
const x: Partial<{ a: string }> = \"s\";
";
    assert_eq!(
        diags(src),
        vec![(2, "TK2322".to_string())],
        "the user Partial (= number) must win over the prelude's"
    );
}

/// Prelude values are lowered in the prelude pass and their declaration table is
/// carried into the single-file user pass, so calls do not silently use `error`.
#[test]
fn prelude_values_reach_single_file_user_pass() {
    let src = "\
console.log(\"ready\", 1, { enabled: true });
Math.max(1, 2, 3);
Math.abs(\"wrong\");
const wrongResult: string = Math.ceil(1);
";
    assert_eq!(
        diags(src),
        vec![(3, "TK2345".to_string()), (4, "TK2322".to_string())],
        "prelude calls retain their parameter and return types in user code"
    );
}

/// A user value declaration naturally shadows the inherited prelude value through
/// the binder's inner scope, with no duplicate declaration diagnostic.
#[test]
fn user_declaration_shadows_prelude_value() {
    let src = "\
declare const console: { log(value: number): void };
console.log(\"wrong\");
";
    assert_eq!(
        diags(src),
        vec![(2, "TK2345".to_string())],
        "the user console declaration must replace the inherited rest signature"
    );
}

/// A local type-only declaration does not mask the inherited `Math` value. Value
/// lookup continues to the parent scope until it finds a value-space slot.
#[test]
fn local_type_does_not_shadow_prelude_value() {
    let src = "\
type Math = { local: true };
Math.abs(\"wrong\");
";
    assert_eq!(
        diags(src),
        vec![(2, "TK2345".to_string())],
        "a type-only Math declaration must not hide the inherited Math value"
    );
}

/// A local value-only declaration does not mask the inherited `Partial` type.
/// Type lookup continues to the parent scope until it finds a type-space slot.
#[test]
fn local_value_does_not_shadow_prelude_type() {
    let src = "\
declare const Partial: number;
const bad: Partial<{ a: string }> = \"wrong\";
";
    assert_eq!(
        diags(src),
        vec![(2, "TK2322".to_string())],
        "a value-only Partial declaration must not hide the inherited Partial type"
    );
}

/// Project mode uses the same prelude pass and value-table handoff for every user
/// module, including modules after the first one in serial checking order.
#[test]
fn prelude_values_reach_project_user_passes() {
    let reports = check_project(vec![
        FileInput {
            name: "first.ts".to_string(),
            source: "console.error({ message: \"failed\" });".to_string(),
        },
        FileInput {
            name: "second.ts".to_string(),
            source: "Math.min(1, \"wrong\");\nconst wrongResult: string = Math.round(1);"
                .to_string(),
        },
    ]);

    assert!(
        reports[0].output.diagnostics.is_empty(),
        "the first project module receives the inherited console value"
    );
    let codes: Vec<String> = reports[1]
        .output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect();
    assert_eq!(codes, vec!["TK2345", "TK2322"]);
}

/// Project modules share the same slot-aware parent traversal as a single-file
/// pass, so a cross-space local declaration cannot hide an inherited prelude slot.
#[test]
fn cross_space_lookup_reaches_prelude_in_project_passes() {
    let reports = check_project(vec![
        FileInput {
            name: "value.ts".to_string(),
            source: "type Math = { local: true };\nMath.abs(\"wrong\");".to_string(),
        },
        FileInput {
            name: "type.ts".to_string(),
            source:
                "declare const Partial: number;\nconst bad: Partial<{ a: string }> = \"wrong\";"
                    .to_string(),
        },
    ]);

    let first_codes: Vec<String> = reports[0]
        .output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect();
    assert_eq!(first_codes, vec!["TK2345"]);

    let second_codes: Vec<String> = reports[1]
        .output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect();
    assert_eq!(second_codes, vec!["TK2322"]);
}

/// `Pick` preserves the picked member's value type and optionality via the M28
/// modifiers source.
#[test]
fn pick_preserves_value_types_and_optionality() {
    let src = "\
type P = { a: number; b?: string };
const ok: Pick<P, \"b\"> = {};
const bad: Pick<P, \"b\"> = { b: 5 };
";
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string())],
        "picked b stays optional (line 2 clean) and stays string-typed (line 3 errors)"
    );
}

/// The `Omit` composition resolves through the whole M28 chain — lazy `Pick`
/// instantiation, `Exclude` distribution over an evaluated **deferred `keyof`**,
/// and the modifiers source: `Omit<P, "a">` is `{ b?: string }`, so reading the
/// kept member types correctly and the omitted member is gone (TK2339).
#[test]
fn omit_composition_resolves_member_access() {
    let src = "\
type P = { a: number; b?: string };
declare const o: Omit<P, \"a\">;
const kept: string | undefined = o.b;
const gone: number = o.a;
";
    assert_eq!(
        diags(src),
        vec![(4, "TK2339".to_string())],
        "kept member reads at its source type; the omitted member does not exist"
    );
}

/// A **symbolic** string intrinsic (`Uppercase<S>` over a still-free parameter)
/// relates per the WU3 contract: assignable to `string` (every application denotes
/// SOME string), NOT assignable to a narrower literal, and nothing flows INTO it.
#[test]
fn symbolic_intrinsic_relates_conservatively_to_string_only() {
    let src = "\
function f<S extends string>(s: S, u: Uppercase<S>): void {
  const widened: string = u;
  const narrowed: \"A\" = u;
  const into: Uppercase<S> = s;
}
";
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (4, "TK2322".to_string())],
        "symbolic Uppercase flows to string, not to a literal, and accepts nothing"
    );
}
