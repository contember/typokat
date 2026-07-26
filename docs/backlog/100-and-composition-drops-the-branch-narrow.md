---
id: 100
title: An `&&` condition inverts the narrow in the branch it guards
blocked-by: []
---

# 100 — An `&&` condition inverts the narrow in the branch it guards

**Summary.** `if (x !== null && anything) { … }` does not narrow `x` inside the branch — it narrows
it to the **negation**. Five one-line probes that `tsc 6.0.3 --strict` accepts produce five
`TK2322` false positives. The idiom is one of the most common in TypeScript. M23 shipped `&&`
narrowing its *right-hand side* and its corpus covers only that; `&&` as an `if` condition was never
tested. Effort M. **Until this is fixed the checker is unusable on ordinary code**, so it outranks
the remaining library families.

## Problem

```ts
declare const nn: number | null;
declare const flag: boolean;

if (nn !== null)                     { const a: number = nn; }  // clean, correct
if (nn !== null && flag)             { const b: number = nn; }  // TK2322: 'null' not assignable
if (nn !== null && nn !== 0)         { const c: number = nn; }  // TK2322
if (nn !== null && nn > 0)           { const d: number = nn; }  // TK2322
if (flag && nn !== null)             { const e: number = nn; }  // TK2322
if (nn !== null && nn !== 0 && flag) { const f: number = nn; }  // TK2322
```

`tsc 6.0.3 --strict --target es2025` reports **nothing**; typokat reports **five**. Reproduced on the
release binary at `12a15b5` and unchanged by the operator work that found it.

Two things make this worse than a missing narrow:

- **The type is inverted, not merely un-narrowed.** The reported type inside the *then* branch is
  `null` — the false-branch type. A guard that failed to narrow would report `number | null`.
- **It fires in either operand position** (`a && x !== null` too) and for any right operand — a bare
  boolean, an equality, a relational comparison. Nesting the same guards
  (`if (nn !== null) { if (nn > 0) { … } }`) narrows correctly, so the defect is in how `&&`
  composes into the branch, not in the individual guards.

These are false positives, which is the safe direction under this project's soundness policy, but on
an idiom this common the checker is unusable on real code until it is fixed. It also very likely has
a dropped-error mirror image in the `else` branch and in `||`, which the probes above do not cover.

## Why the corpus did not catch it

`tests/cases/m23_unstructured_narrowing/logical_ops.ts` tests exactly one shape —
`x !== null && takesString(x)`, i.e. `&&` narrowing its own right-hand side — plus the `||` and
ternary equivalents. All pass. No fixture anywhere puts an `&&` in an `if` condition over a union.
This is a **corpus hole**, not a regression: the M23 milestone shipped the tested half.

That is the same failure mode as [`96`](./96-randomized-differential-corpus.md): a hand-written
corpus covers the cases someone thought of. A differential corpus would have found this on day one.

## Approach / acceptance

Corpus first, and make it wide: both operand positions, `&&` and `||`, then/else branches, two and
three conjuncts, mixed guard kinds (`!== null`, `typeof`, `instanceof`, truthiness, discriminant),
and the `else` branch of each — the `else` cases are where a dropped error would hide, and dropped
errors matter more than these false positives.

The flow-node CFG (`src/check/checker/flowgraph.rs`, `flowgraph/exprs.rs::build_flow_logical`) is the
single narrowing model per [`invariants.md`](../reference/invariants.md) §1; fix it there rather than
special-casing `if` conditions. Cross-check every marker against `tsc 6.0.3 --strict`.

Acceptance: all six probes above are clean; the `else`-branch mirror reports what `tsc` reports;
`m23_unstructured_narrowing` stays green; official-suite ratchet shows no regression and probably
shows progress.

## Touch points

`src/check/checker/flowgraph.rs`, `src/check/checker/flowgraph/exprs.rs`,
`tests/cases/m23_unstructured_narrowing/`.

<!-- Origin: found while fixing backlog 45's operator result typing, 2026-07-26 — the implementing
     agent hit it in an unrelated probe and flagged it rather than working around it. Confirmed
     pre-existing by rebuilding at 12a15b5. -->
