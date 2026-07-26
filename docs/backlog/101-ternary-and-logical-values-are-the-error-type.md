---
id: 101
title: Ternary and logical expression *values* are the error type
blocked-by: []
---

# 101 — Ternary and logical expression *values* are the error type

**Summary.** `flag ? 1 : 2` and `a && b` produce the **error type** as their value, so anything they
flow into is unchecked. Four one-line probes that `tsc 6.0.3 --strict` rejects are silent in
typokat. This is the same silent-false-negative shape as [`45`](./45-operator-comparison-typing.md)'s
`infer_binary`, in the two remaining coarse-typed expression forms. Effort M.

## Problem

```ts
declare const flag: boolean;
declare const nn: number | null;

const a: string = flag ? 1 : 2;          // tsc TS2322 — typokat silent
const b: string = nn !== null && flag;   // tsc TS2322 — typokat silent
const c: string = flag || nn !== null;   // tsc TS2322 — typokat silent
const d: number = flag ? "x" : "y";      // tsc TS2322 — typokat silent
```

`tsc` reports **4**, typokat reports **0**. Measured on the release binary at `429e3dc`.

The error type is assignable to everything by design, so it does not merely lose precision — it
switches off every downstream check that consumes the value. A ternary is one of the most common
ways to produce a value in real code.

## Root cause

`src/check/checker/expr.rs:176-181` walks all three parts of a `ConditionalExpression` for their side
effects and then returns `well_known.error`; `infer_logical` does the same for `&&`/`||`/`??`. The
comment states the reasoning plainly:

> their *value* type is only ever a condition, never an assignment source in the subset, so a coarse
> result type is sufficient.

That premise held when the subset was M7-sized. It stopped holding as the subset grew, and nothing
re-checked it — exactly how [`45`](./45-operator-comparison-typing.md)'s `infer_binary` returned the
error type for every non-additive operator for months.

## Approach / acceptance

The result types are standard and small:

- **Ternary** — the union of the two arm types (with the arms checked under their respective flow
  branches, which `build_flow_conditional` already builds). Contextual typing of the arms by the
  target should follow the same path as any other contextually typed expression.
- **`&&`** — `falsy-part-of(left) | right`. **`||`** — `truthy-part-of(left) | right`.
  **`??`** — `non-nullish-part-of(left) | right`.

Corpus first, per [`dev-method.md`](../reference/dev-method.md) §1, and cross-check every marker
against `tsc 6.0.3 --strict`. Cover the value position of each operator, nested compositions, arms of
differing types, `never` arms, contextual typing from the target, and the interaction with
[`100`](./100-and-composition-drops-the-branch-narrow.md)'s narrowing (an arm must see its branch's
narrow). Expect the official-suite ratchet to show progress; watch for new false positives where the
error type was previously masking an unrelated gap.

`tests/surface/inventory.toml:508` records `expr-infer/conditional-expression/self` as
`disposition = "supported"` / `owner = "shipped"`, which overstates what exists — correct it with the
fix.

## Touch points

`src/check/checker/expr.rs` (`infer_logical`, the `ConditionalExpression` arm),
`tests/surface/inventory.toml`.

<!-- Origin: found by the backlog-100 corpus agent, 2026-07-26, while establishing why
     `const d: number = (nn !== null && flag) ? nn : 0` looked clean — it was vacuous. Leader
     re-verified the four probes against tsc 6.0.3 before filing. -->
