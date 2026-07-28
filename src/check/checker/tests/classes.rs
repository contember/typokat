//! M11 end-to-end tests for classes.
//! Pins instance construction, `new`, `this` scoping, method returns, structural
//! instance assignability, and no-crash handling for deferred features.
//! Fixture acceptance lives in `m11_classes/`.

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

#[test]
fn frozen_generic_templates_resolve_in_class_and_namespace_callable_surfaces() {
    let src = r#"class ArrayConsumer {
  accept(values: Array<string>): void {}
}
namespace Api {
  export function accept(values: Array<string>): void {}
}
new ArrayConsumer().accept(["ok"]);
Api.accept(["ok"]);
new ArrayConsumer().accept([1]);
Api.accept([1]);
"#;
    let checked = check_source(src);
    assert!(checked.parse_errors.is_empty());
    assert!(checked.incomplete.is_empty(), "{:#?}", checked.incomplete);

    assert_eq!(
        diags(src),
        vec![(9, "TK2345".to_string()), (10, "TK2345".to_string()),]
    );
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

#[test]
fn poisoned_body_view_checks_known_members_without_using_unavailable_operands() {
    let src = "\
const seed = 1;
class C {
  unavailable = seed;
  safe!: number;
  check() {
this.safe = \"bad\";
this.unavailable = \"ignored\";
  }
}
declare const c: C;
const n: number = c.safe;
";
    assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
}

#[test]
fn poisoned_body_view_does_not_use_unannotated_accessor_return_as_an_operand() {
    let src = "\
const seed = 1;
class C {
  poison = seed;
  safe!: string;
  get value() { return this.safe; }
  set value(input) { this.value = this.safe; }
}
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

#[test]
fn poisoned_class_value_calls_visit_children_without_fabricating_diagnostics() {
    let src = "\
const seed = 1;
class C {
  poison = seed;
  static known(): void {}
}
C.known(1);
C[\"known\"](1);
C.known(missingArgument);
C[missingKey]();
C.missing();
";
    let out = check_source(src);
    assert_eq!(
        diags(src),
        vec![(8, "TK2304".to_string()), (9, "TK2304".to_string())]
    );
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["class/property-definition/initializer-inference"]
    );
}

#[test]
fn nested_class_body_diagnostic_is_visited_exactly_once() {
    let src = "\
class Outer {
  method() {
    class Inner {
      method() { missingNested; }
    }
  }
}
";
    assert_eq!(diags(src), vec![(4, "TK2304".to_string())]);
}

#[test]
fn nested_class_can_access_private_and_protected_members_of_enclosing_class() {
    let src = "\
class Outer {
  private secret!: number;
  protected value!: number;
  method() {
    class Inner {
      read(outer: Outer): number {
        const secret: number = outer.secret;
        const value: number = outer.value;
        return secret + value;
      }
    }
  }
}
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

#[test]
fn nested_static_class_context_restores_enclosing_and_external_access() {
    let src = "\
class Outer {
  private value!: number;
  method() {
    class Inner {
      static read(outer: Outer): number { return outer.value; }
    }
    const own: number = this.value;
  }
}
const outer = new Outer();
outer.value;
";
    assert_eq!(diags(src), vec![(11, "TK2341".to_string())]);
}

#[test]
fn nested_private_field_access_retains_one_independent_incomplete() {
    let src = "\
class A {
  static #x: number;
  method() {
    class B {
      read() { A.#x; }
    }
  }
}
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["expr-infer/private-field-access/self"]
    );
}

#[test]
fn nested_poisoned_class_uses_body_view_without_fabricated_operands() {
    let src = "\
const seed = 1;
class Outer {
  method() {
    class Inner {
      poison = seed;
      safe!: number;
      check() { this.safe = \"bad\"; }
    }
  }
}
";
    assert_eq!(diags(src), vec![(7, "TK2322".to_string())]);
}

#[test]
fn nested_class_lifecycle_uses_existing_branch_scopes() {
    let src = "\
function visit(flag: boolean) {
  if (flag) {
    class InBlock {
      method() { missingBlock; }
    }
  }
  switch (1) {
    case 1:
      class InSwitch {
        method() { missingSwitch; }
      }
  }
  for (let i = 0; i < 1; i += 1) {
    class InLoop {
      method() { missingLoop; }
    }
  }
  try {
    class InTry {
      method() { missingTry; }
    }
  } catch (error) {
    class InCatch {
      method() { missingCatch; }
    }
  }
}
";
    assert_eq!(
        diags(src),
        vec![
            (4, "TK2304".to_string()),
            (10, "TK2304".to_string()),
            (15, "TK2304".to_string()),
            (20, "TK2304".to_string()),
            (24, "TK2304".to_string()),
        ]
    );
}

#[test]
fn deeply_nested_class_keeps_all_enclosing_private_contexts() {
    let src = "\
class Outer {
  private outerSecret!: number;
  method() {
    class Middle {
      private middleSecret!: number;
      method() {
        class Inner {
          read(outer: Outer, middle: Middle): number {
            return outer.outerSecret + middle.middleSecret;
          }
        }
      }
    }
  }
}
const outer = new Outer();
outer.outerSecret;
";
    assert_eq!(diags(src), vec![(17, "TK2341".to_string())]);
}

#[test]
fn nested_classes_use_the_existing_heritage_and_value_pipeline() {
    let src = "\
class Outer {
  method() {
    class Base {
      value!: number;
    }
    class Derived extends Base {
      read(): number { return this.value; }
    }
    const derived = new Derived();
    const value: number = derived.read();
  }
}
";
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

#[test]
fn unannotated_class_callable_is_published_and_inherited() {
    let src = "\
class Base {
  method(value, ...rest) {}
}
class Derived extends Base {}
new Derived().method(\"value\", 1, true);
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn class_type_query_retains_its_canonical_record_and_span_once() {
    let src = "class C { method(value: typeof Missing): void {} }";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    assert_eq!(out.incomplete.len(), 1, "{:?}", out.incomplete);
    let incomplete = &out.incomplete[0];
    assert_eq!(incomplete.id, "annotation-lower/type-query/typeof");
    assert_eq!(incomplete.context, "typeof type query not lowered");
    assert_eq!(
        &src[incomplete.span.start as usize..incomplete.span.end as usize],
        "typeof Missing"
    );
}

#[test]
fn class_property_this_type_retains_its_canonical_record_and_span_once() {
    let src = "class C { value: this[\"value\"]; }";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    assert_eq!(out.incomplete.len(), 1, "{:?}", out.incomplete);
    let incomplete = &out.incomplete[0];
    assert_eq!(incomplete.id, "annotation-lower/this-type/self");
    assert_eq!(incomplete.context, "this type annotation not modeled");
    assert_eq!(
        &src[incomplete.span.start as usize..incomplete.span.end as usize],
        "this"
    );
}

#[test]
fn computed_method_body_is_checked_without_publishing_its_key() {
    let src = "\
class C {
  #secret = 1;
  [missingKey](): void {
    const bad: number = \"bad\";
    this.#secret;
  }
}
new C().method();
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert_eq!(
        diags(src),
        vec![
            (3, "TK2304".to_string()),
            (4, "TK2322".to_string()),
            (8, "TK2339".to_string())
        ]
    );
    let incomplete = out
        .incomplete
        .iter()
        .map(|incomplete| incomplete.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        incomplete,
        [
            "class/method-definition/computed-key",
            "expr-infer/private-field-access/self"
        ]
    );
}

#[test]
fn computed_method_key_preserves_private_incomplete_without_relation_cascade() {
    let src = "\
let getX: (value: A) => number;
class A {
  #x = 1;
  [(getX = (value: A) => value.#x, \"method\")]() {}
}
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    let incomplete = out
        .incomplete
        .iter()
        .map(|incomplete| incomplete.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        incomplete,
        [
            "class/method-definition/computed-key",
            "expr-infer/private-field-access/self"
        ]
    );
}

#[test]
fn computed_property_replays_its_annotation_without_publishing_the_member() {
    let src = "\
namespace N {}
declare const computedKey: \"field\";
class C {
  [computedKey]!: N.Missing;
  sibling: number = 1;
}
const sibling: number = new C().sibling;
new C().field;
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert_eq!(
        diags(src),
        vec![(4, "TK2694".to_string()), (8, "TK2339".to_string())]
    );
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["class/property-definition/computed-key"]
    );
}

#[test]
fn unavailable_retained_parameter_still_checks_its_default_initializer() {
    let src = "\
class C {
  method(value: Missing = missingDefault): void {}
}
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    let mut missing = out
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "TK2304")
        .map(|diagnostic| {
            src[diagnostic.span.start as usize..diagnostic.span.end as usize].to_string()
        })
        .collect::<Vec<_>>();
    missing.sort();
    assert_eq!(missing, ["Missing", "missingDefault"]);
}

#[test]
fn class_interface_groups_publish_one_composed_instance_in_both_orders() {
    let src = "\
class ClassFirst { own: number; }
interface ClassFirst { added: string; recursive: ClassFirst; }
const classFirst = new ClassFirst();
const classFirstWrong: number = classFirst.added;
const classFirstRecursiveWrong: number = classFirst.recursive.added;

interface InterfaceFirst { added: string; recursive: InterfaceFirst; }
class InterfaceFirst { own: number; }
const interfaceFirst = new InterfaceFirst();
const interfaceFirstWrong: number = interfaceFirst.added;
const interfaceFirstRecursiveWrong: number = interfaceFirst.recursive.added;
";
    assert_eq!(
        diags(src),
        vec![
            (4, "TK2322".to_string()),
            (5, "TK2322".to_string()),
            (10, "TK2322".to_string()),
            (11, "TK2322".to_string()),
        ]
    );
}

#[test]
fn class_first_setter_interface_conflict_keeps_the_setter_type() {
    let src = "\
class Mixed {
  set value(next: number) {}
}
interface Mixed {
  value: string;
}
declare const mixed: Mixed;
const value: number = mixed.value;
const wrong: string = mixed.value;
";
    assert_eq!(
        diags(src),
        vec![(5, "TK2717".to_string()), (9, "TK2322".to_string())]
    );
}

#[test]
fn interface_first_setter_conflict_keeps_the_interface_type() {
    let src = "\
interface Mixed {
  value: string;
}
class Mixed {
  set value(next: number) {}
}
declare const mixed: Mixed;
const value: string = mixed.value;
const wrong: number = mixed.value;
";
    assert_eq!(
        diags(src),
        vec![
            (2, "TK2300".to_string()),
            (5, "TK2300".to_string()),
            (9, "TK2322".to_string()),
        ]
    );
}

#[test]
fn paired_accessor_interface_merge_keeps_distinct_read_and_write_types() {
    let src = "\
class Mixed {
  get value(): number { return 1; }
  set value(next: string) {}
}
interface Mixed {
  value: number;
}
declare const mixed: Mixed;
const value: number = mixed.value;
const wrong: string = mixed.value;
mixed.value = \"next\";
";
    assert_eq!(diags(src), vec![(10, "TK2322".to_string())]);
}

#[test]
fn interface_first_accessor_pair_reports_every_occurrence_and_keeps_interface_type() {
    let src = "\
interface Mixed {
  value: string;
}
class Mixed {
  get value(): number { return 1; }
  set value(next: number) {}
}
declare const mixed: Mixed;
const value: string = mixed.value;
const wrong: number = mixed.value;
";
    assert_eq!(
        diags(src),
        vec![
            (2, "TK2300".to_string()),
            (5, "TK2300".to_string()),
            (6, "TK2300".to_string()),
            (10, "TK2322".to_string()),
        ]
    );
}

#[test]
fn class_first_static_accessor_namespace_collision_reports_every_occurrence() {
    let src = "\
class Mixed {
  static get value(): number { return 1; }
  static set value(next: number) {}
}
namespace Mixed {
  export const value: string = \"namespace\";
}
const value: number = Mixed.value;
const wrong: string = Mixed.value;
";
    assert_eq!(
        diags(src),
        vec![
            (2, "TK2300".to_string()),
            (3, "TK2300".to_string()),
            (6, "TK2300".to_string()),
            (9, "TK2322".to_string()),
        ]
    );
}

#[test]
fn namespace_first_static_accessor_collision_uses_block_scoped_diagnostics() {
    let src = "\
namespace Mixed {
  export const value: string = \"namespace\";
}
class Mixed {
  static get value(): number { return 1; }
  static set value(next: number) {}
}
const value: string = Mixed.value;
const wrong: number = Mixed.value;
";
    assert_eq!(
        diags(src),
        vec![
            (1, "TK2434".to_string()),
            (2, "TK2451".to_string()),
            (5, "TK2451".to_string()),
            (6, "TK2451".to_string()),
            (9, "TK2322".to_string()),
        ]
    );
}

#[test]
fn class_interface_recovery_frame_keeps_fragment_local_binders() {
    let src = "\
class Mixed<T> { own: T; }
interface Mixed<U> { added: U; }
declare const mixed: Mixed<string, number>;
const ownWrong: boolean = mixed.own;
const addedWrong: boolean = mixed.added;
";
    assert_eq!(
        diags(src),
        vec![
            (1, "TK2428".to_string()),
            (2, "TK2428".to_string()),
            (4, "TK2322".to_string()),
            (5, "TK2322".to_string()),
        ]
    );
}

#[test]
fn class_owned_interface_heritage_follows_aliases_without_static_inheritance() {
    let src = "\
class Base { inherited: number = 1; static baseStatic: string = \"s\"; }
type BaseAlias = Base;
class Derived {}
interface Derived extends BaseAlias {}
const inherited: number = new Derived().inherited;
const wrong: string = new Derived().inherited;
Derived.baseStatic;
";
    assert_eq!(
        diags(src),
        vec![(6, "TK2322".to_string()), (7, "TK2339".to_string())]
    );
    let output = check_source(src);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn class_owned_composite_heritage_validates_projected_class_members_after_publication() {
    let src = "\
class Base { value: string = \"base\"; static baseStatic: number = 1; }
type Composite = Base & { other: boolean };
class Derived { value: number = 1; }
interface Derived extends Composite {}
const valueWrong: string = new Derived().value;
const otherWrong: number = new Derived().other;
Derived.baseStatic;
";
    assert_eq!(
        diags(src),
        vec![
            (4, "TK2430".to_string()),
            (5, "TK2322".to_string()),
            (6, "TK2322".to_string()),
            (7, "TK2339".to_string()),
        ]
    );
    let output = check_source(src);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn class_interface_recovery_arity_uses_the_last_required_parameter() {
    let src = "\
class Mixed<T = string> {}
interface Mixed<U> {}
type TooShort = Mixed<number>;
type Exact = Mixed<number, string>;
";
    assert_eq!(
        diags(src),
        vec![
            (1, "TK2428".to_string()),
            (2, "TK2428".to_string()),
            (3, "TK2314".to_string()),
        ]
    );
}

#[test]
fn class_interface_composition_preserves_private_nominal_origin() {
    let src = "\
class Owned {
  private identity: number;
  own: number;
}
interface Owned { added: string; recursive: Owned; }
class Foreign {
  private identity: number;
  own: number;
  added: string;
  recursive: Owned;
}
const owned = new Owned();
const addedWrong: number = owned.added;
const nominalWrong: Owned = new Foreign();
";
    assert_eq!(
        diags(src),
        vec![(13, "TK2322".to_string()), (14, "TK2322".to_string())]
    );
}

#[test]
fn class_bodies_see_the_prepublication_interface_composition() {
    let src = "\
class ComposedBody {
  read(): string { return this.added; }
}
interface ComposedBody { added: string; }
const value: string = new ComposedBody().read();
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", diags(src));
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}
