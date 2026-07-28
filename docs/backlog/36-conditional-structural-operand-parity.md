---
id: 36
title: tsc parity for conditionals with structurally-wrapped deferred operands
---

# 36 — Conditional parity for structurally-wrapped deferred operands

**Summary.** M28 ships the FN-free conservative rule for deferred nodes nested inside
composite conditional operands (deep undecidable walk → stay deferred → reject both
branches). tsc resolves these shapes with MIXED results; matching it exactly needs its
eager-false shortcut modeled. Leader arbitration probe (tsc 6.0.3, M28 review round 2):

| Shape (concrete args) | tsc |
|---|---|
| `{ v: keyof T } extends { v: "a" }`, T=`{a:1}` | `"y"` |
| `(() => keyof T) extends () => "a"` | `"y"` |
| `[Uppercase<S>] extends ["ABC"]`, S=`"abc"` | `"y"` |
| `{ v: Uppercase<S> } extends { v: "ABC" }`, S=`"abc"` | `"n"` |
| `Uppercase<S>[] extends "ABC"[]`, S=`"abc"` | `"n"` |

## Problem

Object-wrapped `keyof` evaluates in tsc while object-wrapped intrinsics do not, yet
tuple-wrapped intrinsics do — emergent behavior of tsc resolving some generic
conditionals eagerly-FALSE at alias declaration (hypothesis: the
permissive/restrictive-instantiation assignability shortcut in
`getConditionalType`), so later concrete instantiations inherit `"n"` without ever
evaluating the nested node. typokat's deferred verdict over-reports on the five
fixture lines marked as divergences in
`tests/cases/m28_utility_types/conditional_positions.ts`.

## Approach / acceptance

Verify the hypothesis against the tsc source first (which shortcut fires per shape),
then model it: eager-false decision at template-build time where tsc makes one;
demand-evaluation of nested deferred nodes where tsc instantiation evaluates them
(keyof in structural positions, intrinsics in tuples). Acceptance: the five divergence
markers in `conditional_positions.ts` flip to tsc-exact and the
`docs/reference/divergences.md` entry is deleted; no new FN on the rest of the fixture.

## Touch points

`crates/typokat-check/src/check/checker/eval.rs` (`eval_conditional`/`decide_conditional`,
`operand_undecidable`), possibly template-build in `annotations.rs`/`decls.rs`.

<!-- Origin: M28 review round 2 + leader arbitration probe m28_arb2.ts (2026-07-05). -->
