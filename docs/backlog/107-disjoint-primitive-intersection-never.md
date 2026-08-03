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

Normalization alone does not close the surface gap. `Relater::relate_uncached` currently handles
the source-`any`/internal-error shortcut before the target-`never` rule, so even a normalized
target still accepts `any`. TypeScript has one deliberate exception to `any` bidirectionality:
`any` is not assignable to `never`. The internal error type must remain assignable to `never` to
suppress cascades; it cannot be coupled to the source-`any` exception.

The existing `intersection/disjoint-primitives-message` divergence therefore misclassified a
silent under-report as a message-only difference. The exact witness is
`tests/cases/m31_intersections/any_to_disjoint_primitives.ts`.

## Approach / acceptance

Collapse only intersections that are structurally provable to have no common primitive value.
The proof may compare primitive or `object`-keyword domains, unequal singleton literals of one
primitive domain, and finite unions composed entirely of those forms. Recursing through a union
only has to prove every relevant member pairing disjoint; it does not have to materialize a
distributed result or simplify an overlapping union. Keep `void` and `undefined` in one
potentially overlapping void-like domain. Preserve the existing error/`never`/`any` absorption
order and do not generalize this into an assignability query inside the interner.

In the relation engine, enforce the target-`never` decision before the source-`any` shortcut while
keeping the internal error shortcut distinct. The exact boundary is: `any` is not assignable to
`never`; the internal error type remains assignable to `never`; `never` remains the bottom type;
and `any` remains bidirectionally assignable with every target other than `never`.

Keep potentially inhabited forms intact: a primitive with its own literal subtype (`string &
"x"`), overlapping finite primitive/literal/`object`-keyword unions, primitive branding (`string
& { readonly brand: ... }`), type parameters, templates, and general object intersections.
Conflicting object members and broader union distribution remain outside this item. `object &
string` is the narrow exception: its domains are structurally disjoint without inspecting object
members.

Acceptance is the new M31 fixture matching `tsc 6.0.3 --strict` in both member orders: `any` is
rejected from disjoint targets as `TK2322`, a reduced disjoint source flows to `never`, existing
concrete/`unknown` rejections remain, `never` still flows in, and the overlap/brand controls remain
inhabited. The boundary matrix also pins primitive/nullish distinctions: `number & bigint`,
`string & symbol`, `string & null`, `string & undefined`, `string & void`, `object & string`, and
`null & undefined` reduce to `never`; `void & undefined` remains inhabited (tsc normalizes it to
`undefined`); and `0 & -0` remains inhabited as `0`. Add interner unit coverage for order
independence, recursive finite-union domain proofs, singleton-literal domains, and every preserved
boundary above.

Add relation unit coverage for the `any`/`never` rejection and its three controls: internal error
to `never`, `never` to `any` and other targets, and `any` in both directions with non-`never`
targets. The committed M31 fixture remains the surface acceptance test; no second fixture is
required.

## Touch points

- `crates/typokat-types/src/types/intern/operators.rs`
- `crates/typokat-types/src/types/intern/tests.rs`
- `crates/typokat-relate/src/relate/relation/mod.rs`
- `crates/typokat-relate/src/relate/relation/tests.rs`
- `tests/cases/m31_intersections/any_to_disjoint_primitives.ts`
- `docs/reference/divergences.md`

<!-- Origin: default-library cutover closure WU5 adversarial review. -->
