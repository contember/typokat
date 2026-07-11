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
- **Array-spread elements are skipped** — `infer_array_literal` explicitly skips
  spread elements, so `[...(a = "bad")]` escapes checking.
- **`for-of` over a non-iterable / `for-in` over a non-object are undiagnosed**
  (tsc TS2488/TS2407) — the element type falls back to the error type (no cascade,
  but no diagnostic either). Needs at least a structural iterability check; full
  fidelity needs `lib.d.ts` (backlog `14`).

## Approach / acceptance

Add the missing `infer_expr` arms (binary, template) and spread handling, reusing the
existing operand-checking machinery and add the missing iteration obligations. Acceptance: a
conformance corpus pinning each family against
`tsc 6.0.3 --strict`; controls prove no over-reports on well-typed operands.
Operator *result typing* fidelity (TK2362/2365 families) stays owned by backlog `45` —
this item only stops the silent skips.

The forward local-function call found by the same review shipped in the
[`2026-07-11 declaration-hoisting sprint`](../archive/sprint-2026-07-11-declaration-hoisting-parity.md). Backlog
[`73`](./73-unsupported-surface-audit.md) owns the systematic AST-variant inventory that prevents
new silent traversal gaps; this item owns these concrete known families.

## Touch points

`src/check/checker/expr.rs`, `src/check/checker/statements.rs`,
`src/check/checker/calls.rs`, `tests/cases/` (new corpus), backlog `45`/`46`
boundaries.

<!-- Origin: sprint-2026-07-10-soundness-review-fixes run log (WU4-A/WU4-B byproducts). -->
