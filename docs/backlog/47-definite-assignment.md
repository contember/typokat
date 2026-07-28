---
id: 47
title: Definite assignment (TK2454/TK2448/TK2564)
---

# 47 — Definite assignment / use before assignment

**Summary.** `let x: number; const y: number = x;` passes silently (probe 2026-07-07;
tsc --strict: TS2454). The CFG has assignment nodes and a memoized backward walk —
definite assignment is the same machinery pointed at "was assigned on every path" instead
of "what type".

## Problem

Missing Tier A codes: TK2454 (variable used before being assigned), TK2448 (block-scoped
variable used before its declaration — TDZ), TK2564 (property has no initializer and is
not definitely assigned in the constructor). Their absence is a silent-FN family in
strict-mode real code.

The adjacent `undefined` assignment-target over-report remains owned here: typokat reports TK2304
where tsc gives TS2539. Function/`var` declaration visibility shipped in the
[`2026-07-11 hoisting sprint`](../archive/sprint-2026-07-11-declaration-hoisting-parity.md);
definite-assignment timing stays here.

## Approach / acceptance

Reuse the flow-node walk: a reference is "assigned" iff every path from function entry
assigns first. Loops follow the M23 fixpoint discipline — definite-assignment must not
durably cache provisional seeds (same trap as narrowing; invariants §1). TK2564 walks the
constructor CFG for `this.x` assignments. Definite-assignment assertions (`x!: T`) must be
honored. Corpus first (branches, loops, closures — tsc gives up inside closures, match
it); cross-check tsc 6.0.3 --strict.

## Touch points

`crates/typokat-check/src/check/checker/flowgraph.rs` + `flow.rs` (assigned-ness walk), constructor checking
(`crates/typokat-check/src/check/checker/classes.rs`), `src/diagnostics.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); probe: use-before-assign is silent. -->
