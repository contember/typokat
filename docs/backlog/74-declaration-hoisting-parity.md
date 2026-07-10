---
id: 74
title: Function and var declaration hoisting parity
blocked-by: []
---

# 74 — Function and `var` declaration hoisting parity

**Summary.** Give function declarations and `var` their TypeScript visibility rules so a call
before a local function declaration is checked (silent-FN fix) and block-contained `var` names are
visible from their function scope (safe-FP fix).

## Problem

Declaration order currently changes checker coverage inside a function. Calling a local function
before its declaration misses call/overload diagnostics because the declaration is not available
when the call is checked. Conversely, `var` is treated like a block-scoped declaration, so a
reference outside its block or switch clause gets a spurious `TK2304` even though TypeScript
hoists the binding to the containing function.

These are one binder predeclaration problem with opposite symptoms. The function case is a release-
blocking dropped-error family; the `var` case is a documented safe-direction parity gap. Definite
assignment and temporal-dead-zone diagnostics remain owned by backlog [`47`](./47-definite-assignment.md).

## Approach / acceptance

Add a function-scope predeclaration phase using the existing scope graph and symbol slots:

- collect local function declaration groups before checking executable statements, preserving
  M33 overload order and hiding implementation signatures from calls;
- bind every `var` name in the nearest function/module scope while leaving initializer evaluation
  and flow assignment at the original statement position;
- keep `let`/`const` block scoped and do not conflate name visibility with TK2448/TK2454 definite-
  assignment rules.

Corpus first, cross-checked with `tsc 6.0.3 --strict`: forward calls to ordinary, generic, and
overloaded local functions must receive the same arity/argument/no-overload diagnostics as calls
after the declaration; recursive/mutually-referential declarations remain stable. `var` in blocks,
loops, and switch clauses resolves throughout the containing function, while its pre-initializer
value/flow behavior stays non-permissive. An independent review probes duplicate declarations,
shadowing, nested functions, and reordered statements without changing module boundaries.

## Touch points

`src/binder/`, function-body declaration grouping, `src/check/checker/calls.rs`, flow/definite-
assignment integration, conformance fixtures, and `docs/reference/divergences.md`.

<!-- Origin: sprint-2026-07-10 WU4-B byproduct plus post-sprint owner audit. -->
