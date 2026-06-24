---
id: 10
title: Mapped types ({ [K in keyof T]: … }) — M26
blocked-by: [./08-generic-constraints.md]
---

# 10 — Mapped types (`{ [K in keyof T]: … }`)

**Summary.** Type-level VM phase (tree-walked first). Homomorphic mapped types with modifier and
key-remapping support.

## Problem

Mapped types aren't evaluated: `{ [K in keyof T]: … }`, including modifiers and key remapping.

## Approach / acceptance

Evaluate homomorphic mapped types with `+?`/`-?`, `+readonly`/`-readonly`, and key remapping (`as`).
Acceptance: fixtures covering a plain mapping, optional/readonly modifiers (both add and remove), and
key remapping, matching tsc.

## Touch points

Mapped-type evaluation in the checker; the type store (constructing the resulting object type);
`keyof` + indexed-access machinery.

<!-- Origin: dev roadmap M26 (was HANDOFF §3, the type-level VM phase). -->
