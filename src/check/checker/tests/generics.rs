//! M9 end-to-end tests for generics.
//! Pins substitution through instantiated signatures/bodies, stable interning,
//! type-parameter scoping, and graceful wrong-arity handling.

use crate::check::test_support::check_source;

/// Run the checker and return the sorted `(1-based line, code)` of every
/// diagnostic, keyed on its primary-span start line (matching the conformance
/// harness's line mapping).
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

/// A generic function instantiated with explicit type args: the instantiated
/// **return** drives `TK2322` and the instantiated **parameter** drives
/// `TK2345`, while the correctly-typed call is clean. This is the headline
/// `generic_functions.ts` behaviour as a unit-level pin.
#[test]
fn generic_function_instantiates_return_and_parameter() {
    let src = "\
function identity<T>(x: T): T { return x; }
const a: number = identity<number>(5);
const b: number = identity<string>(\"s\");
const c = identity<number>(\"s\");
";
    // `identity<number>(5)` (line 2) is clean; `identity<string>` returns string
    // → line 3 TK2322; the arg "s" is not number → line 4 TK2345.
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (4, "TK2345".to_string())]
    );
}

/// Two type parameters substitute independently and positionally: `pick<A, B>`
/// returns `A`, so `pick<string, number>` returns `string`.
#[test]
fn generic_function_with_two_type_parameters() {
    let src = "\
function pick<A, B>(a: A, b: B): A { return a; }
const d: number = pick<number, string>(1, \"x\");
const e: number = pick<string, number>(\"x\", 1);
";
    // `pick<number, string>` returns number (line 2 clean); `pick<string,
    // number>` returns string (line 3 TK2322).
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// A generic interface instantiated with a type argument: the instantiated
/// object body drives `TK2322` (wrong member type) and `TK2353` (excess
/// property on the instantiated type).
#[test]
fn generic_interface_instantiates_body() {
    let src = "\
interface Box<T> { value: T; }
const x: Box<number> = { value: 1 };
const y: Box<number> = { value: \"s\" };
const z: Box<number> = { value: 1, extra: 2 };
";
    // `{ value: 1 }` is a `Box<number>` (line 2 clean); `{ value: "s" }` is not
    // (line 3 TK2322); `extra` is excess on the instantiated type (line 4 TK2353).
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (4, "TK2353".to_string())]
    );
}

/// A generic type alias `Pair<A, B>` instantiates both parameters.
#[test]
fn generic_alias_instantiates_both_parameters() {
    let src = "\
type Pair<A, B> = { first: A; second: B };
const p: Pair<number, string> = { first: 1, second: \"s\" };
const q: Pair<number, string> = { first: \"s\", second: 1 };
";
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// Nested instantiation `Box<Box<number>>`: substitution flows through the
/// nested generic, so the inner member's type is checked too.
#[test]
fn nested_generic_instantiation_flows_through() {
    let src = "\
interface Box<T> { value: T; }
const nn: Box<Box<number>> = { value: { value: 1 } };
const mm: Box<Box<number>> = { value: { value: \"s\" } };
";
    // The well-typed nested literal (line 2) is clean; the inner `"s"` (line 3)
    // is not assignable to the substituted inner `number` → TK2322.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// A generic type nested in a generic function: `unwrap<T>(b: Box<T>): T`
/// substitutes `Box<T>` and the return. A fresh object-literal argument reports
/// assignment-style member mismatch (`TK2322`), matching tsc's contextual literal
/// diagnostics.
#[test]
fn generic_type_nested_in_generic_function() {
    let src = "\
interface Box<T> { value: T; }
function unwrap<T>(b: Box<T>): T { return b.value; }
const n: number = unwrap<number>({ value: 1 });
const m: number = unwrap<string>({ value: \"s\" });
const bad = unwrap<number>({ value: \"s\" });
";
    // `unwrap<number>({value:1})` is clean (line 3); `unwrap<string>` returns
    // string (line 4 TK2322); `{value:"s"}` has a value member incompatible with
    // `Box<number>` (line 5 TK2322).
    assert_eq!(
        diags(src),
        vec![(4, "TK2322".to_string()), (5, "TK2322".to_string())]
    );
}

/// Type parameters are scoped to their declaration: out-of-scope `T` is
/// unresolved and must not leak into another generic's checks.
#[test]
fn type_parameter_does_not_leak_across_declarations() {
    let src = "\
function first<T>(x: T): T { return x; }
function second(y: number): number { return y; }
const ok: number = second(1);
const bad: number = second(\"s\");
";
    // `second` is non-generic; its `number` parameter rejects the string arg
    // (line 4 TK2345). `T` from `first` never leaks to affect `second`.
    assert_eq!(diags(src), vec![(4, "TK2345".to_string())]);
}

/// A type parameter **shadows** a same-named named type inside the generic, and
/// the shadowing does not escape: outside the generic, the named type is seen
/// again. `T` (the alias `= string`) is shadowed by the parameter `T` inside
/// `f`, so `f<number>(5)` is fine; outside, `T` is `string`.
#[test]
fn type_parameter_shadows_named_type_only_inside() {
    let src = "\
type T = string;
function f<T>(x: T): T { return x; }
const a: number = f<number>(5);
const outside: T = \"s\";
const bad: T = 5;
";
    // Inside `f`, `T` is the parameter (so `f<number>(5)` returns number → line 3
    // clean). Outside, `T` is the alias `string`: line 4 clean, line 5 TK2322
    // (number not assignable to string).
    assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
}

/// `Box<number>` and `Box<string>` are **distinct** instantiations: assigning a
/// `Box<string>`-shaped literal to a `Box<number>` annotation errors, confirming
/// the two instantiations are different interned types.
#[test]
fn distinct_instantiations_are_not_interchangeable() {
    let src = "\
interface Box<T> { value: T; }
const a: Box<number> = { value: 1 };
const b: Box<string> = { value: 1 };
";
    // `{ value: 1 }` is a `Box<number>` (line 2 clean) but not a `Box<string>`
    // (line 3 TK2322) — the instantiations are distinct.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// Persistent generic function signatures reject an explicit wrong type-argument
/// count with `TK2558`. Generic type-reference arity remains the older M9
/// best-effort path, so this test keeps one control for that separate surface.
#[test]
fn wrong_function_type_argument_count_reports_tk2558() {
    // Too few explicit arguments for `pick` now uses the persistent function
    // descriptor and reports `TK2558`; the unrelated `Box` type reference still
    // exercises the established graceful type-reference path.
    let src = "\
function pick<A, B>(a: A, b: B): A { return a; }
interface Box<T> { value: T; }
const p = pick<number>(1, 2);
const x: Box<number> = { value: 1 };
type Bad = Box<number, string>;
const y: Bad = { value: 1 };
";
    assert_eq!(
        diags(src),
        vec![(3, "TK2558".to_string()), (5, "TK2314".to_string())]
    );
}
