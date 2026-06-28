---
id: 09
title: Conditional types (T extends U ? X : Y) — M25
blocked-by: [./08-generic-constraints.md]
---

# 09 — Conditional types (`T extends U ? X : Y`)

**Summary.** First milestone of the type-level evaluation phase (tree-walked — the bytecode VM is a
deferred, profiling-gated refactor, [ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md);
architecture §7, §12). This is where the de Bruijn migration likely becomes necessary.

## Problem

Conditional types aren't evaluated. Needs: the `extends` check, `infer` extraction, and distribution
over unions.

## Approach / acceptance

Evaluate the `extends` check via the relation engine; support `infer` extraction; distribute over
unions. This is where you likely need the **de Bruijn migration** for type parameters (`infer` +
alpha-equivalent hash-consing want de Bruijn — see [`../reference/invariants.md`](../reference/invariants.md)
§2; the migration is localized to the type-param repr + `substitute`). Acceptance: fixtures covering
a basic conditional, `infer` extraction, and distribution over a union, matching tsc.

Performance/scalability acceptance is part of this milestone, not a later cleanup: conditional
evaluation must use a heap work-stack/trampoline rather than host recursion, memoize repeated
`(conditional, args)` evaluations keyed by interned `TypeId`s, and enforce a tsc-like instantiation
depth limit with cycle detection. A deep recursive conditional fixture must terminate with the
intended result or an intentional depth-limit diagnostic, not overflow the Rust stack.

## Touch points

Type-param repr (de Bruijn migration); the relation engine (extends check); conditional-type
evaluation in the checker; evaluator memoization/work-stack/depth-limit machinery.

<!-- Origin: dev roadmap M25 (was HANDOFF §3, the type-level VM phase). -->
