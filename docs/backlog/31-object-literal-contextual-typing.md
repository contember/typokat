---
id: 31
title: Contextual typing of object/tuple/array literals (literal-member preservation)
---

# 31 — Contextual typing of literals in declaration position

**Summary.** Object-literal members widen unconditionally at inference, so
`type Q = { x: 5 }; const q: Q = { x: 5 }` reports a false TK2322 (`{ x: 5 }` infers as
`{ x: number }`). tsc contextually preserves literal member types against a
literal-expecting target. Surfaced by the b28 warm-up (the interface-override fixture
had to switch to read-based checks); same family as the documented m18 note (tuple/
array-literal contextual typing is declaration-position only and over-strict).

## Problem

A false-positive family on trivially valid code: any literal-typed member in an
annotation (`{ kind: "a" }` discriminants, numeric enums-by-hand, `as const`-less
configs) rejects its own literal initializer. Safe direction, but loud and common —
it will pollute every future corpus that uses literal members (utility types,
discriminated shapes).

## Approach / acceptance

Target-aware literal preservation for FRESH object/array/tuple literals in checked
assignment positions (declaration annotation, argument against a known param,
return against a known return type): when the contextual member type is a literal
(or union containing one), keep the initializer's literal instead of widening.
This is the same contextual pass the M24 clamp exemption anticipates (fresh-literal
reshaping) — design them together. Corpus first; cross-check tsc 6.0.3 (incl.
`let` vs `const` widening differences and nested literals).

## Touch points

`src/check/checker/expr.rs` (`infer_object_literal`, array/tuple literal inference),
assignment/argument obligation sites (threading the contextual target), m18 notes.

<!-- Origin: b28 warm-up blocker (2026-07-05) — pre-existing, proven with no
     interface syntax involved. -->
