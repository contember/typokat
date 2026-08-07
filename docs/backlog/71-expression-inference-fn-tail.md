---
id: 71
title: expression and iteration traversal silent-FN tail
blocked-by: []
---

# 71 — expression and iteration traversal silent-FN tail

**Summary.** Expression positions that `infer_expr` still skips silently — each one a
dropped-error class in ordinary code. Filed from the 2026-07-10 soundness sprint's
WU4-A adversarial review (all verified pre-existing at that sprint's HEAD).

## Problem

- **Binary arithmetic results are never inferred** — `infer_expr` has no
  `BinaryExpression` arm, so `let i: number = 0; const bad: string = i + i;` drops
  `TK2322` even at top level (tsc: TS2322).
- **Template-literal interpolations are unchecked** — no `TemplateLiteral` arm, so
  `` `${a = "bad"}` `` never checks the embedded assignment or expression.
- **Array elisions and spreads are incomplete** — array holes, array/object spread elements,
  and call spread arguments can lose traversal or value obligations.
- **Tagged templates and iteration targets are incomplete** — tagged-template operands and
  assignment targets in `for-in`/`for-of` need structural traversal before their semantic rules
  can be checked.
- **Assignment targets containing nested lexical scopes are incomplete target-wide** — assignment-
  LHS reservation does not allocate callable/class body owners in static/computed/private members,
  assertion/`satisfies`/non-null wrappers, or array/object destructuring children (including
  defaults, rest, and computed keys). The whole target needs one owner while assertion and
  class-expression records remain independent and additive.
- **Update targets have the same nested-scope boundary across every representable target family** —
  prefix/postfix update reservation does not allocate callable/class owners in static or computed
  member bases, computed keys, private-field objects, or assertion/`satisfies`/non-null wrappers.
  The update needs a target-wide owner because update-operand traversal is otherwise supported.
- **`for-of` over a non-iterable / `for-in` over a non-object are undiagnosed**
  (tsc TS2488/TS2407) — the element type falls back to the error type (no cascade,
  but no diagnostic either). The full default library is now available; this item still owns the
  structural iteration-target checks and diagnostics that consume it.

## Approach / acceptance

Add the missing `infer_expr` arms (binary, template), elision/object/call spread and tagged-template
traversal, and the missing iteration-target obligations, reusing the existing operand-checking
machinery. Acceptance: a
conformance corpus pinning each family against
`tsc 6.0.3 --strict`; controls prove no over-reports on well-typed operands.
Until assignment-LHS lexical reservation is implemented, any assignment target containing a nested
arrow/function/class records `expr-infer/assignment-expression/nested-scope-target`, walks assertion
syntax without entering the nested scope, preserves the existing class-expression record, and
checks the RHS exactly once. Assertion records never substitute for the target record.
The analogous update target records `expr-infer/update-expression/nested-scope-target`; prefix and
postfix forms and all representable `SimpleAssignmentTarget` families share that identity, while
assertion/class-expression records remain additive.
Operator *result typing* fidelity (TK2362/2365 families) stays owned by backlog `45` —
this item only stops the silent skips.

The forward local-function call found by the same review shipped in the
[`2026-07-11 declaration-hoisting sprint`](../archive/sprint-2026-07-11-declaration-hoisting-parity.md).
The shipped surface-accounting inventory prevents new silent traversal gaps; this item owns these
concrete known families.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs`, `crates/typokat-check/src/check/checker/statements.rs`,
`crates/typokat-check/src/check/checker/calls.rs`, `tests/cases/` (new corpus), backlog `45`/`46`
boundaries.

<!-- Origin: sprint-2026-07-10-soundness-review-fixes run log (WU4-A/WU4-B byproducts). -->
