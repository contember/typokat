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
Qualified namespace names `A.B` are shipped; this item retains the distinct value/type misuse and
generic-arity diagnostics around their endpoints.

## Problem

Type-reference and value-type-query resolution accept shapes they should reject, in both directions: silence
(FN — value-as-type, extra type args ignored) and confusion (bare generic checked against
open type params). Small, self-contained, and high-noise-reduction once ambient types
appear (the shipped minimal prelude and `14` make misuse of generic names much more common).

Type queries also form an atomic publication boundary. In the official
`subtypingWithCallSignaturesA.ts` witness, the return `typeof cb` is explicitly incomplete, so
typokat withholds the whole signature and drops tsc's call-site `TS2345` at harness line 2. That is
sounder than publishing a partial callable, but support for `typeof` must restore the downstream
diagnostic as well as the queried value type.

## Approach / acceptance

Validate every type-reference and `typeof` query against the target symbol: value-only symbol → TK2749
(with the `typeof` hint), generic arity mismatch → TK2314, type args on a type param →
TS2315, explicit call-site type-arg count → TK2558. Corpus first; cross-check tsc 6.0.3
--strict.

Pin `subtypingWithCallSignaturesA.ts` as the atomic-unavailable control: before support it must
retain `annotation-lower/type-query/typeof` and never publish a partial signature; after support it
must resolve `typeof cb` and recover the exact `TK2345` call rejection.

## Touch points

`crates/typokat-check/src/check/checker/annotations/` (type-reference and `typeof` query
lowering), `crates/typokat-check/src/check/checker/calls.rs`
(explicit type arguments), `crates/typokat-diagnostics/src/diagnostics/mod.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); M22 deferral list + probes. -->
