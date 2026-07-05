---
id: 29
title: Silent alias-cycle permissiveness (error-typing with no primary diagnostic)
---

# 29 — Silent alias-cycle permissiveness

**Summary.** On-demand alias resolution that re-enters itself silently yields the
error type with **no primary diagnostic**, and the M22 "error type is silent
downstream" discipline then suppresses everything — a fully permissive value with
zero errors. Proven pre-existing at HEAD `db0c8b8` with no mapped/conditional syntax
(M26 review attribution probes ATTR1/ATTR2): `type Y = Y | null` gets no `TK2456`
(tsc: TS2456), and `type X = { a: X | null }` — which is LEGAL — has member reads
silently error-typed (`const s: string = x.a` passes, `x.q` passes). Mutual mapped
aliases (`NE7`) hit the same mechanism: tsc TS2456×2, typokat exit 0 + permissive.

## Problem

Two intertwined defects, both false-negative families:
1. **Missing `TK2456` for non-conditional circular aliases** — M25 added surface
   re-entry detection for conditional aliases and M26 for mapped aliases, but plain
   aliases (`type Y = Y | null`, mutual pairs) still resolve to a silent error type.
2. **Legal recursion through members degrades to the error type** on the demand
   path (`type X = { a: X | null }` should type `x.a` as `X | null`; m5 handles the
   declaration-side representation, but the on-demand resolution path error-types
   it), silently suppressing all downstream checking.

The M22 discipline presumes a primary diagnostic exists whenever the error type is
produced; this path violates that invariant.

## Approach / acceptance

Unify alias-cycle handling: the surface re-entry detection (M25/M26 machinery)
generalizes to ALL alias resolution — genuine surface cycles report `TK2456` at the
declaration (then error-type, silent downstream — the invariant holds again); legal
recursion through members must resolve via the m5 named-recursive representation
instead of re-entering (no diagnostic, correct member types). Corpus first: plain
circular aliases (direct, mutual, through unions), legal member recursion reads and
writes, mixed shapes; cross-check tsc 6.0.3. Audit every other producer of the
error type for the "no primary diagnostic" smell while there.

## Touch points

`src/check/checker/decls.rs` (`resolve_type_decl` / `resolving_alias` context),
`annotations.rs` (demand path), m5 corpus interplay.

<!-- Origin: M26 adversarial review (2026-07-05) — NE7/NE9/ATTR1/ATTR2 attribution
     probes; the review's sharpest cross-cutting find. -->
