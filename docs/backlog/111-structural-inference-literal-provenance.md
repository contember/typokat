---
id: 111
title: Structural inference loses literal widenability provenance
blocked-by: []
---

# 111 — Structural inference loses literal widenability provenance

**Summary.** Structurally derived candidates collapse raw and explicitly fixed literal members into
the same union, so fresh object/array inference can silently retain `1 | 2` where TypeScript widens
to `number`. This is a soundness bug. Effort L.

## Problem

Pinned TypeScript 6.0.3 reports `TS2322` on the final assignment; typokat accepts it:

```ts
declare function pairedBaseline<T>(value: { left: T; right: T }): T;
const value = pairedBaseline({ left: 1, right: 2 });
const literalValue: 1 | 2 = value;
```

The structural inference walk collapses both member contributions to one union candidate before
fixing. The current candidate record retains only the outer source and a `fresh` bit. That shape
cannot distinguish raw literals, which must widen, from typed identifiers or asserted literal
members, which TypeScript preserves. Recursively widening every union under outer `fresh` would
reject valid typed/asserted controls and is not an acceptable fix.

## Approach / acceptance

Retain candidate-level literal widenability provenance through structural descent and union
formation, or keep occurrence contributions separate until fixing. Pin fresh object and array
sources that widen, plus otherwise identical typed identifiers, literal assertions, constrained
parameters, and non-fresh union/object sources that remain exact. Acceptance restores the missing
diagnostic above without weakening the preserved controls, query-order stability, or optional
fallback inference.

## Touch points

`crates/typokat-check/src/check/infer/mod.rs` (`CallSiteCandidate`, structural candidate
collection, `fix_call_site_candidates_for_param`), expression freshness handoff in
`crates/typokat-check/src/check/checker/calls.rs`, inference tests, a spec-first conformance fixture,
and the mandatory randomized differential gate.

<!-- Origin: optional-parameter sprint WU1 adversarial review, 2026-08-11. -->
