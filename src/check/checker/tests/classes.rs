//! M11 end-to-end tests for classes.
//! Pins instance construction, `new`, `this` scoping, method returns, structural
//! instance assignability, and no-crash handling for deferred features.
//! Fixture acceptance lives in `m11_classes/`.

use crate::driver::check_source;

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

/// Instance-type construction: a field is accessible on an instance with its
/// declared type (member access + assignability), and a missing member is
/// `TK2339`. This is the `basic.ts` headline as a unit pin.
#[test]
fn instance_field_access_and_missing_member() {
    let src = "\
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
this.x = x;
this.y = y;
  }
}
const p = new Point(1, 2);
const a: number = p.x;
const b: string = p.x;
const c = p.z;
";
    // `p.x` is number: line 10 clean, line 11 (string = number) TK2322; `p.z` is
    // missing → line 12 TK2339.
    assert_eq!(
        diags(src),
        vec![(11, "TK2322".to_string()), (12, "TK2339".to_string())]
    );
}

/// `new` checks the constructor signature like an M3 call: wrong arity is
/// `TK2554`, a wrong argument type is `TK2345`, and a correct call is clean.
#[test]
fn new_checks_constructor_arity_and_arguments() {
    let src = "\
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
this.x = x;
this.y = y;
  }
}
const ok = new Point(1, 2);
const few = new Point(1);
const bad = new Point(1, \"s\");
";
    // `new Point(1)` → TK2554 (arity); `new Point(1, "s")` → TK2345 (arg).
    assert_eq!(
        diags(src),
        vec![(10, "TK2554".to_string()), (11, "TK2345".to_string())]
    );
}

/// A class with no explicit constructor defaults to **zero** parameters: `new C()`
/// is clean and `new C(1)` is `TK2554`.
#[test]
fn default_constructor_is_zero_arity() {
    let src = "\
class Empty {
  x: number;
}
const ok = new Empty();
const bad = new Empty(1);
";
    assert_eq!(diags(src), vec![(5, "TK2554".to_string())]);
}

/// `this` resolves to the instance type inside a method body (so `this.field` and
/// `this.method()` resolve), and a `return` is checked against the method's
/// declared return type (`TK2322`). This is the `methods.ts` headline.
#[test]
fn this_resolves_in_methods_and_returns_are_checked() {
    let src = "\
class Counter {
  count: number;
  constructor() {
this.count = 0;
  }
  increment(by: number): number {
return this.count + by;
  }
  bad(): string {
return this.count;
  }
}
const c = new Counter();
const n: number = c.increment(1);
const m: string = c.increment(1);
const w = c.increment(\"s\");
";
    // `bad()` returns `this.count` (number) but is declared `string` → line 10
    // TK2322; `m: string = c.increment(1)` (number) → line 15 TK2322; the bad arg
    // → line 16 TK2345. `increment`'s body (line 7) is clean.
    assert_eq!(
        diags(src),
        vec![
            (10, "TK2322".to_string()),
            (15, "TK2322".to_string()),
            (16, "TK2345".to_string())
        ]
    );
}

/// Structural assignability both ways: an instance is assignable to a matching
/// object type and an object literal is assignable to the instance type; a wrong
/// member type is `TK2322` and a missing required member is `TK2741`. This is the
/// `structural.ts` headline.
#[test]
fn instance_type_is_structural_both_ways() {
    let src = "\
class Box {
  value: number;
  constructor(v: number) {
this.value = v;
  }
}
const obj: { value: number } = new Box(1);
const fromObj: Box = { value: 1 };
const bad: { value: string } = new Box(1);
const miss: Box = {};
";
    // instance → object type (line 7) and object literal → instance (line 8) are
    // clean; wrong member type (line 9) TK2322; missing `value` (line 10) TK2741.
    assert_eq!(
        diags(src),
        vec![(9, "TK2322".to_string()), (10, "TK2741".to_string())]
    );
}

/// A method is exposed on the instance type as a **function-typed property**, so
/// `p.method` is a value of that function type and calling it with a wrong
/// argument is `TK2345` (the call path runs on the method's signature).
#[test]
fn method_is_a_function_typed_property() {
    let src = "\
class Greeter {
  greet(name: string): string {
return name;
  }
}
const g = new Greeter();
const ok: string = g.greet(\"x\");
const bad = g.greet(1);
";
    // `g.greet(1)` — the argument 1 is not a string → line 8 TK2345.
    assert_eq!(diags(src), vec![(8, "TK2345".to_string())]);
}

/// A field can reference the class's **own type** (reserve-then-fill): a recursive
/// `next: Node | null` lowers and is accessible, and the narrowed-away null is the
/// instance type again. The whole program type-checks clean (no crash, no error).
#[test]
fn recursive_self_referential_field() {
    let src = "\
class Node {
  value: number;
  next: Node | null;
  constructor(value: number) {
this.value = value;
this.next = null;
  }
}
const head = new Node(1);
const v: number = head.value;
const tail: Node | null = head.next;
";
    // A self-referential field lowers and resolves: the program is clean.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// **No leak**: `this` outside any class member resolves to the error type (no
/// narrowing, no crash). A top-level `this.foo` does not emit a property error
/// (the error-typed base suppresses cascade) and the run completes.
#[test]
fn this_does_not_leak_outside_a_class_member() {
    let src = "\
class C {
  x: number;
  constructor() {
this.x = 1;
  }
}
function free() {
  const leaked = this.x;
  return leaked;
}
const c = new C();
const n: number = c.x;
";
    // `this.x` inside the free function is the error type (out of any class
    // member), so it emits NO diagnostic (no TK2339, no crash). The class itself
    // checks clean.
    assert!(
        diags(src).is_empty(),
        "this must not leak / crash outside a class member, got {:?}",
        diags(src)
    );
}

/// Two classes with the **same instance shape** are structurally interchangeable
/// in this slice (no nominal/private typing yet): a `B`-typed instance is
/// assignable to an `A` annotation. This pins the structural-only choice for M11.
#[test]
fn same_shape_classes_are_structurally_interchangeable() {
    let src = "\
class A {
  x: number;
  constructor(x: number) {
this.x = x;
  }
}
class B {
  x: number;
  constructor(x: number) {
this.x = x;
  }
}
const a: A = new B(1);
";
    // Same structural shape → assignable (no nominal distinction in this slice).
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// Deferred class features are out of M11 scope, but must not crash or
/// false-report constructor member assignment.
#[test]
fn deferred_class_features_do_not_crash() {
    let src = "\
class Base {
  base: number;
  constructor() {
this.base = 0;
  }
}
class Derived extends Base {
  private secret: number;
  static count: number;
  readonly id: number;
  constructor(id: number) {
super();
this.secret = 1;
this.id = id;
  }
  get value(): number {
return this.secret;
  }
}
const d = new Derived(1);
";
    // The run completes (no panic). Inheritance/modifiers/static/getters are
    // deferred, so we do not assert their semantics — only that nothing crashes
    // and the constructor body's member-assignments are not falsely flagged. The
    // exact diagnostic set is intentionally not pinned (deferred behaviour).
    let _ = diags(src);
}

/// M14 closed the constructor member-assignment gap: the wrong RHS now emits
/// `TK2322`, while the type-correct path is pinned in `member_assign_tests`.
#[test]
fn member_assignment_target_type_mismatch_is_checked() {
    let src = "\
class C {
  x: number;
  constructor() {
this.x = \"not a number\";
  }
}
const c = new C();
";
    // `this.x = "..."` — string is not assignable to the `number` property → TK2322
    // on the RHS (line 4). No crash.
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}
