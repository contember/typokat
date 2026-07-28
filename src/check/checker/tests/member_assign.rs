//! M14 end-to-end tests for member-assignment targets and `readonly` properties.
//! Pins RHS checking, M11-M13 assignment regressions, and the constructor-only
//! readonly write carve-out. Fixture acceptance lives in `m14_readonly/`; relation
//! pins cover that `readonly` does not affect assignability.

use crate::check::test_support::check_source;

/// Run the checker and return the sorted `(1-based line, code)` of every
/// diagnostic, keyed on its primary-span start line.
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

/// A member-assignment RHS is checked against the property type: a type-correct
/// assignment (instance and object-literal base) is clean; a mismatched one is
/// `TK2322` on the RHS.
#[test]
fn member_assignment_checks_rhs_against_property_type() {
    let src = "\
class Box {
  v: number;
  constructor(v: number) {
this.v = v;
  }
}
const b = new Box(1);
b.v = 2;
b.v = \"s\";
const o = { a: 1 };
o.a = 3;
o.a = \"x\";
";
    // `this.v = v` (4) ok; `b.v = 2` (8) ok; `b.v = "s"` (9) TK2322; `o.a = 3` (11)
    // ok; `o.a = "x"` (12) TK2322.
    assert_eq!(
        diags(src),
        vec![(9, "TK2322".to_string()), (12, "TK2322".to_string())]
    );
}

/// No regression: the M11–M13 constructor/method assignments (`this.x = x`, etc.)
/// stay clean now that member targets are checked — including an **incomplete**
/// (arithmetic) RHS (`this.count + by`) that infers to the error type, which must not
/// false-fire. This is the central anti-regression pin.
#[test]
fn existing_constructor_and_method_assignments_stay_clean() {
    let src = "\
class Counter {
  count: number;
  constructor(start: number) {
this.count = start;
this.count = 0;
  }
  add(by: number): void {
this.count = this.count + by;
  }
}
const c = new Counter(1);
";
    // Every member-assignment here is type-correct (or has an error-typed RHS that
    // is skipped); the program is clean.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// `readonly`: assignable via `this` inside the declaring class's constructor (ok),
/// but `TK2540` in another method and from external code; a mutable sibling is clean
/// everywhere.
#[test]
fn readonly_constructor_ok_method_and_external_error() {
    let src = "\
class Point {
  readonly x: number;
  y: number;
  constructor(x: number, y: number) {
this.x = x;
this.y = y;
  }
  reset(): void {
this.x = 0;
this.y = 0;
  }
}
const p = new Point(1, 2);
p.y = 5;
p.x = 5;
";
    // Constructor `this.x = x` (5) ok; method `this.x = 0` (9) TK2540; mutable
    // `this.y`/`p.y` clean; external `p.x = 5` (15) TK2540.
    assert_eq!(
        diags(src),
        vec![(9, "TK2540".to_string()), (15, "TK2540".to_string())]
    );
}

/// `readonly` allowance is scoped to the **declaring** class's constructor and to a
/// `this.prop` target: assigning a base class's `readonly` member from a subclass
/// constructor (via `this`) is `TK2540` (it is not the declaring class), and so is a
/// constructor assignment via a **non-`this`** base of the same type.
#[test]
fn readonly_only_in_declaring_class_constructor_via_this() {
    let src = "\
class Base {
  readonly id: number;
  constructor(id: number) {
this.id = id;
  }
}
class Derived extends Base {
  constructor() {
super(1);
this.id = 2;
  }
}
class Other {
  readonly k: number;
  constructor(other: Other) {
other.k = 1;
  }
}
";
    // `this.id = id` in `Base`'s ctor (4) ok; reassigning the inherited `readonly`
    // `id` in `Derived`'s ctor (10) TK2540 (Derived is not the declaring class); a
    // non-`this` base in `Other`'s ctor (16) TK2540 (not a `this.prop` target).
    assert_eq!(
        diags(src),
        vec![(10, "TK2540".to_string()), (16, "TK2540".to_string())]
    );
}

/// `readonly` does **not** change assignability: a `{ readonly x }`-bearing instance
/// and a plain `{ x }` object relate both ways (no `TK2322`/`TK2741` from the flag
/// alone). Drives it end-to-end (the structural-level pin lives in the relation
/// tests). A type-correct `readonly` field assigned in the constructor stays clean.
#[test]
fn readonly_does_not_affect_assignability() {
    let src = "\
class R {
  readonly x: number;
  constructor(x: number) {
this.x = x;
  }
}
const r = new R(1);
const o: { x: number } = r;
const back: R = { x: 2 };
";
    // instance → `{ x: number }` (8) ok; object literal → `R` (9) ok — `readonly`
    // does not gate either direction. The whole program is clean.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// Compound assignment to a member target stays **deferred**: `this.x += 1` collects
/// no obligation and triggers no `readonly` check (matching the identifier-target
/// baseline). The RHS is still walked (an unresolved name in it would still error).
#[test]
fn compound_member_assignment_is_deferred() {
    let src = "\
class C {
  readonly x: number;
  y: number;
  constructor(x: number) {
this.x = x;
  }
  bump(): void {
this.x += 1;
this.y += 1;
  }
}
const c = new C(1);
";
    // `this.x += 1` (8) — compound on a `readonly` target is deferred (no TK2540, no
    // TK2322); `this.y += 1` (9) likewise. The program is clean.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// Access control applies before the normal/compound assignment split. Same-class
/// private writes and subclass protected writes remain valid.
#[test]
fn member_assignment_enforces_private_and_protected_access() {
    let src = "\
class Base {
  private secret: number;
  protected shared: number;
  update(other: Base) {
this.secret = 1;
other.secret += 1;
this.shared = 1;
other.shared += 1;
  }
}
class Derived extends Base {
  update(other: Derived) {
this.shared = 1;
other.shared += 1;
this.secret = 1;
this.secret += 1;
  }
}
const base = new Base();
base.secret = 1;
base.secret += 1;
base.shared = 1;
base.shared += 1;
";
    assert_eq!(
        diags(src),
        vec![
            (15, "TK2341".to_string()),
            (16, "TK2341".to_string()),
            (20, "TK2341".to_string()),
            (21, "TK2341".to_string()),
            (22, "TK2445".to_string()),
            (23, "TK2445".to_string()),
        ]
    );
}
