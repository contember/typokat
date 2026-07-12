---
id: 78
title: Generic class value aliases retain construction and class facts
blocked-by: []
---

# 78 — generic class value aliases

**Summary.** Backlog `22` now preserves class-only construction facts through
parentheses and non-generic one-step const aliases. A generic class alias remains on the
structural construct-signature path, losing both generic substitution and the original
class's abstract/constructor-accessibility facts.

## Problem

For `const Alias = GenericClass`, `new Alias<T>(...)` does not recover the originating
class `DeclId`. Public generic aliases can retain an unsubstituted `T`, producing spurious
argument/result diagnostics, while abstract/private/protected generic aliases silently
miss `TK2511`/`TK2673`/`TK2674`. Parentheses around a direct generic class are already
transparent and are covered by shipped backlog `22`; general/chained/flow aliases remain
outside this one-step const boundary.

## Approach / acceptance

**Acceptance spec (ready):** the disabled
[`tests/cases/b78_generic_class_value_aliases/`](../../tests/cases/b78_generic_class_value_aliases/)
corpus pins public explicit/inferred construction, inferred result precision, and
abstract/private/protected diagnostics against `tsc 6.0.3 --strict`.

Carry the originating generic class declaration through the admitted one-step const alias
and reuse the existing direct generic constructor substitution path without adding general
alias-flow analysis or identity-bearing class metadata to arbitrary structural constructor
types. Preserve lexical constructor accessibility and diagnostic precedence.

## Touch points

Class value alias provenance and `src/check/checker/calls.rs::infer_new`; generic class
constructor substitution and the focused disabled corpus. Stop if exact support requires a
general value-flow/alias analysis or static-side object identity redesign.

<!-- Origin: backlog 22 independent review, sprint-2026-07-12 pre-lib hardening. -->
