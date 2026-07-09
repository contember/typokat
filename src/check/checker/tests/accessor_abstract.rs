//! M15 end-to-end tests for **getters/setters** and **`abstract` classes**. These
//! drive the whole pipeline (parse → bind → check) and assert the *set* of
//! `(line, code)` diagnostics, pinning the invariants the reviewer should scrutinize:
//!
//!  - a `get`/`set` accessor is **one** property of the getter's return type, and it
//!    is **writable** (assigning a wrong type is `TK2322`, a right one is clean);
//!  - a **get-only** accessor is `readonly` → assigning it is `TK2540`, while reading
//!    it yields the getter's type;
//!  - `new` on an **`abstract`** class is `TK2511`, but a **concrete subclass** of an
//!    abstract class instantiates fine (only the named class's flag matters);
//!  - an **abstract method** (no body) is part of the instance type and is satisfied
//!    by a concrete subclass's implementation (its return type still checked).
//!
//! The per-fixture acceptance lives in `m15_accessors/`.

use crate::driver::check_source;

/// Run the checker and return the sorted `(1-based line, code)` of every diagnostic,
/// keyed on its primary-span start line.
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

/// A `get`+`set` accessor is a single property of the **getter's return type**, and it
/// is **writable**: a read is the getter's type (a wrong-typed `const` is `TK2322`), a
/// type-correct assignment is clean, and a wrong-typed assignment is `TK2322`.
#[test]
fn get_set_accessor_is_writable_property_of_getter_type() {
    let src = "\
class Temperature {
  private _c: number;
  constructor(c: number) {
this._c = c;
  }
  get celsius(): number {
return this._c;
  }
  set celsius(v: number) {
this._c = v;
  }
}
const t = new Temperature(20);
const c: number = t.celsius;
const bad: string = t.celsius;
t.celsius = 25;
t.celsius = \"hot\";
";
    assert_eq!(
        diags(src),
        vec![(15, "TK2322".to_string()), (17, "TK2322".to_string())]
    );
}

/// A **get-only** accessor (no setter) behaves like a `readonly` property: it reads as
/// the getter's type, but assigning it is `TK2540`.
#[test]
fn get_only_accessor_is_readonly() {
    let src = "\
class Temperature {
  private _c: number;
  constructor(c: number) {
this._c = c;
  }
  get fahrenheit(): number {
return this._c;
  }
}
const t = new Temperature(20);
const f: number = t.fahrenheit;
t.fahrenheit = 100;
";
    assert_eq!(diags(src), vec![(12, "TK2540".to_string())]);
}

/// A get-only **accessor** is read-only **everywhere, including its declaring class's
/// constructor** (`TK2540`) — the M14 constructor carve-out is for `readonly` *fields*
/// only. The sibling `readonly` data field written in the same constructor stays clean,
/// pinning that the field behaviour is unchanged (no M14 regression).
#[test]
fn get_only_accessor_in_constructor_is_tk2540_field_stays_clean() {
    let src = "\
class C {
  private _n: number;
  readonly id: number;
  constructor(n: number) {
this._n = n;
this.id = n;
this.value = n;
  }
  get value(): number {
return this._n;
  }
}
";
    assert_eq!(diags(src), vec![(7, "TK2540".to_string())]);
}

/// `new` on an `abstract` class is `TK2511`; a **concrete subclass** of it instantiates
/// fine, and the abstract method it implements is part of the instance type (read OK,
/// wrong-typed read `TK2322`).
#[test]
fn abstract_new_errors_concrete_subclass_ok() {
    let src = "\
abstract class Shape {
  abstract area(): number;
  name: string;
  constructor(name: string) {
this.name = name;
  }
}
const s = new Shape(\"x\");
class Circle extends Shape {
  radius: number;
  constructor(r: number) {
super(\"circle\");
this.radius = r;
  }
  area(): number {
return this.radius;
  }
}
const c = new Circle(5);
const a: number = c.area();
const n: string = c.name;
const w: string = c.area();
";
    assert_eq!(
        diags(src),
        vec![(8, "TK2511".to_string()), (22, "TK2322".to_string())]
    );
}

/// Only the **directly-named** class's `abstract` flag gates `new`: a concrete class
/// extending an abstract base is instantiable, and a *further* abstract subclass of a
/// concrete class is itself not instantiable. Confirms the flag is per-declaration, not
/// inherited.
#[test]
fn abstractness_is_per_named_class_not_inherited() {
    let src = "\
abstract class A {
  constructor() {}
}
class B extends A {
  constructor() {
super();
  }
}
abstract class C extends B {
  constructor() {
super();
  }
}
const a = new A();
const b = new B();
const c = new C();
";
    assert_eq!(
        diags(src),
        vec![(14, "TK2511".to_string()), (16, "TK2511".to_string())]
    );
}

/// A getter body is checked like a method: a `return` whose type is not assignable to
/// the getter's declared return type is `TK2322` (primary span = the returned
/// expression). A type-correct setter body is clean.
#[test]
fn accessor_bodies_are_checked() {
    let src = "\
class C {
  private _n: number;
  constructor(n: number) {
this._n = n;
  }
  get bad(): string {
return this._n;
  }
  set ok(v: number) {
this._n = v;
  }
}
";
    assert_eq!(diags(src), vec![(7, "TK2322".to_string())]);
}
