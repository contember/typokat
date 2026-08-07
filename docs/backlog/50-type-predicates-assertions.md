---
id: 50
title: Type predicates (x is T) and assertion functions (asserts x)
---

# 50 — Type predicates + assertion functions

**Summary.** User-defined type guards — `function isFoo(x: unknown): x is Foo` — and
annotation-level type predicates and assertion signatures (`asserts x`, `asserts x is Foo`) are a
documented M23 deferral:
calls to them narrow nothing (safe direction, but they are the idiomatic narrowing tool
of real-world code, and `lib.d.ts` ships them — `Array.isArray`).

The pinned TS 6.0.3 ES5 readiness gate records exactly 8
`annotation-lower/type-predicate/self` incompletes owned here. The shipped full default library
exposes these records without approximating them; they remain independent checker-1.0 work.
See [`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).

## Problem

The flow-node CFG narrows on syntactic guards only. A call whose callee's return type is
a type predicate must narrow the argument's symbol in the true branch (and per tsc in the
false branch); an assertion signature narrows unconditionally in the flow after the call
statement (its false path is unreachable-by-throw).

## Approach / acceptance

Lower predicate/assertion annotations and return types onto the signature repr (identity-bearing); the
CFG condition node recognizes guard calls keyed on the argument symbol (member paths
follow `51`); assertion calls insert a narrowing flow node after the statement. Corpus
first: positive/negative branches, unions, `asserts` with and without `is`,
predicate-signature assignability; cross-check tsc 6.0.3 --strict.

## Touch points

`crates/typokat-types/src/types/repr.rs` (predicates on signatures),
`crates/typokat-check/src/check/checker/annotations/` (lowering `x is T`
/ `asserts`), `crates/typokat-check/src/check/checker/flowgraph/` and `crates/typokat-check/src/check/flow.rs` (guard-call recognition).

<!-- Origin: completion-roadmap review (2026-07-07); M23 deferral list (README known limitations). -->
