---
id: 0002
title: De Bruijn indices are scoped to infer binders; declaration type params stay named ids
status: accepted
date: 2026-07-04
---

# 0002 — De Bruijn indices are scoped to `infer` binders; declaration type params stay named ids

## Context

Architecture §3.1 calls for de Bruijn indices for type parameters and `infer`, so
alpha-equivalent generics hash-cons to the same node. Invariants §2 recorded named
unique `TypeParamId`s as a deliberate deferral and expected the conditional-types
milestone (backlog `09` / M25) to force the migration.

Planning M25 surfaced a conflict with invariant #1 (relation-cache soundness). The
3×`u32` relation cache is sound **because a `TypeId` alone determines a type's
meaning**: a named `TypeParamId` maps to exactly one declaration (and, since M24, to
its constraint). Migrating declaration params to bare de Bruijn indices makes open
types context-dependent — `TypeParam(#0)` under two different binders would intern
to the same `TypeId` with different meanings — poisoning the cache across
environments, the exact bug class of the project's sharpest historical defect
(architecture §6.3). Avoiding that requires canonical skolemization when relating
under binders plus folding constraints into binder-node identity: a deep relation-
engine redesign that **nothing in `09`–`12` actually forces**.

What M25 does need from de Bruijn is well-scoped: `infer` declarations introduce
binders *inside* a conditional-type node, and the evaluator's memoization wants
alpha-equivalent conditionals to share one interned node.

## Decision

We will use de Bruijn indices **only for `infer` binders, resolved within their
enclosing conditional-type node**. Alpha-equivalent conditionals hash-cons to the
same node; the indices never reach the relation engine unbound, because branches
are only related after the match substitutes them (a deferred conditional whose
branch still contains the node's own infer binder falls back to conservative
`No` — the sound over-report direction).

Declaration type parameters (`<T>` on functions, classes, aliases, interfaces)
**stay named unique `TypeParamId`s**, keeping every interned open type
context-free and the relation cache sound as-is. The M24 constraint column
continues to key on `TypeParamId`.

The full §3.1 migration (alpha-equivalent hash-consing of generic *declarations*)
is demoted to a measured optimization, ADR-0001 style: undertake it only if
profiling shows cross-declaration dedup / memo hit-rate on real code justifies the
skolemization redesign, and treat it as a relation-engine project, not a repr
rename.

## Consequences

- The relation cache and its soundness argument are untouched; M25's evaluator
  builds on the existing invariants instead of destabilizing them.
- Two alpha-equivalent generic *declarations* still do not share a node (unchanged
  from today; documented deviation in `src/types/repr.rs`). Evaluator memoization
  still hits where it matters: repeated instantiations of the same declaration —
  the dominant case in recursive type-level code.
- `substitute` must not capture: substituting under a conditional node leaves the
  node's own infer indices alone (they are bound, not free).
- Architecture §3.1's de Bruijn bullet and invariants §2's migration note now read
  through this ADR; both updated to point here.

## Alternatives considered

- **Full de Bruijn per §3.1 now** — rejected: requires canonical skolemization +
  constraint-in-binder-identity to keep the cache sound; high-risk surgery on
  invariant #1 with no forcing function in `09`–`12` and a payoff (cross-declaration
  dedup) we have no evidence we need.
- **No de Bruijn at all (fresh named ids for infer binders)** — rejected: every
  textually identical conditional in a re-instantiated alias body would intern a
  distinct node, defeating evaluation memoization keyed on interned `TypeId`s —
  the single biggest evaluator lever (architecture §7.2).
