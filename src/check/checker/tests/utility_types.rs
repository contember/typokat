//! M28 end-to-end tests for the prelude compilation unit.
//! Pins clean prelude checking, user shadowing, and `Pick` modifier-source
//! preservation. Fixture acceptance lives in `m28_utility_types/`.

use crate::check::checker::PRELUDE_SOURCE;
use crate::driver::check_source;

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
