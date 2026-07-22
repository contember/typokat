//! M16 end-to-end tests for generic classes.
//! Pins class type-parameter substitution, distinct instantiations, constructor
//! argument checking, inference, and `C<args>` type annotations. Fixture
//! acceptance lives in `m16_generic_classes/`.

use crate::driver::check_source;

/// Run the checker and return the sorted `(1-based line, code)` of every diagnostic,
/// keyed on its primary-span start line (matching the conformance harness's mapping).
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

#[test]
fn qualified_namespace_generic_construction_uses_class_inference_and_respects_value_shadowing() {
    let src = r#"namespace DeltaSpace {
  export class Box<T> { constructor(public value: T) {} }
}
const good: DeltaSpace.Box<string> = new DeltaSpace.Box("ok");
const inferredBad: DeltaSpace.Box<string> = new DeltaSpace.Box(1);
const explicitBad = new DeltaSpace.Box<string>(1);
namespace ShadowSpace {
  export class Box { constructor(value: string) {} }
}
function shadow(ShadowSpace: { Box: new (value: number) => { value: number } }) {
  const local = new ShadowSpace.Box(1);
  const value: number = local.value;
}
"#;
    let checked = check_source(src);
    assert!(checked.parse_errors.is_empty());
    assert!(checked.incomplete.is_empty(), "{:#?}", checked.incomplete);
    let index = crate::span::LineIndex::new(src);
    let diagnostics = checked
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str().to_string(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        diagnostics,
        vec![(5, "TK2322".to_string()), (6, "TK2345".to_string())],
    );
}

/// Explicit type arguments instantiate the constructor + instance: `new Box<number>(1)`
/// types the constructor `(v: number)` and the instance `{ value: number; get: () =>
/// number }`, so `b.get()` is `number`. A wrong target annotation (`bad: string`) is
/// `TK2322`; a wrong constructor argument (`new Box<number>("s")`) is `TK2345`.
#[test]
fn explicit_type_arguments_substitute_ctor_and_members() {
    let src = "\
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
  get(): T {
return this.value;
  }
}
const b = new Box<number>(1);
const n: number = b.get();   // ok
const bad: string = b.get(); // TK2322
const e = new Box<number>(\"s\"); // TK2345
";
    // `b.get()` is the substituted `number`: line 11 ok, line 12 (string target) TK2322,
    // line 13 (string arg vs number param) TK2345.
    assert_eq!(
        diags(src),
        vec![(12, "TK2322".to_string()), (13, "TK2345".to_string())]
    );
}

/// No type arguments → the parameter is **inferred** from the constructor argument:
/// `new Box(5)` infers `T = number`, so `inf.get()` is `number` — a `string` target is
/// `TK2322`, a `number` target is clean.
#[test]
fn inferred_type_argument_from_constructor_argument() {
    let src = "\
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
  get(): T {
return this.value;
  }
}
const inf = new Box(5);
const m: number = inf.get();  // ok
const m2: string = inf.get(); // TK2322
";
    // `T` inferred `number` from `5`: line 11 ok, line 12 (string target) TK2322.
    assert_eq!(diags(src), vec![(12, "TK2322".to_string())]);
}

/// Two distinct instantiations are distinct types: `Box<number>` is not assignable to
/// `Box<string>` (their `value`/`get` members differ structurally) → `TK2322`, while a
/// matching annotation (`Box<number>`) is clean.
#[test]
fn distinct_instantiations_are_distinct_types() {
    let src = "\
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
  get(): T {
return this.value;
  }
}
const x: Box<number> = new Box<number>(1); // ok
const y: Box<string> = new Box<number>(1); // TK2322
";
    // `Box<number>` ≠ `Box<string>`: only line 11 errors (TK2322).
    assert_eq!(diags(src), vec![(11, "TK2322".to_string())]);
}

/// A multi-parameter generic class substitutes each parameter independently:
/// `Pair<number, string>` types `first: number`, `second: string`. A swapped target
/// (`bad: string = p.first`) is `TK2322`; swapped constructor arguments fail per-arg
/// (`TK2345` each).
#[test]
fn multi_parameter_substitutes_each_independently() {
    let src = "\
class Pair<A, B> {
  first: A;
  second: B;
  constructor(a: A, b: B) {
this.first = a;
this.second = b;
  }
}
const p = new Pair<number, string>(1, \"x\");
const a: number = p.first;   // ok
const b: string = p.second;  // ok
const bad: string = p.first; // TK2322
const e = new Pair<number, string>(\"x\", 1); // TK2345 x2
";
    // `p.first` is `number`, `p.second` is `string`: line 12 (string target) TK2322;
    // line 13 swaps both arguments → two TK2345 (each fails its own parameter).
    assert_eq!(
        diags(src),
        vec![
            (12, "TK2322".to_string()),
            (13, "TK2345".to_string()),
            (13, "TK2345".to_string())
        ]
    );
}

/// A multi-parameter class with **no** explicit type arguments infers each parameter
/// from its own constructor argument: `new Pair(1, "x")` infers `A = number`,
/// `B = string`, so `inferred.second` is `string` — a `string` target is clean, a
/// `number` target is `TK2322`.
#[test]
fn multi_parameter_inference_from_arguments() {
    let src = "\
class Pair<A, B> {
  first: A;
  second: B;
  constructor(a: A, b: B) {
this.first = a;
this.second = b;
  }
}
const inferred = new Pair(1, \"x\");
const c: string = inferred.second; // ok
const d: number = inferred.second; // TK2322
";
    // `B` inferred `string` from `\"x\"`: line 10 ok, line 11 (number target) TK2322.
    assert_eq!(diags(src), vec![(11, "TK2322".to_string())]);
}

/// `C<args>` is usable as a plain **type annotation** (not just at `new`): a
/// `Box<number>` parameter accepts a `Box<number>` argument and rejects a `Box<string>`
/// one. This exercises the M9 generic type-reference instantiation over a class's
/// instance template.
#[test]
fn generic_class_used_as_type_annotation() {
    let src = "\
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
}
function take(b: Box<number>): number {
  return b.value;
}
const ok = take(new Box<number>(1));   // ok
const bad = take(new Box<string>(\"s\")); // TK2345
";
    // The `Box<string>` argument is not assignable to the `Box<number>` parameter:
    // line 11 TK2345. The `Box<number>` argument (line 10) is clean.
    assert_eq!(diags(src), vec![(11, "TK2345".to_string())]);
}

/// A well-typed generic class with member bodies referencing the type parameter checks
/// **clean** — `T` resolves in every member body (constructor, getter, setter) under
/// the parameter frame pushed by `check_class`. No crash, no spurious error.
#[test]
fn generic_member_bodies_check_under_parameter_scope() {
    let src = "\
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
  get(): T {
return this.value;
  }
  set(v: T): void {
this.value = v;
  }
}
const b = new Box<number>(1);
const n: number = b.get();
";
    // A well-typed generic class checks clean — `T` resolves in every member body.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// A class type parameter must not leak: inside the class `T` shadows the
/// top-level alias, outside it resolves back to that alias. Pins both
/// `with_type_params` pops and the M9 shadowing order.
#[test]
fn type_parameter_does_not_leak_and_shadows_inside() {
    let src = "\
type T = string;
class Box<T> {
  value: T;
  constructor(v: T) {
this.value = v;
  }
}
const b = new Box<number>(1);
const inside: number = b.value; // ok — inside, `T` is the parameter (number here)
const s: T = 1;                 // TK2322 — outside, `T` is the top-level string alias
";
    // Inside the class `T` is the parameter (so `b.value` is `number`, line 9 ok);
    // outside, `T` resolves to the top-level `string` alias, so `s: T = 1` is line 10
    // TK2322. A leaked parameter would make line 10 clean instead.
    assert_eq!(diags(src), vec![(10, "TK2322".to_string())]);
}

#[test]
fn constructor_inference_uses_the_call_candidate_policy() {
    let src = "\
interface HasX { x: number; }
class Box<T extends HasX> {
  constructor(value: T) {}
}
class SameBox<T> {
  constructor(first: T, second: T) {}
}
class TupleBox<T> {
  constructor(value: [T, T]) {}
}
new Box(\"s\");                    // TK2345
new SameBox(1, \"s\");            // TK2345
new TupleBox([1, \"s\"]);         // TK2322
";
    assert_eq!(
        diags(src),
        vec![
            (11, "TK2345".to_string()),
            (12, "TK2345".to_string()),
            (13, "TK2322".to_string()),
        ]
    );
}

#[test]
fn unresolved_constructor_type_argument_records_inference_exhaustion() {
    let src = "\
class Unresolved<T> {}
const value = new Unresolved();
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(
        out.incomplete
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["expr-infer/new-expression/class-type-argument-inference"]
    );
}

#[test]
fn class_roots_inside_union_consumers_are_projected_before_use() {
    let src = "\
class A {
  kind: \"a\";
  value: number;
  onlyA: string;
}
class B {
  kind: \"b\";
  value: string;
  onlyB: boolean;
}
declare let union: A | B;
const readBad: boolean = union.value; // TK2322
const indexBad: boolean = union[\"value\"]; // TK2322
union.value = true; // TK2322
if (union.kind === \"a\") {
  const narrowedBad: number = union.onlyA; // TK2322
}
if (\"onlyA\" in union) {
  const inBad: number = union.onlyA; // TK2322
} else {
  const elseBad: string = union.onlyB; // TK2322
}
";
    assert_eq!(
        diags(src),
        vec![
            (12, "TK2322".to_string()),
            (13, "TK2322".to_string()),
            (14, "TK2322".to_string()),
            (16, "TK2322".to_string()),
            (19, "TK2322".to_string()),
            (21, "TK2322".to_string()),
        ]
    );
}

#[test]
fn class_context_shapes_fresh_literals_and_checks_excess_properties() {
    let src = "\
class Shape {
  x: number;
}
const ok: Shape = { x: 1 };
const wrong: Shape = { x: \"s\" }; // TK2322
const excess: Shape = { x: 1, y: 2 }; // TK2353
const wrongAndExcess: Shape = { x: \"s\", y: 2 }; // TK2322 + TK2353
";
    assert_eq!(
        diags(src),
        vec![
            (5, "TK2322".to_string()),
            (6, "TK2353".to_string()),
            (7, "TK2322".to_string()),
            (7, "TK2353".to_string()),
        ]
    );
}

#[test]
fn generic_heritage_composes_only_after_every_argument_is_available() {
    let src = "\
class Base<T> {
  value!: T;
}
class Derived extends Base<number> {}
const good: number = new Derived().value;
const bad: string = new Derived().value;
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn qualified_heritage_visits_every_argument_without_a_partial_application() {
    let src = "\
namespace N {}
class Base<First, Second> {
  inherited!: First | Second;
}
class Derived extends Base<N.First, N.Second> {}
new Derived().inherited;
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert_eq!(
        diags(src),
        vec![(5, "TK2694".to_string()), (5, "TK2694".to_string())]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}
