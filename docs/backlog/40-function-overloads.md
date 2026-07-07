---
id: 40
title: Function overloads (declarations + resolution, TK2769)
---

# 40 — Function overloads

**Summary.** Overload declarations are ignored — only the implementation signature is
typed: with `h(x: number): number; h(x: string): string;` calls resolve against the
implementation (`x: number | string → number | string`), so `const n: number = h(1)`
over-reports TK2322, and a no-match call reports per-argument TK2345 instead of
`TK2769 No overload matches this call` (probe 2026-07-07). A mixed FP+FN family; Tier S
in the scope map; `lib.d.ts` (`14`) is full of overloads.

## Problem

Lowering collapses an overload set to the implementation signature. tsc semantics: calls
resolve against the overload list in order (the implementation signature is NOT callable
from outside), assignability relates the whole set, the implementation signature must be
compatible with each overload (TS2394), and TK2769 aggregates the best per-overload
failure.

## Approach / acceptance

Represent an ordered signature list on the function's type; call resolution walks
overloads per tsc (first match wins; TK2769 with the best-failure reason chain when none
match); include class/interface method overloads and construct-signature overloads.
Corpus first (resolution order, implementation-not-callable, TK2769 wording, TS2394);
cross-check tsc 6.0.3 --strict. This item's callability model is what unblocks `19`
(TK2349).

## Touch points

`src/binder/` (overload grouping), `src/types/repr.rs` (signature list),
`src/check/checker/calls.rs` (resolution + TK2769), `src/relate/relation.rs` (overload-set
assignability).

<!-- Origin: completion-roadmap review (2026-07-07); overloads deferred since F1 WU2 (see backlog 19). -->
