---
id: 42
title: Enums — the type side
---

# 42 — Enums (type side)

**Summary.** `enum` is not modeled at all — `enum E { A, B }` leaves `E` unresolved
(TK2304 on every use; probe 2026-07-07), so enum-using files are all noise. Architecture
§9 puts enum **type-side** semantics in the keep-correct core (union of member types;
narrowing in `switch`); only the runtime side (reverse map, IIFE, const-enum inlining)
stays sacrificed.

## Problem

Real-world TS uses enums pervasively, and the official suite gates a whole `syntax:enum`
bucket. Needed: the `E` type (union of member types), member types `E.A` (nominal
enum-literal semantics per tsc, including the number-assignability quirks — pin the exact
tsc 6.0.3 rules in the spec first), the value side (`E.A` member access), string/number
members, `const enum` treated type-side-only. This item also owns enum/function `TK2567` diagnostic
sites and the exact three-way `enum`+`namespace`+`function` legality and recovery matrix. Until it
lands, backlog `43` owns namespace placement `TK2434` and must keep the function+namespace receiver
non-permissive without guessing the enum surface.

## Approach / acceptance

Bind `enum` into both value and type spaces (the multi-slot symbol is built for this);
intern enum/member types with nominal identity; `switch`/equality narrowing over members
reuses the discriminated-union machinery. Specify enum/function `TK2567` ownership and all legal/
illegal enum+function+namespace declaration orders with exact typed recovery. Corpus first;
cross-check tsc 6.0.3 --strict;
the official-suite `syntax:enum` gate opens after landing.

## Touch points

`src/binder/` (enum declarations, multi-slot), `src/types/repr.rs` (enum/member types),
`src/check/checker/` (member access, expression typing), `src/check/flow.rs` (narrowing),
official-suite `OUT_OF_SCOPE_SYNTAX`.

<!-- Origin: completion-roadmap review (2026-07-07); architecture §9 keep-column. -->
