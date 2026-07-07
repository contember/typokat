---
id: 64
title: infer binders under readonly array types raise spurious TK2304
---

# 64 — `readonly (infer U)[]` binder gap

**Summary.** An `infer` binder nested under a `readonly` array type node is not
collected, so `type RElem<T> = T extends readonly (infer U)[] ? U : never` raises a
spurious `TK2304 Cannot find name 'U'` at the alias declaration, degrades the alias to
the error type, and thereby **silently accepts** downstream misuse: `type R1 =
RElem<readonly string[]>; const a: R1 = 5` reports nothing where tsc reports TS2322
(probe 2026-07-07, leader-verified). The mutable form `(infer U)[]` collects the binder
correctly (line-for-line control errors as expected).

## Problem

The binder's `infer`-collection walk descends into `(infer U)[]` (`TSArrayType`) but not
into the `readonly (infer U)[]` (`TSTypeOperator` with the `readonly` operator wrapping
the array). A loud TK2304 at the declaration plus a masked silent FN on every use of the
degraded alias — the same shape as backlog `32`'s degrade-to-permissive family, in a
different node. Pre-existing (independent of the b57 Tuple↔Array inference work that
surfaced it — reproduces with a pure `readonly string[]` array source, no cross-kind
pairing involved).

## Approach / acceptance

Descend through the `readonly` type-operator node when collecting `infer` binders (and
audit the other `TSTypeOperator` forms — `keyof`, `unique` — for the same
non-traversal). Corpus: `readonly (infer U)[]` extraction, readonly tuple `readonly
[infer A, infer B]`, nested `readonly (infer U)[][]`; cross-check tsc 6.0.3 --strict.
The b57 corpus deliberately avoids readonly forms; this item lifts that restriction.

## Touch points

The `infer`-binder collection walk (`src/check/checker/annotations.rs` or the binder
path that gathers conditional `infer` names), m25 corpus.

<!-- Origin: b57 (WU5) adversarial review byproduct, 2026-07-07, leader-verified pre-existing. -->
