---
id: 39
title: Optional and default parameters in signatures
---

# 39 — Optional (`b?: T`) and default (`b: T = …`) parameters

**Summary.** Optional/default parameters are not in the type model (recorded out of the
M3 subset — `src/types/repr.rs`): `function f(a: number, b?: string) {}; f(1)` raises a
spurious `TK2554 Expected 2 arguments, but got 1`, and a defaulted parameter behaves the
same (probes 2026-07-07; tsc clean on both). A false-positive family on ubiquitous
real-world signatures, and a hard blocker for `lib.d.ts` (`14`), whose declarations use
`?` everywhere.

## Problem

Function types carry a single fixed arity: there is no required-min/total-max split, so
every call omitting an optional argument over-reports, and arity messages can't render
tsc's range form (`Expected 1-2 arguments, but got 0`). Default-parameter initializers
don't make a parameter optional either. Relation-side, optionality also matters for
signature assignability (fewer/optional target params).

## Approach / acceptance

Add a required-arity/total-arity split to the function repr (identity-bearing — carried
through `substitute` and the structural hash); optional/defaulted parameters follow tsc's
ordering rules; arity checks report `TK2554` with the range wording and `TK2555` where tsc
uses it; relation rules per tsc. Interacts with rest parameters (backlog `24`) — coordinate
the repr change so it is one signature-shape migration, not two. Corpus first; cross-check
tsc 6.0.3 --strict.

## Touch points

`src/types/repr.rs` / `intern.rs` / `hash.rs` / `substitute.rs` (function repr),
`src/relate/relation.rs` (arity/param rules), `src/check/checker/calls.rs` + `decls.rs`
(lowering + arity diagnostics), `src/check/infer.rs` (candidate positions).

<!-- Origin: completion-roadmap review (2026-07-07); probe: spurious TK2554 on optional/default params. -->
