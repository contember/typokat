---
id: 24
title: Rest elements in the type model (tuple rest + rest parameters)
---

# 24 — Rest elements in the type model

**Summary.** Neither tuple rest elements (`[string, ...number[]]`) nor function rest
parameters (`(...args: T[]) => R`) exist in the type model — both lower to something
silently permissive (probes: `const t: [string, ...number[]] = [1]` and a wrong-typed
callback against `(...args: never[]) => number` both pass). `src/types/repr.rs:330`
records optional/rest params as out of the M3 subset; M18 tuples are fixed-length only.

## Problem

A whole diagnostic class is silently dropped on any signature using rest syntax — a
false-negative family, and it blocks fixture coverage elsewhere: the M25 conditional-types
corpus had to avoid `[infer H, ...infer Rest]` peeling and `(...args: never[]) => infer R`
patterns entirely (the idiomatic forms of both), and variadic tuple spreads (`[...T, X]`)
are the standard type-level accumulator, which the mapped/tuple milestones (`10`+) and
arithmetic intrinsics (architecture §7.3) build on.

## Approach / acceptance

Add a rest slot to tuple and function-param reprs (identity-bearing, carried through
`substitute` and the structural hash), relate them per tsc's arity + element rules, and
lower variadic tuple spreads. Then extend the m25 corpus with the rest-based `infer`
patterns. Fixture corpus first, per dev-method; cross-check tsc 6.0.3.

## Touch points

`src/types/repr.rs` / `intern.rs` / `hash.rs` (repr + identity), `src/relate/relation.rs`
(tuple/function arity rules), `src/check/checker/decls.rs` (lowering), `src/check/infer.rs`
(candidate positions), m17/m18/m25 corpora.

<!-- Origin: M25 sprint planning (2026-07-04) — probes showed rest syntax silently permissive. -->
