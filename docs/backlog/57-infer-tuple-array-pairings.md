---
id: 57
title: Inference walker lacks Tuple/Array cross-kind pairings (wrong infer results)
---

# 57 — Inference: missing Tuple↔Array pairings

**Summary.** The candidate walker (`src/check/infer.rs:442-472`) only pairs like kinds
(Object/Object, Function/Function, Union/Union, Array/Array, Tuple/Tuple). Two
probe-verified consequences (2026-07-07, leader-verified):

- **Tuple source vs Array pattern — silent FN, wrong evaluation.** `type Elem<T> =
  T extends (infer U)[] ? U : never; type E1 = Elem<[1, 2]>; const e1: E1 = 5;` —
  typokat binds `U = unknown` (no candidate; `run_extends_test` fixes the unbound-infer
  fallback, and `[1,2] <: unknown[]` still holds → true branch), so `E1` accepts
  everything. tsc: `U = 1 | 2`, TS2322. The `T extends (infer U)[]` idiom over tuples is
  ubiquitous. **HIGH.**
- **Array source vs Tuple parameter — spurious FP.** `function h<T>(t: [T, T]): T` with
  `h([1, 2])` infers `T = unknown` → TK2322 on correct code (tsc clean): call-argument
  inference runs on the raw argument type (`number[]` for a fresh `[1,2]` literal,
  `calls.rs:277-289`) and the M30 contextual retype happens only after instantiation.

## Approach / acceptance

Add cross-kind arms mirroring the relation engine's tuple→array covariance: Tuple source
vs Array target infers from each element (union); Tuple-vs-Tuple positional (exists) plus
Array-source-vs-Tuple-target either via contextual tuple typing of fresh literals before
inference or a positional arm per tsc's behavior (pin against tsc in the spec — the two
tools must agree on which calls are legal). Extend the m25 `infer` corpus and the m10
inference corpus; cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/infer.rs` (pairing arms), `src/check/checker/calls.rs` (inference input vs
M30 contextual retype ordering), m10/m25 corpora.

<!-- Origin: cross-cutting soundness review 2026-07-07 (evaluator #3 + modules #5), leader-verified. -->
