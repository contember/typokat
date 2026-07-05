---
id: 34
title: fix_params should evaluate deferred keyof before gating (missed TS2345)
---

# 34 — `fix_params`: evaluate deferred `keyof` before gating

**Summary.** The inference clamp in `src/check/infer.rs` (`fix_params`) filters ANY
substituted `Keyof` node as undecidable without evaluating it first, so a decidable
`keyof {a, b}` produced by inference is discarded:
`declare function f<T, K extends keyof T>(o: T, k: K): void; f({a:1,b:2}, "c")`
misses tsc's TS2345. Pre-existing (the pre-M28 `keyof = error` clamp was equally
vacuous; M28's `unknown` fallback is slightly sounder).

## Problem

`calls.rs` evaluates-then-gates its deferred-keyof undecidability check; `fix_params`
only substitutes-then-gates. The asymmetry drops constraint violations that become
fully concrete after inference — a known false-negative window in generic calls.

## Approach / acceptance

Apply the same evaluate-first discipline as `calls.rs`: demand-evaluate the
substituted constraint through the evaluator; gate only if a deferred node survives
evaluation. Corpus: generic calls whose inferred `K` violates `keyof T` with concrete
`T` (literal + object args, unions), plus cases that must STAY gated (free params).
Cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/infer.rs` (`fix_params` clamp), shared eval entry in
`src/check/checker/eval.rs`.

<!-- Origin: M28 review round 1 (2026-07-05), incidental pre-existing finding. -->
