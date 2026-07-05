---
id: 37
title: TK2344 constraint approximation for provable deferred arguments
---

# 37 — Constraint approximation for provable deferred arguments

**Summary.** `type UE<K> = Uppercase<Extract<K, string>>` over-reports TK2344 at the
declaration: the argument stays a deferred conditional, typokat checks it
conservatively, but tsc PROVES `Extract<K, string> <: string` via its conditional
constraint approximation and stays clean (leader probe m28_arb3.ts, M28 review
round 3). Unprovable shapes (`MyExclude<K, "a">`) error in both — tsc-exact.

## Problem

typokat has no constraint approximation for deferred conditional/instantiation
arguments: a conditional's best-known upper bound (tsc: roughly the union of branch
approximations / the distributive constraint) is never computed, so provably-safe
compositions against concrete constraints (mostly the string intrinsics) are the one
divergence pinned in `tests/cases/m28_utility_types/constraint_arguments.ts` (the
annotated UE line).

## Approach / acceptance

Model the upper-bound approximation of a deferred conditional (union of approximated
branches; for `Extract`-shape `T extends U ? T : never` that yields `U`-bounded) and
use it in the TK2344 argument check before falling back to the conservative verdict.
Acceptance: the UE divergence marker flips to clean with no FN elsewhere in
`constraint_arguments.ts`; README divergence entry deleted.

## Touch points

`src/check/checker/calls.rs` (TK2344 argument side), `src/check/checker/eval.rs`
(approximation helper over conditional templates).

<!-- Origin: M28 review round 3 + leader arbitration probe m28_arb3.ts (2026-07-05). -->
