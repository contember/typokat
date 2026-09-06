---
id: 60
title: Fresh literals vs union targets — excess and assignability silently skipped
---

# 60 — Fresh object literals vs union targets

**Summary.** Two probe-verified silent-FN shapes at the fresh-literal × union boundary
(2026-07-07; tsc 6.0.3 errors on all, typokat silent):

- **Excess-property check skipped** for `A | null` and multi-shape unions:
  `const u: A | B = { a: "s", extra: 1 }` and `const v: A | null = { a: "s", extra: 1 }`
  pass (only `A | undefined` works). `contextual_literal_target`
  (`crates/typokat-check/src/check/checker/expr.rs:841-856`)
  unwraps a union only when every non-shape member is exactly `Undefined`, and
  `check_object_excess_properties` no-ops on a non-object target.
- **Assignability miss on unions of optional-member objects:**
  `const z: { a?: number } | { b?: string } = { a: "s" }` is silent (tsc TS2322) — the
  literal apparently satisfies one member vacuously; reproduces without any evaluator
  involvement (M2/M4/M30 union-source relation of fresh literals).

## Discriminated initializer compatibility control

This item also owns the pre-existing false positive at
`tests/cases/b44_as_const/assignment_semantic_reduction.ts:29:5`. The valid initializer
`{ kind: "b", value: "start" }` is rejected against
`{ kind: "a"; value: number } | { kind: "b"; value: string }` after its discriminator becomes
`kind: string`. Strict TypeScript 6.0.3 accepts it; pre-feature, R1a and the frozen publication/role
candidates each report the same single TK2322. The internal context-loss point is not yet established.

Leader disposition: this existing declaration-initializer gap is deferred from bounded as-const
delivery to the fresh-literal/union-target owner. It is not B51's later transfer/copy failure and does
not authorize deferring the introduced M2 errors at lines 6/25. Preserve all assertions and positive
controls in the mixed fixture; do not disable it wholesale. Include this valid discriminator case
alongside the union excess/optional-member negative controls below when implementing this item.

## Approach / acceptance

**Acceptance spec (ready):** the disabled
[`tests/cases/sr_deferred_ledger/b60_fresh_literal_unions.ts`](../../tests/cases/sr_deferred_ledger/b60_fresh_literal_unions.ts)
fixture pins the three dropped-error shapes (multi-shape-union excess, `| null` excess,
optional-member-union absorption) plus the clean controls.

Model tsc's union rules for fresh literals: excess checking against the union's
matching member set (tsc's "most properties in common" discrimination + the union excess
rule), and the assignability path must not let optional-member union members vacuously
absorb a fresh literal with wrong-typed known properties. `| null` unwrap is the cheap
first slice. Corpus first (union excess shapes incl. `| null`, `| undefined | null`,
multi-shape, discriminated unions must stay working); cross-check tsc 6.0.3 --strict.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs` (`contextual_literal_target`),
`crates/typokat-check/src/check/checker/assignment.rs` (excess against unions),
`crates/typokat-relate/src/relate/relation/mod.rs`
(fresh-literal union-target rule), m30 corpus extension.

<!-- Origin: cross-cutting soundness review 2026-07-07 (modules reviewer #3 + evaluator observation), leader-verified. -->
