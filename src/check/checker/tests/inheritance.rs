//! M12 end-to-end tests for class inheritance (`extends`, `super`). These drive
//! the whole pipeline (parse → bind → check) and assert the *set* of `(line,
//! code)` diagnostics, pinning the invariants the reviewer should scrutinize:
//! the composed instance type (inherited + own members, own overriding base),
//! subclass→base assignability (and the base→subclass failure direction),
//! `super(...)` checked against the base constructor (arity + args), an inherited
//! constructor when the derived class declares none, and an `extends` **cycle**
//! terminating without a panic.
//!
//! The per-fixture acceptance lives in the conformance corpus
//! (`m12_inheritance/`); these unit pins guard the construction/scoping invariants
//! directly.

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

/// The **composed instance type** carries both inherited and own members: an
/// inherited field, an inherited method, and an own field all resolve on a derived
/// instance, while an unknown property is `TK2339`.
#[test]
fn composed_instance_type_has_inherited_and_own_members() {
    let src = "\
class Animal {
  name: string;
  constructor(name: string) {
this.name = name;
  }
  speak(): string {
return this.name;
  }
}
class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
super(name);
this.breed = breed;
  }
}
const d = new Dog(\"Rex\", \"Lab\");
const n: string = d.name;
const br: string = d.breed;
const s: string = d.speak();
const z = d.missing;
";
    // Only the unknown property `d.missing` (line 21) errors — inherited field,
    // own field, and inherited method all resolve.
    assert_eq!(diags(src), vec![(21, "TK2339".to_string())]);
}

/// Subclass→base assignability falls out of structural width: `Dog` (a width-
/// superset) is assignable to `Animal`, but `Animal` is **not** assignable to
/// `Dog` (the required `breed` is missing → `TK2741`). Pins both directions, so a
/// false negative in the unsound direction would fail.
#[test]
fn subclass_to_base_assignable_base_to_subclass_not() {
    let src = "\
class Animal {
  name: string;
  constructor(name: string) {
this.name = name;
  }
}
class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
super(name);
this.breed = breed;
  }
}
const a: Animal = new Dog(\"Rex\", \"Lab\");
const bad: Dog = new Animal(\"Rex\");
";
    // `Dog`→`Animal` (line 14) is clean; `Animal`→`Dog` (line 15) is missing
    // `breed` → TK2741.
    assert_eq!(diags(src), vec![(15, "TK2741".to_string())]);
}

/// `super(args)` is checked against the **base constructor** signature: a correct
/// call is clean, a wrong arity is `TK2554`, and a wrong argument type is `TK2345`.
#[test]
fn super_call_checked_against_base_constructor() {
    let src = "\
class Base {
  id: number;
  constructor(id: number) {
this.id = id;
  }
}
class Ok extends Base {
  constructor() {
super(1);
  }
}
class BadArity extends Base {
  constructor() {
super();
  }
}
class BadArg extends Base {
  constructor() {
super(\"s\");
  }
}
";
    // `super(1)` clean; `super()` (line 14) wrong arity TK2554; `super("s")`
    // (line 19) wrong argument type TK2345.
    assert_eq!(
        diags(src),
        vec![(14, "TK2554".to_string()), (19, "TK2345".to_string())]
    );
}

/// A derived class with **no own constructor** inherits the base's signature, so
/// `new Derived(...)` is checked against the base constructor: a correct call is
/// clean, a missing argument is `TK2554`, and a wrong argument type is `TK2345`.
#[test]
fn inherited_constructor_drives_new() {
    let src = "\
class Base {
  id: number;
  constructor(id: number) {
this.id = id;
  }
}
class Plain extends Base {}
const p = new Plain(5);
const q = new Plain();
const r = new Plain(\"s\");
";
    // `new Plain(5)` clean; `new Plain()` (line 9) wrong arity TK2554;
    // `new Plain("s")` (line 10) wrong argument type TK2345.
    assert_eq!(
        diags(src),
        vec![(9, "TK2554".to_string()), (10, "TK2345".to_string())]
    );
}

/// An own member **overrides** an inherited one of the same name: the derived
/// type wins. Here `Derived.x: string` overrides `Base.x: number`, so a
/// `string`-annotated read is clean and a `number`-annotated read errors —
/// proving the override replaced the inherited member. The override is also
/// type-incompatible (`string` ≇ `number`), so backlog 06 reports `TK2416` on the
/// derived member's name (matching tsc `TS2416`).
#[test]
fn own_member_overrides_inherited() {
    let src = "\
class Base {
  x: number;
  constructor() {
this.x = 0;
  }
}
class Derived extends Base {
  x: string;
  constructor() {
super();
this.x = \"s\";
  }
}
const d = new Derived();
const ok: string = d.x;
const bad: number = d.x;
";
    // The incompatible override is `TK2416` on the derived `x` (line 8). The
    // override makes `d.x` a `string`: `string` read (line 15) clean; `number` read
    // (line 16) TK2322.
    assert_eq!(
        diags(src),
        vec![(8, "TK2416".to_string()), (16, "TK2322".to_string())]
    );
}

/// An `extends` **cycle** (`A extends B`, `B extends A`) must terminate without a
/// panic or infinite loop. Correctness of a cyclic hierarchy is undefined (it is
/// not valid TS); we only assert the run completes (the cycle guard breaks it).
#[test]
fn extends_cycle_terminates_without_panic() {
    let src = "\
class A extends B {
  a: number;
}
class B extends A {
  b: number;
}
const x = new A();
const y = new B();
";
    // The run completes (no panic / no hang). The exact diagnostic set for a
    // cyclic hierarchy is intentionally not pinned (undefined / invalid TS).
    let _ = diags(src);
}

/// A two-level chain composes transitively: a grandchild instance carries
/// members from **both** ancestors plus its own, and all resolve.
#[test]
fn multi_level_inheritance_composes_transitively() {
    let src = "\
class A {
  a: number;
  constructor() {
this.a = 1;
  }
}
class B extends A {
  b: number;
  constructor() {
super();
this.b = 2;
  }
}
class C extends B {
  c: number;
  constructor() {
super();
this.c = 3;
  }
}
const obj = new C();
const x: number = obj.a;
const y: number = obj.b;
const z: number = obj.c;
const w: A = new C();
";
    // All ancestor + own members resolve and `C`→`A` is assignable: clean.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// **No leak**: `super(...)` outside a derived class member (here in a free
/// function, where no base constructor is in scope) collects no obligation and
/// emits no `super`-specific diagnostic, and does not crash.
#[test]
fn super_call_outside_derived_member_does_not_crash() {
    let src = "\
function free() {
  super(1);
  return 0;
}
";
    // No base ctor in scope → no diagnostic, no crash (super arg `1` is still
    // walked).
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}
