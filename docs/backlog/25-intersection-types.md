---
id: 25
title: Intersection types (A & B) in the type model
---

# 25 — Intersection types (`A & B`)

**Summary.** Intersection types do not exist in the type model — there is no
`Intersection` variant in `src/types/repr.rs`, and `type AB = { a: number } & { b: string };
const bad: AB = { a: 1 }` passes silently (tsc: TS2741 missing property). Every
intersection annotation lowers to something permissive, silencing all downstream
diagnostics — a false-negative family, same severity class as the rest-elements gap
(backlog `24`).

## Problem

Any code using `&` is effectively unchecked. It also blocks corpus coverage elsewhere:
the M24 circular-constraint fixture had to drop the `<T extends T & { x: number }>` case
(tsc reports TS2313 there; the composite-circularity walk cannot see members of a node
that doesn't exist), and the type-level milestones lean on intersections —
`10` (mapped types intersect with index signatures in idioms), `12` (utility types),
and same-name contravariant `infer` candidates intersect in tsc.

## Approach / acceptance

Add an interned intersection node (canonicalized per architecture §3.3: sort by `TypeId`,
dedup, flatten, `X & unknown → X`), relate it per tsc (source intersection: any member
satisfies target; target intersection: all members required; property collection merges
members), lower `&` annotations, and extend the M24 circularity walk to intersection
members. Fixture corpus first, per dev-method; cross-check tsc 6.0.3.

## Touch points

`src/types/repr.rs` / `intern.rs` / `hash.rs` (node + canonicalization),
`src/relate/relation.rs` (member rules), `src/check/checker/decls.rs` (lowering),
`src/check/checker/expr.rs` (member access over intersections), m24 corpus (the dropped
`T & X` circularity case).

<!-- Origin: M24 fix-loop (2026-07-04) — composite-circularity spec hit the missing node. -->
