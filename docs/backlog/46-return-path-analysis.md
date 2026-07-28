---
id: 46
title: Return-path analysis (TK2355/TK2366/TK2378/TK7030)
---

# 46 — Return-path analysis

**Summary.** A function declared `(): number` whose body never returns a value passes
silently (probe 2026-07-07; tsc: TS2355). The M23 flow-node CFG already models
reachability and exits — the return-path diagnostics were never wired onto it.

## Problem

Missing: TK2355 (declared non-void/any function returns no value at all), TK2366 (lacks
ending return statement — some paths return), TK2378 (a `get` accessor must return),
TK7030 (not all code paths return — the `noImplicitReturns` flavor; typokat has no flag
surface, so decide the always-strict stance and document it). tsc's split between
2355/2366/7030 per shape is subtle — pin it against tsc in the spec first.

The same return-model gap affects inference: a bare `return;` currently contributes no candidate,
so it is not folded as `undefined` into the inferred return union. Mixed `return value;` / bare
`return;` paths must infer the tsc-equivalent union and let downstream assignability report it.

## Approach / acceptance

Classify each body's CFG exit node: fall-through-reachable exits vs return coverage,
`never`-returning and throw-only bodies stay clean. Corpus first (throw-only, infinite
loops, branch coverage, accessors, bare-return-only and value-plus-bare-return inference);
cross-check tsc 6.0.3 --strict. Generators/async are
out of the current model — document, don't guess.

## Touch points

`crates/typokat-check/src/check/checker/flowgraph/` (exit reachability), function-body checking in
`crates/typokat-check/src/check/checker/`, `crates/typokat-diagnostics/src/diagnostics/mod.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); probe: (): number with no return is silent. -->
