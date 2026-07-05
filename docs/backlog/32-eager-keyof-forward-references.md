---
id: 32
title: Eager keyof over forward-referenced declarations (silent FN family)
---

# 32 — Eager `keyof` over forward references

**Summary.** `keyof` in a member annotation evaluates eagerly at fill time, so
`interface First { k: keyof Other }` with `Other` declared LATER yields a silent
false negative (`k` checks nothing; tsc errors on misuse — probe `keyof_fwd_pre.ts`,
review_wu). Empirically pre-existing at HEAD `eff3eb3` with no heritage involved;
the b28 heritage force-fill added one more route into the family (a heritage-filled
class member with `keyof NotYetFilledInterface`).

## Problem

Same smell as backlog 29 chased: a resolution-order hazard silently degrades to a
permissive type with no primary diagnostic. Any eager type-level operator (`keyof`
today; indexed access likely) over a not-yet-filled declaration is affected.

## Approach / acceptance

Either demand-driven `keyof` (defer until the operand's fill completes — the
evaluator's demand machinery from M25–M27 is the natural home) or a fill-ordering
pass that resolves declaration dependencies before member lowering. Corpus first:
forward-referenced interface/class/alias operands, heritage-forced fills, mutual
shapes; cross-check tsc 6.0.3. Audit indexed access for the same hazard.

## Touch points

`src/check/checker/annotations.rs` (keyof lowering), `decls.rs` (fill ordering),
possibly `eval.rs` (demand-driven keyof).

<!-- Origin: warm-ups sprint re-review note 1 (2026-07-05), attribution-proven
     pre-existing in an isolated HEAD worktree. -->
