---
id: 110
title: Contextual return inference widens singleton literals
blocked-by: []
---

# 110 — Contextual return inference widens singleton literals

**Summary.** Return-context inference widens a singleton literal candidate and overrides a
dependent type-parameter default, causing a safe `TK2322` over-report. Effort M.

## Problem

Pinned TypeScript 6.0.3 accepts this independent baseline:

```ts
declare function generalReturnContext<T, U = T>(value: T): [T, U];
const result: [1, 1] = generalReturnContext(1);
```

typokat infers `[number, number]` and reports `TK2322`. The same failure occurs without an optional
argument, so the optional-parameter sprint did not own it. Call-site fixing first widens the
ordinary singleton literal. `apply_return_context_candidates` then supplies contextual evidence for
the dependent parameter instead of preserving `U = T`; changing optional fallback pruning cannot
fix that general ordering rule.

## Approach / acceptance

Specify return-context candidate priority across ordinary evidence, dependent defaults,
constraints, and contextual return evidence. Keep uncontextual calls widened where TypeScript
widens them, but preserve the exact singleton when the return context proves it. Cover direct and
dependent parameters, reversed parameter order, constrained binders, explicit type arguments,
overloads, and a wrong contextual target. Acceptance matches pinned tsc 6.0.3 without changing
optional fallback semantics or publishing context-dependent inference state.

## Touch points

`crates/typokat-check/src/check/infer/mod.rs` (`apply_return_context_candidates`, call-site
candidate fixing), inference tests, a spec-first conformance fixture, and the mandatory randomized
differential gate.

<!-- Origin: optional-parameter sprint WU1 review, 2026-08-11. -->
