---
id: 60
title: Fresh literals vs union targets — excess and assignability silently skipped
---

# 60 — Fresh object literals vs union targets

**Summary.** Two probe-verified silent-FN shapes at the fresh-literal × union boundary
(2026-07-07; tsc 6.0.3 errors on all, typokat silent):

- **Excess-property check skipped** for `A | null` and multi-shape unions:
  `const u: A | B = { a: "s", extra: 1 }` and `const v: A | null = { a: "s", extra: 1 }`
  pass (only `A | undefined` works). `contextual_literal_target` (`expr.rs:841-856`)
  unwraps a union only when every non-shape member is exactly `Undefined`, and
  `check_object_excess_properties` no-ops on a non-object target.
- **Assignability miss on unions of optional-member objects:**
  `const z: { a?: number } | { b?: string } = { a: "s" }` is silent (tsc TS2322) — the
  literal apparently satisfies one member vacuously; reproduces without any evaluator
  involvement (M2/M4/M30 union-source relation of fresh literals).

## Approach / acceptance

Model tsc's union rules for fresh literals: excess checking against the union's
matching member set (tsc's "most properties in common" discrimination + the union excess
rule), and the assignability path must not let optional-member union members vacuously
absorb a fresh literal with wrong-typed known properties. `| null` unwrap is the cheap
first slice. Corpus first (union excess shapes incl. `| null`, `| undefined | null`,
multi-shape, discriminated unions must stay working); cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/checker/expr.rs` (`contextual_literal_target`),
`src/check/checker/assignment.rs` (excess against unions), `src/relate/relation.rs`
(fresh-literal union-target rule), m30 corpus extension.

<!-- Origin: cross-cutting soundness review 2026-07-07 (modules reviewer #3 + evaluator observation), leader-verified. -->
