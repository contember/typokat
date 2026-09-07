---
id: 120
title: Preserve semantic construction order for generic constraints
blocked-by: []
---

# 120 — Preserve semantic construction order for generic constraints

**Summary.** Complete generic constraint instantiation and source-construction fidelity without
reconstructing lost order from canonical types. Effort XL. The bounded source-owned protocol in
[ADR-0040](../decisions/0040-complete-registered-callables-at-source-demand-boundaries.md) is now
explicitly required by the as-const sprint; broader construction/lazy-source compatibility remains here.
Deferral does not authorize new diagnostic losses or restoring old missed errors in that proof.

## Problem

TypeScript 6.0.3 cyclic inference fallback can depend on semantic constituent creation and reuse.
Declaration-first checking, forward calls and callable relations need not construct signatures in
the same order. A plain function copy need not resolve its signature. Written union order, binder
ordinal and first textual use are therefore insufficient reconstruction rules.

For example, a declaration-first call of the following signature rejects assigning its result to
`{ c: { b: unknown } }`; a forward call before the declaration accepts that assignment but rejects
the deeper `{ c: { b: { c: unknown } } }` target:

```ts
declare function graph<A extends C | B, B extends { c: C }, C extends { b: B }>(): B;
```

Aliases add another distinction: erased written arguments must not request unused inference
parameters, while retained applications can preserve argument order not visible in their members.
Declared compound returns and bound-to-bound demand expose further scheduling differences.

The [as-const sprint](../sprints/sprint-2026-08-17-as-const.md) preserves the failed planner experiments
and their oracle witnesses. Its intersection-only retained-expression proof was independently
accepted, but never integrated as a general union/source migration. Later planner candidates also
introduced erased-alias false cleans; those candidates cannot be shipped under this deferral.

## Approach / acceptance

- Keep exact source fixtures and pinned tsc oracles for erased/retained aliases, intersections,
  unions, forward calls, copies, callable relations, array constituent reuse and return/bound demand.
  Current witnesses include `tests/cases/b44_as_const/constraint_*`, `source_construction_*`, and
  `recursive_constraint_fallbacks.ts`; preserve their assertions when separating corpus ownership.
- Establish which failures predate the feature and which were introduced by an experiment. Restore
  the pre-change diagnostic contract before integrating any shared inference changes.
- Preserve immutable published meaning and single-owner source effects. No query-chosen once-cell,
  duplicate semantic walk, syntax/name heuristic, fixture dispatch or post-call inference repair.
- [ADR-0040](../decisions/0040-complete-registered-callables-at-source-demand-boundaries.md) supersedes
  proposed ADR-0034. Prove its complete admission, field-demand, early-decision, snapshot and effect
  contract before source cutover; the old restricted tracer cannot stand in for constructed returns.
- C0 structural facts (`93058337`) and C1a consumed-input/scope receipts (`ae145bc8`) are available.
  General final-basis authentication and batch resolution are not prerequisites established by the
  bounded protocol. Its narrow current-binder join, closed-use plan and registered-callable construction
  are now on backlog 44's critical path. Do not restore a process-shared production Binder.
- Require direct malformed/poison/rollback/freeze tests, real `CheckerSession` source witnesses,
  independent adversarial review, differential and official regression gates before cutover.

### Retained design-review constraints

The unfinished C1b design uses an exact owned dependency read-set and borrowed input views, not a
second Binder or lookup cache. With prefix acceleration deferred, discard its authority before any
fallible freeze; even unchanged successful freeze stays unavailable. Product sealing must follow
successful product-map finalization. Historical C1a receipts alone do not authorize mutable state.

Independent preview found one omitted bounds dependency: `GlobalAugmentation.owner` can contain
`GlobalOwner::DeferredAmbientModule(id)`. A future basis must retain/compare `deferred_modules.len()`,
bounds-check that owner domain and audit its edges, even while deferred-module contents and
resolution remain excluded. Pin the existing ambient-global constructor and malformed owner IDs.
This is retained design evidence, not authorization to restart C1b during backlog-44 delivery.

## Touch points

Shared inference and declared-expression/query owners; source signature publication; binder
reference inputs and finalization; module/product identity joins. Coordinate general source demand
with [76](76-lazy-value-type-resolution.md), without treating this item as permission for full lazy
declaration migration.
