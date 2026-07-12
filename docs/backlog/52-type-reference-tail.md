---
id: 52
title: Type-reference tail — TS2749, TS2314/TS2315, TK2558
---

# 52 — Type-reference diagnostics tail

**Summary.** The M22 deferral list for type positions, verified still open (probes
2026-07-07): a value used as a type is silent (`type X = c` — tsc: TS2749), a generic
used with the wrong type-argument count is silent (`Box<number, string>` — tsc: TS2314),
and bare `Box` relates against the *uninstantiated* `{ v: T }` (a confusing FP instead of
tsc's TS2314). Type args on a type parameter (TS2315) and `TK2558` (wrong count on
explicit call-site type arguments) belong to the same family. `typeof value` type queries must
also resolve the value-side type without accepting a value as an ordinary type reference.
Qualified names `A.B` (TS2503) are covered by `43`.

## Problem

Type-reference and value-type-query resolution accept shapes they should reject, in both directions: silence
(FN — value-as-type, extra type args ignored) and confusion (bare generic checked against
open type params). Small, self-contained, and high-noise-reduction once ambient types
appear (the shipped minimal prelude and `14` make misuse of generic names much more common).

## Approach / acceptance

Validate every type-reference and `typeof` query against the target symbol: value-only symbol → TK2749
(with the `typeof` hint), generic arity mismatch → TK2314, type args on a type param →
TS2315, explicit call-site type-arg count → TK2558. Corpus first; cross-check tsc 6.0.3
--strict.

## Touch points

`src/check/checker/annotations.rs` (type-reference and `typeof` query lowering), `src/check/checker/calls.rs`
(explicit type arguments), `src/diagnostics.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); M22 deferral list + probes. -->
