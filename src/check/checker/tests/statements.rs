//! WU1 (soundness-review-fixes sprint) end-to-end tests. Pins the three findings:
//! nested assignment expressions are checked in every position, un-annotated return
//! inference unions ALL value returns (order-independent), and `for`/`for-in`/
//! `for-of`/`do` bodies plus `throw` operands are bound and checked. Fixture
//! acceptance lives in `sr_wu1_expressions/`.

use crate::driver::check_source;

/// Sorted `(1-based line, code)` of every diagnostic, keyed on its primary-span
/// start line (matching the conformance harness's mapping).
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

// ---- Finding 1: nested assignment expressions are checked in every position ----

/// An assignment in a ternary arm is checked (`(a = "bad")` with `a: number`).
#[test]
fn nested_assignment_in_ternary_arm_is_checked() {
    let src = "\
let a: number = 0;
const t = a > 0 ? (a = \"bad\") : 0;
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// An assignment inside an array literal element is checked.
#[test]
fn nested_assignment_in_array_element_is_checked() {
    let src = "\
let a: number = 0;
const arr = [a = \"z\"];
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// An assignment used as another assignment's RHS: only the inner (type-wrong)
/// assignment errors; the outer sees the inner's value (a `string`, assignable to
/// `s`).
#[test]
fn nested_assignment_as_assignment_rhs_checks_only_inner() {
    let src = "\
let a: number = 0;
let s: string = \"\";
s = (a = \"wrong\");
";
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// An assignment used as an `if` condition is checked.
#[test]
fn nested_assignment_in_if_condition_is_checked() {
    let src = "\
let a: number = 0;
if (a = \"cond\") {}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A well-typed nested assignment stays clean (no false positive).
#[test]
fn well_typed_nested_assignment_is_clean() {
    let src = "\
let a: number = 0;
const good = (a = 5);
";
    assert!(
        diags(src).is_empty(),
        "a well-typed nested assignment must not error"
    );
}

// ---- Finding 2: un-annotated return inference unions all returns, order-free ----

/// A function returning `1` then `"x"` infers `string | number`; assigning its call
/// to a `number` is `TK2322`. The bug was inferring only the first return (`number`).
#[test]
fn return_inference_unions_all_returns() {
    let src = "\
function mixed(cond: boolean) {
  if (cond) return 1;
  return \"x\";
}
const n: number = mixed(true);
";
    assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
}

/// The inferred return type does not depend on visitation order: the same function
/// with the string return FIRST still infers `string | number` and errors.
#[test]
fn return_inference_is_order_independent() {
    let first_number = "\
function mixed(cond: boolean) {
  if (cond) return 1;
  return \"x\";
}
const n: number = mixed(true);
";
    let first_string = "\
function mixed(cond: boolean) {
  if (cond) return \"x\";
  return 1;
}
const n: number = mixed(true);
";
    // Both orders produce exactly one TK2322 at the call site (line 5).
    assert_eq!(diags(first_number), vec![(5, "TK2322".to_string())]);
    assert_eq!(diags(first_string), diags(first_number));
}

/// Same-type returns still infer that single type (no spurious union): `number`.
#[test]
fn return_inference_same_type_stays_precise() {
    let src = "\
function twoSame(cond: boolean) {
  if (cond) return 1;
  return 2;
}
const ok: number = twoSame(true);
";
    assert!(
        diags(src).is_empty(),
        "a same-type multi-return must infer that type"
    );
}

// ---- Finding 3: loop forms + throw operands are bound and checked ----

/// A C-style `for` body is checked, and the loop variable is bound (usable in the
/// test/update and, here, the body).
#[test]
fn for_statement_body_and_variable_are_checked() {
    let src = "\
for (let i = 0; i < 3; i++) {
  const bad: number = \"x\";
  const ok: number = i;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A `for-in` body is checked and the key variable is typed `string`.
#[test]
fn for_in_body_and_string_key_are_checked() {
    let src = "\
for (const k in { a: 1 }) {
  const bad: number = \"y\";
  const key: string = k;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A `for-in` key is `string`, so binding it to a `number` is `TK2322` (proves the
/// key is really typed, not left as the error type).
#[test]
fn for_in_key_is_string_not_number() {
    let src = "\
for (const k in { a: 1 }) {
  const n: number = k;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A `for-of` body is checked and the element variable is typed as the source's
/// element type (`number` for `[1, 2]`).
#[test]
fn for_of_body_and_element_type_are_checked() {
    let src = "\
for (const v of [1, 2]) {
  const bad: number = \"z\";
  const ok: number = v;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A `for-of` element over `[1, 2]` is `number`, so binding it to a `string` is
/// `TK2322` (proves the element type is really computed).
#[test]
fn for_of_element_type_is_computed_from_source() {
    let src = "\
for (const v of [1, 2]) {
  const s: string = v;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A `do … while` body is checked.
#[test]
fn do_while_body_is_checked() {
    let src = "\
do {
  const bad: number = \"w\";
} while (false);
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A nested function inside a loop body is bound and checked (proves the body is
/// recursed through the normal statement/block machinery).
#[test]
fn nested_function_in_loop_body_is_checked() {
    let src = "\
for (const v of [1, 2]) {
  const f = (p: number) => {
    const bad: string = p;
  };
}
";
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// A `throw` operand is type-checked, so a bad argument to a call in the operand is
/// `TK2345` (previously the operand was used only for reachability).
#[test]
fn throw_operand_is_checked() {
    let src = "\
declare function takesNumber(x: number): void;
throw takesNumber(\"oops\");
";
    assert_eq!(diags(src), vec![(2, "TK2345".to_string())]);
}

// ---- LabeledStatement: bind the body so labeled loops/blocks resolve names ----

/// A labeled `for-of` gets its head scope: the loop variable resolves (no spurious
/// `TK2304`) and its body's type error is reported (exactly one `TK2322`).
#[test]
fn labeled_for_of_binds_loop_variable_and_checks_body() {
    let src = "\
outer: for (const v of [1, 2]) {
  const s: string = v;
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A labeled block binds its declarations: `x` resolves (no `TK2304`) and the
/// type-wrong assignment is the only diagnostic (`TK2322`).
#[test]
fn labeled_block_binds_declarations() {
    let src = "\
blk: {
  let x: number = 0;
  const s: string = x;
}
";
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// Control: a labeled `while` still type-checks its body (label is transparent).
#[test]
fn labeled_while_body_is_checked() {
    let src = "\
loop: while (false) {
  const bad: number = \"x\";
}
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}
