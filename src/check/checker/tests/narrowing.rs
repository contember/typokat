//! M7 end-to-end soundness tests for control-flow narrowing.
//! Pins branch scoping, symbol isolation, reassignment resets, and unrecognized
//! guard behavior; per-operation math is unit-tested in `flow.rs`.

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

/// The narrowed branch is genuinely clean while the wide references error —
/// narrowing both *enables* the in-branch assignment and *does not escape* the
/// `if` (the trailing reference re-errors). This is the headline behaviour the
/// `typeof.ts` fixture observes.
#[test]
fn narrowing_clears_in_branch_and_does_not_escape() {
    let src = "\
function f(x: string | number) {
  const wide: string = x;
  if (typeof x === \"string\") {
const s: string = x;
  } else {
const n: number = x;
  }
  const after: string = x;
}
";
    // Only the two wide references (lines 2 and 8) error; the narrowed
    // then/else assignments (lines 4, 6) are clean.
    assert_eq!(
        diags(src),
        vec![(2, "TK2322".to_string()), (8, "TK2322".to_string())]
    );
}

/// Narrowing `x` must never affect a *different* symbol `y` (symbol-keying).
#[test]
fn narrowing_does_not_affect_other_symbol() {
    let src = "\
function f(x: string | number, y: string | number) {
  if (typeof x === \"string\") {
const sx: string = x;
const sy: string = y;
  }
}
";
    // `x` is narrowed (line 3 clean); `y` is untouched, so line 4 errors.
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}

/// Reassigning a narrowed symbol resets it — a later reference sees the wide
/// (declared) type again, never the stale narrowing.
#[test]
fn reassignment_resets_narrowing() {
    let src = "\
function f(x: string | number) {
  if (typeof x === \"string\") {
const s1: string = x;
x = 1;
const s2: string = x;
  }
}
";
    // Before the reassignment `x` is `string` (line 3 clean); after `x = 1` it
    // is reset to `string | number`, so line 5 errors.
    assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
}

/// An unrecognized guard narrows nothing (no false negatives): both branches
/// see the wide type.
#[test]
fn unknown_guard_does_not_narrow() {
    let src = "\
function f(x: string | number, c: boolean) {
  if (c) {
const bad: string = x;
  } else {
const bad2: string = x;
  }
}
";
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (5, "TK2322".to_string())]
    );
}

/// Nested `if`s compose, and the complement of a typeof guard over a >2-member
/// union keeps the remaining members.
#[test]
fn nested_ifs_compose_over_three_member_union() {
    let src = "\
function f(x: string | number | boolean) {
  if (typeof x !== \"string\") {
if (typeof x === \"number\") {
  const n: number = x;
} else {
  const b: boolean = x;
}
  }
}
";
    // Every in-branch assignment is satisfied by composed narrowing: no errors.
    assert!(diags(src).is_empty(), "got {:?}", diags(src));
}

/// `!x` truthiness flips the branches; the else of `!z` is the truthy (object)
/// one, and the falsy then-branch keeps the nullish member.
#[test]
fn negated_truthiness_flips_branches() {
    let src = "\
function f(z: { a: number } | null) {
  if (!z) {
const bad: { a: number } = z;
  } else {
const ok: { a: number } = z;
  }
}
";
    // then-branch of `!z` has `z: null` → assignment errors (line 3); else is
    // the object → clean (line 5).
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// Narrowing enables otherwise-rejected member access (`TK2339`) and does not
/// escape: the pre-`if` access still errors.
#[test]
fn narrowing_enables_member_access() {
    let src = "\
function f(x: { a: number } | null) {
  const bad = x.a;
  if (x !== null) {
const ok: number = x.a;
  }
}
";
    // The wide `x.a` (line 2) is `TK2339`; after narrowing out null the access
    // (line 4) is clean.
    assert_eq!(diags(src), vec![(2, "TK2339".to_string())]);
}

/// Compound assignment must reset narrowing like `=`; the reset sits before the
/// compound-operator early return in `check_assignment`.
#[test]
fn compound_assignment_resets_narrowing() {
    let src = "\
function f() {
  let x: string | number = \"a\";
  if (typeof x === \"string\") {
const s1: string = x;
x += \"b\";
const s2: string = x;
  }
}
";
    // Before `x += "b"` the narrowing holds (line 4 clean); the compound
    // assignment resets it, so line 6 errors against `string | number`.
    assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
}

// -----------------------------------------------------------------------
// M8 — discriminated-union / `in` / `switch` narrowing soundness.
// -----------------------------------------------------------------------

/// Discriminant narrowing (`x.kind === "lit"`) refines inside the branch and
/// does **not escape** the `if`: the matching property is accessible in-branch
/// but the pre-`if` wide access still errors. Mirrors the `discriminated.ts`
/// fixture's headline behaviour as a unit-level soundness pin.
#[test]
fn discriminant_narrows_in_branch_and_does_not_escape() {
    let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape) {
  const wide = s.radius;
  if (s.kind === \"circle\") {
const r: number = s.radius;
  } else {
const d: number = s.side;
  }
  const after = s.radius;
}
";
    // The two wide `s.radius` accesses (lines 3 and 9) are TK2339; both branch
    // bodies are clean (then narrows to circle, else to square).
    assert_eq!(
        diags(src),
        vec![(3, "TK2339".to_string()), (9, "TK2339".to_string())]
    );
}

/// An **unrecognized discriminant narrows nothing** (no false negatives): a
/// member access on a different symbol, or a non-literal comparison, leaves the
/// union wide in both branches. Here `s.kind === t.kind` is not a literal
/// discriminant, so the in-branch `s.radius` still errors.
#[test]
fn unknown_discriminant_does_not_narrow() {
    let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape, t: Shape) {
  if (s.kind === t.kind) {
const bad = s.radius;
  }
}
";
    // The discriminant is not `x.prop === <literal>` → no narrowing → line 4
    // errors with TK2339 (radius not on every member of the wide union).
    assert_eq!(diags(src), vec![(4, "TK2339".to_string())]);
}

/// Discriminant narrowing keys on the **specific symbol**: `s.kind === "circle"`
/// narrows `s`, never a different union-typed symbol `t`.
#[test]
fn discriminant_narrows_only_its_symbol() {
    let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape, t: Shape) {
  if (s.kind === \"circle\") {
const r: number = s.radius;
const bad = t.radius;
  }
}
";
    // `s` is narrowed to circle (line 4 clean); `t` is untouched, so `t.radius`
    // (line 5) errors with TK2339.
    assert_eq!(diags(src), vec![(5, "TK2339".to_string())]);
}

/// `in`-operator narrowing refines both branches and keys on the symbol. The
/// pre-`if` wide access errors; each branch sees the narrowed member.
#[test]
fn in_operator_narrows_both_branches() {
    let src = "\
type Box = { a: number } | { b: string };
function f(x: Box) {
  const bad = x.a;
  if (\"a\" in x) {
const ok: number = x.a;
  } else {
const ok2: string = x.b;
  }
}
";
    // Only the wide `x.a` (line 3) errors; the then-branch narrows to `{ a }`
    // and the else-branch to `{ b }`.
    assert_eq!(diags(src), vec![(3, "TK2339".to_string())]);
}

/// `switch` narrows the discriminant per `case` and the narrowing does **not
/// escape** a clause: a clause accessing the *other* member's property errors.
#[test]
fn switch_narrows_per_case_and_does_not_escape() {
    let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape): number {
  switch (s.kind) {
case \"circle\": {
  return s.radius;
}
case \"square\": {
  return s.side;
}
  }
  return 0;
}
function bad(s: Shape) {
  switch (s.kind) {
case \"circle\": {
  const w: number = s.side;
  break;
}
  }
}
";
    // The circle/square clauses are clean (narrowed); the `bad` switch's circle
    // clause accesses `s.side` (line 16) → TK2339 (narrowed to circle).
    assert_eq!(diags(src), vec![(16, "TK2339".to_string())]);
}

/// A falling-through `case` must not let the next clause over-narrow; per-case
/// narrowing is suppressed on fallthrough to avoid false negatives.
#[test]
fn switch_fallthrough_is_conservative() {
    let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape) {
  switch (s.kind) {
case \"circle\":
case \"square\": {
  const bad = s.side;
}
  }
}
";
    // `case "circle":` is empty → falls through into `case "square":`, whose
    // body therefore could see a circle too. Narrowing is suppressed (wide
    // union), so `s.side` (line 6) errors with TK2339.
    assert_eq!(diags(src), vec![(6, "TK2339".to_string())]);
}

/// `switch` reassignment reset still holds across clauses: assigning the
/// discriminant symbol inside a clause drops its narrowing for later references
/// in that clause.
#[test]
fn switch_clause_respects_reassignment_reset() {
    let src = "\
function f(x: string | number) {
  switch (typeof x) {
case \"string\": {
  x = 1;
  const s: string = x;
}
  }
}
";
    // The discriminant is `typeof x` (not a member-access discriminant), so the
    // switch installs no narrowing; `x = 1` then resets any narrowing and `x`
    // stays `string | number`, so line 5 errors (TK2322). This pins that a
    // switch clause does not leave a stale narrowing.
    assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
}
