---
id: 10
title: Mapped types ({ [K in keyof T]: … }) — M26
blocked-by: [./08-generic-constraints.md]
---

# 10 — Mapped types (`{ [K in keyof T]: … }`)

**Summary.** Type-level evaluation phase (tree-walked; bytecode VM deferred — ADR-0001). Homomorphic
mapped types with modifier and key-remapping support.

## Problem

Mapped types aren't evaluated: `{ [K in keyof T]: … }`, including modifiers and key remapping.

## Approach / acceptance

Evaluate homomorphic mapped types with `+?`/`-?`, `+readonly`/`-readonly`, and key remapping (`as`).
Acceptance: fixtures covering a plain mapping, optional/readonly modifiers (both add and remove), and
key remapping, matching tsc.

Performance/scalability acceptance is part of this milestone: mapped-type expansion must reuse the
shared evaluator memoization/work-stack from `09`, avoid repeatedly rebuilding identical key/value
substitutions, and include a stress fixture over a wide object/union key set so expansion is not
accidentally quadratic in the common homomorphic case.

## Touch points

Mapped-type evaluation in the checker; the type store (constructing the resulting object type);
`keyof` + indexed-access machinery; evaluator memoization/work-stack reuse.

<!-- Origin: dev roadmap M26 (was HANDOFF §3, the type-level VM phase). -->
