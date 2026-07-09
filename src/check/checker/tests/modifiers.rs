//! M13 end-to-end tests for access modifiers and `static` members.
//! Pins current-class access control, nominal private/protected relation, and
//! static/instance partitioning. Fixture acceptance lives in `m13_modifiers/`;
//! relation-level nominal pins live in `relate::relation::tests`.

use crate::driver::check_source;

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

/// Access control: a `private` member is reachable inside its declaring class but
/// `TK2341` outside it (and even inside a *subclass*); a `protected` member is
/// reachable inside the class and its subclasses but `TK2445` outside.
#[test]
fn private_and_protected_access_control() {
    let src = "\
class Account {
  private balance: number;
  protected owner: string;
  constructor(b: number, o: string) {
this.balance = b;
this.owner = o;
  }
  check(): number {
return this.balance;
  }
}
const acc = new Account(100, \"a\");
const b = acc.balance;
const o = acc.owner;
class Sub extends Account {
  constructor() {
super(1, \"a\");
  }
  readOwner(): string {
return this.owner;
  }
  readBalance(): number {
return this.balance;
  }
}
";
    assert_eq!(
        diags(src),
        vec![
            (13, "TK2341".to_string()),
            (14, "TK2445".to_string()),
            (23, "TK2341".to_string())
        ]
    );
}

/// Same-class access to **another instance's** `private` member is allowed (the
/// rule keys on the declaring class, not the instance).
#[test]
fn same_class_private_access_to_other_instance_is_allowed() {
    let src = "\
class Box {
  private v: number;
  constructor(v: number) {
this.v = v;
  }
  eq(other: Box): boolean {
return this.v === other.v;
  }
}
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// Nominal typing: a structurally-identical object literal is NOT assignable to a
/// class with a `private` member, a different class with a same-named private
/// member is NOT assignable, but the class's own instance IS.
#[test]
fn private_member_makes_class_nominal() {
    let src = "\
class Secret {
  private x: number;
  constructor() {
this.x = 1;
  }
}
const a: Secret = new Secret();
const b: Secret = { x: 1 };
class Other {
  private x: number;
  constructor() {
this.x = 1;
  }
}
const c: Secret = new Other();
";
    assert_eq!(
        diags(src),
        vec![(8, "TK2322".to_string()), (15, "TK2322".to_string())]
    );
}

/// A `protected` member is nominal too: a structurally-identical public object is
/// not assignable, but the class's own instance is.
#[test]
fn protected_member_makes_class_nominal() {
    let src = "\
class Secret {
  protected x: number;
  constructor() {
this.x = 1;
  }
}
const a: Secret = new Secret();
const b: Secret = { x: 1 };
";
    assert_eq!(diags(src), vec![(8, "TK2322".to_string())]);
}

/// Static/instance partition: a static member lives on the class value (the
/// static side) and an instance member on instances; cross-access is `TK2339` in
/// both directions, and a static field's type is checked on the class value.
#[test]
fn static_and_instance_member_partition() {
    let src = "\
class Counter {
  static total: number;
  static reset(): void {}
  value: number;
  constructor() {
this.value = 0;
  }
}
const t: number = Counter.total;
Counter.reset();
const bad: string = Counter.total;
const c = new Counter();
const v: number = c.value;
const x = c.total;
const z = Counter.value;
";
    assert_eq!(
        diags(src),
        vec![
            (11, "TK2322".to_string()),
            (14, "TK2339".to_string()),
            (15, "TK2339".to_string())
        ]
    );
}

/// A `private`/`protected` member accessed where there is **no class context**
/// (e.g. a free function) is still rejected — `current_class == None` matches no
/// origin (no false negative).
#[test]
fn non_public_access_outside_any_class_is_rejected() {
    let src = "\
class Account {
  private balance: number;
  protected owner: string;
  constructor(b: number, o: string) {
this.balance = b;
this.owner = o;
  }
}
function leak(a: Account) {
  const p = a.balance;
  const q = a.owner;
  return 0;
}
";
    assert_eq!(
        diags(src),
        vec![(10, "TK2341".to_string()), (11, "TK2445".to_string())]
    );
}

/// Static member bodies are checked; this guards the old false negative where
/// they were skipped.
#[test]
fn static_member_bodies_are_checked() {
    let src = "\
function helper(x: number): void {}
class C {
  static label: number = missingName;
  value: number;
  constructor() {
this.value = 0;
  }
  inst(): number {
return 1;
  }
  static good(): number {
return 1;
  }
  static badReturn(): number {
return \"s\";
  }
  static useMissing(): void {
const z = nope;
  }
  static callBad(): void {
helper(\"s\");
  }
}
";
    assert_eq!(
        diags(src),
        vec![
            (3, "TK2304".to_string()),
            (15, "TK2322".to_string()),
            (18, "TK2304".to_string()),
            (21, "TK2345".to_string()),
        ]
    );
}

/// A static body's `this` is the **static side** and does not leak into a
/// following instance member: a static method may reach a static field via
/// `this`, and the next instance method still resolves an instance field via
/// `this` (no cross-contamination). Pins the per-member `this` save/restore.
#[test]
fn static_this_is_static_side_and_does_not_leak() {
    let src = "\
class C {
  static total: number = 0;
  value: number;
  constructor() {
this.value = 0;
  }
  static readTotal(): number {
return this.total;
  }
  readValue(): number {
return this.value;
  }
}
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// Remaining deferred modifiers do not crash and produce no new diagnostics here:
/// `public` (the default), getters, and a parameter property must complete the run.
/// `readonly` is handled in M14, but a `readonly` field assigned via `this` inside
/// the **constructor** (as here, `this.b = b`) is allowed, so this case stays clean.
#[test]
fn deferred_modifiers_do_not_crash() {
    let src = "\
class C {
  public a: number;
  readonly b: number;
  private c: number;
  constructor(a: number, b: number, c: number) {
this.a = a;
this.b = b;
this.c = c;
  }
  get d(): number {
return this.a;
  }
}
const x = new C(1, 2, 3);
const y: number = x.a;
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}
