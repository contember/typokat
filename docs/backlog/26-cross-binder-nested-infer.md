---
id: 26
title: Cross-binder nested infer (de Bruijn shifting on embed)
---

# 26 — Cross-binder nested `infer` (de Bruijn shifting on embed)

**Summary.** A nested conditional type that references an OUTER conditional's `infer`
binder (`T extends { a: infer U } ? (U extends string ? … ) : …`) is not modelled:
per ADR-0002 infer binders are flat per-node de Bruijn indices, so an outer binder
embedded inside a nested node is meaningless there (index collision with the inner
node's own binders). M25 ships the sound stopgap: such conditionals are **poisoned at
lowering** and stay deferred unless a definitive primitive-versus-`object` rejection can select
the false branch without observing the captured binder. The guarded false branch uses the ordinary
evaluation cycle/budget lifecycle; captured-infer true branches remain unavailable and relate
conservatively (over-report, corpus `m25_conditional_types/nested_infer.ts`; tsc resolves them).

## Problem

The conservative deferral is a false-positive family on an idiom real type-level code
uses (nested extraction pipelines). Before the stopgap it was worse — silently WRONG
evaluation (verdict inversions, fully silent files; see the M25 sprint run log's
probe table p1–p3) — but the right end state is resolving these like tsc.

## Approach / acceptance

Proper binder arithmetic, one of: **index shifting on embed** (classic de Bruijn —
shift free indices when a term is placed under a binder, unshift on extraction) or
**level-tagged binders** (de Bruijn levels; no shift on embed, renumber at binder
entry). Either way `substitute_infers` descends into nested conditional nodes with
correct index/level adjustment, and the poison flag is removed. Acceptance: the
`nested_infer.ts` over-report markers flip to tsc-equal resolution (the fixture is
rewritten to pin resolution), p1–p3 probe shapes match tsc, no regression in the rest
of the m25 corpus or the official suite.

## Touch points

`src/types/repr.rs` / `substitute.rs` (index arithmetic), `src/check/checker/
annotations.rs` (lowering: drop the poison, bind across frames with shifts),
`src/check/checker/eval.rs` (`substitute_infers` descent), `m25_conditional_types/
nested_infer.ts` (rewrite to resolution semantics).

<!-- Origin: M25 sprint (2026-07-04) — implementation-agent probes showed the unmodelled
     shape could drop errors; poisoned-deferral stopgap chosen over rushing binder
     arithmetic into the milestone. -->
