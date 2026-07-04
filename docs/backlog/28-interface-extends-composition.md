---
id: 28
title: Interface extends composition (inherited members missing from the interface type)
---

# 28 — Interface `extends` composition

**Summary.** Members inherited through interface `extends` are missing from the
interface's own type: with `interface Base { x: number }` and
`interface Derived extends Base { y: string }`, `const bad: Derived = { y: "s" }`
passes silently (tsc: TS2741 missing `x`) and `keyof Derived` is `"y"` only. Found
by the M26 adversarial review's attribution probes (`Eattr_plain.ts` — zero mapped
types, fails identically), so it is pre-existing, independent of mapped types.

## Problem

A silent false-negative family on a core language feature: any obligation flowing
through a derived interface misses every inherited member (assignability, keyof,
mapped types over derived interfaces, member access presumably works via other
paths — verify). Classes compose correctly (instance types carry the extends
chain); interfaces do not.

## Approach / acceptance

Fold the `extends` chain into the interface's object type at fill time (the class
machinery already walks heritage — mirror it), including multiple bases and
override-by-name (own wins). Corpus first: assignability both directions, keyof,
mapped-over-derived, TK2741 spans, deep chains, diamond shapes; cross-check tsc
6.0.3. Watch interplay with `f1_object_interface_*` corpora.

## Touch points

`src/check/checker/decls.rs` (interface fill), possibly the binder's heritage
resolution; `tests/cases/` new corpus dir.

<!-- Origin: M26 adversarial review attribution probe (2026-07-04). -->
