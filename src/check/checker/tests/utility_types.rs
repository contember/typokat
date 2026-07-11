//! M28 end-to-end tests for the prelude compilation unit.
//! Pins clean prelude checking, user shadowing, and `Pick` modifier-source
//! preservation. Fixture acceptance lives in `m28_utility_types/`.

use crate::check::checker::PRELUDE_SOURCE;
use crate::driver::{check_project, check_source, FileInput};

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

/// The prelude checks clean both as a trusted asset and as user source, proving
/// full user shadowing without duplicate-name or lowering diagnostics.
#[test]
fn prelude_checks_clean() {
    assert!(
        diags("").is_empty(),
        "an empty program must be clean (the prelude never leaks diagnostics)"
    );
    assert!(
        diags(PRELUDE_SOURCE).is_empty(),
        "the prelude source must also check clean as ordinary user code"
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
