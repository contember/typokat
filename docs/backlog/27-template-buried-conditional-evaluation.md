---
id: 27
title: Evaluate conditionals buried in named alias / interface / class bodies
---

# 27 — Evaluate template-buried conditionals

**Summary.** `evaluate_type` fires only on a top-level Conditional/Instantiation/Union
demand (`src/check/checker/eval.rs`, phase-1 annotation path). A conditional buried
inside a named template body — `type W = { foo: IsString<string> }`, an interface
member, a class field — lowers under `building_template` and is never demanded, so it
stays a deferred node and relates conservatively (M25 review, probe `a1_alias_object.ts`;
inline annotations evaluate fine). Always the safe over-report direction, but a common
idiom (object-of-extractors), and the asymmetry (inline vs named) is surprising.

## Approach / acceptance

Make evaluation demand-driven through structure: either walk the lowered template type
and evaluate embedded conditional/instantiation nodes at instantiation time, or evaluate
lazily at relation/member-access time (mind the memo + budget discipline — the demand
span for TK2589 attribution needs a sensible owner). Corpus: extend
`m25_conditional_types/` with named-template shapes (alias object member, interface
member, class field, tuple/array element inside an alias), cross-checked vs tsc.

## Touch points

`src/check/checker/eval.rs` (structural demand), `decls.rs` (template instantiation),
possibly `relate/relation.rs` (lazy-eval hook). Watch invariants: never evaluate inside
the relation engine's immutable-store phase without a plan — evaluation interns new
types (`&mut Interner`), which the two-phase split exists to separate.

<!-- Origin: M25 adversarial review secondary note (2026-07-04). -->
