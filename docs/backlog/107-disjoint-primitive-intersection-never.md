---
id: 107
title: Disjoint primitive intersections normalize to never
---

# 107 — Disjoint primitive intersections normalize to `never`

**Summary.** HIGH silent false negative, effort S: a provably disjoint primitive intersection keeps
an ordinary intersection identity, so `any` can enter a target that TypeScript reduces to `never`.

## Problem

`Interner::intersection` in `crates/typokat-types/src/types/intern/operators.rs` flattens and
canonicalizes members but deliberately leaves `string & number` and similar disjoint primitive
sets unreduced. Most concrete sources still fail the per-member target relation, which made the
gap look cosmetic. `any` is assignable to each retained member, however, so typokat accepts it
without a diagnostic while TypeScript 6.0.3 reports `TS2322: Type 'any' is not assignable to type
'never'`. A value of the intersection also fails to flow back to `never`.

The existing `intersection/disjoint-primitives-message` divergence therefore misclassified a
silent under-report as a message-only difference. The exact witness is
`tests/cases/m31_intersections/any_to_disjoint_primitives.ts`.

## Approach / acceptance

Collapse only intersections that are structurally provable to have no common primitive value:
disjoint primitive domains and unequal singleton literals of one primitive domain. Preserve the
existing error/`never`/`any` absorption order and do not generalize this into an assignability query
inside the interner.

Keep potentially inhabited forms intact: a primitive with its own literal subtype (`string &
"x"`), primitive branding (`string & { readonly brand: ... }`), type parameters, templates,
unions, and object intersections. Conflicting object members and other broader intersection
reduction remain outside this item.

Acceptance is the new M31 fixture matching `tsc 6.0.3 --strict` in both member orders: `any` is
rejected from disjoint targets as `TK2322`, a reduced disjoint source flows to `never`, existing
concrete/`unknown` rejections remain, `never` still flows in, and the overlap/brand controls remain
inhabited. Add interner unit coverage for order independence, singleton-literal domains, and every
preserved boundary above.

## Touch points

- `crates/typokat-types/src/types/intern/operators.rs`
- `crates/typokat-types/src/types/intern/tests.rs`
- `tests/cases/m31_intersections/any_to_disjoint_primitives.ts`
- `docs/reference/divergences.md`

<!-- Origin: default-library cutover closure WU5 adversarial review. -->
