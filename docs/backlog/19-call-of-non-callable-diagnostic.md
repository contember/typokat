---
id: 19
title: Call of non-callable diagnostic
blocked-by: []
---

# 19 — Call of non-callable diagnostic

**Summary.** Add the `TS2349`-style "this expression is not callable" diagnostic once callability
can distinguish true non-callable values from unsupported signatures.

## Problem

F1 made object/interface call signatures callable, M32/M33 added signature shape plus overload
lists, and B41 added persistent generic method/call/construct signatures. Annotations and optional
methods that do not yet lower still exist, however. Emitting a not-callable diagnostic for every
value without represented callability would therefore create false positives: some values are
genuinely non-callable, while others only lost their call signature because it is outside
typokat's current subset. Unions need the same care.

## Approach / acceptance

Introduce a `TK2349` diagnostic when the checker can prove the callee is not callable, while staying
silent for values whose callability was dropped as out-of-subset. Acceptance should cover plain
object/non-function calls, represented overloaded/optional/rest call signatures that must not
produce a false positive, and union callability once the checker can model it soundly.

## Touch points

`crates/typokat-check/src/check/checker/calls.rs`, `src/diagnostics.rs`, `tests/cases/README.md`, and a focused
conformance corpus for non-callable calls.

<!-- Origin: WU2 of ../archive/sprint-2026-06-28-object-interface-signatures.md. -->
