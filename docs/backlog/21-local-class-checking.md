---
id: 21
title: Function-local classes are entirely unchecked
blocked-by: []
---

# 21 — Function-local classes are entirely unchecked

**Summary.** A class declared inside a function body gets none of the class-keyed checks —
arity (`TK2554`), abstract instantiation (`TK2511`), constructor accessibility
(`TK2673`/`TK2674`), completeness (`TK2416`/`TK2515`/`TK2654`) are all silent there.

## Problem

`class_ctors`/`ClassInfo` registration runs for top-level class declarations; a local class's
`new` resolves no `ClassInfo`, so every class-specific check silently degrades. Surfaced by both
b06/b20 adversarial reviews (probes `ab9*`, `q1`: even a plain `TS2554` inside a local class is
missed). This is a checked-scope gap, not a b06/b20 defect.

## Approach / acceptance

**No disabled fixture yet** — unlike `30`/`56`/`60`/`62`/`66`/`67`, this silent-FN family was
not included in the WU0 `sr_deferred_ledger/` corpus; its acceptance spec is written spec-first
when the item is scheduled (manifest criterion `C-local-class-checking`).

Register local class declarations through the same fill path (scoped `DeclId`s already exist for
them in the binder — verify). Acceptance: fixtures with a local class exercising `TK2554`,
`TK2511`, `TK2673`, and `TK2416` match tsc; no regression at top level.

## Touch points

Class fill entry points (`src/check/checker/classes.rs`, `decls.rs`), statement walk for
function bodies.

<!-- Origin: b06/b20 adversarial reviews (sprint 2026-07-04-class-completeness). -->
