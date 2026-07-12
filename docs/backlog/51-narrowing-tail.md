---
id: 51
title: Narrowing tail — remaining loop forms, member paths, closures
---

# 51 — Narrowing tail: `for`/`for-of`/`do-while`, member paths, closures

**Summary.** The M23 deferrals that remain after the CFG landed: flow through `for`/`for-of`/
`do-while` bodies falls back to declared types (safe), member-path narrowing (`x.a` —
narrowing is symbol-keyed today, so a discriminant check through a property doesn't
narrow that path), and closure narrowing of never-reassigned bindings. All
safe-direction, all common idioms.

## Problem

Three slices, in value order:

1. **Loop forms** — generalize the `while` edge machinery (back edge / exit / `break` /
   `continue`) to `for`, `for-of` (element typing needs iterables — a `lib.d.ts`-era
   concern; the narrowing edges don't), and `do-while`.
2. **Member-path narrowing** — re-key the narrowing environment on access paths
   (`SymbolId` + property chain) with tsc's invalidation rules (any assignment through
   the head, or an aliasable write, resets the path). The big slice, and a prerequisite
   piece of `49`'s member-access-guard half.
3. **Closure narrowing** for `const` / never-reassigned `let` per tsc.

## Approach / acceptance

One slice per sprint WU; corpus first per slice. The loop fixpoint discipline (never
memoize provisional seeds — invariants §1) applies to every new edge kind. Cross-check
tsc 6.0.3 --strict.

## Touch points

`src/check/checker/flowgraph.rs` (loop edges, path keys), `src/check/flow.rs`
(environment keying, invalidation), `for`/`for-of`/`do-while` statement checking.

<!-- Origin: completion-roadmap review (2026-07-07); M23 deferral list (README known limitations). -->
