---
id: 51
title: Narrowing tail — remaining loop forms, member paths, closures
---

# 51 — Narrowing tail: `for`/`for-of`/`do-while`, member paths, closures

**Summary.** The M23 deferrals that remain after the CFG landed: flow through `for`/`for-of`/
`do-while` bodies falls back to declared types (safe), member-path narrowing (`x.a` —
narrowing is symbol-keyed today, so a discriminant check through a property doesn't
narrow that path), correlated narrowing across dependent destructured bindings, closure
narrowing of never-reassigned bindings, assignment-target evaluation order, and semantic assignment
results through loops. Assignment evaluation and inferred-copy transfer can drop errors; a reset
to a declared surface is not a blanket soundness guarantee.

## Problem

Seven slices, in value order:

1. **Loop forms** — generalize the `while` edge machinery (back edge / exit / `break` /
   `continue`) to `for`, `for-of` (element typing needs iterables — a `lib.d.ts`-era
   concern; the narrowing edges don't), and `do-while`. The current official witnesses are
   `controlFlowDoWhileStatement.ts` line 34 and `controlFlowForStatement.ts` lines 10 and 17.
2. **Member-path narrowing** — re-key the narrowing environment on access paths
   (`SymbolId` + property chain) with tsc's invalidation rules (any assignment through
   the head, or an aliasable write, resets the path). The big slice, and a prerequisite
   piece of `49`'s member-access-guard half.
3. **Dependent destructured bindings** — retain the source union and constituent-to-leaf
   projections for a flat binding group. A guard on one leaf filters the source constituents and
   re-projects its siblings; assignment to any participant invalidates the correlation. The
   official `dependentDestructuredVariables.ts` witness covers ordinary, generic, optional, and
   `T | T[]` sibling correlations.
4. **Closure narrowing** for `const` / never-reassigned `let` per tsc.
5. **Remaining string/number truthiness split** — `NarrowOp::Truthy` precisely splits boolean,
   but keeps broad `string` and `number` whole in both branches. Since backlog `101` that
   imprecision is also visible in a *value*: `a && b` is `string | b` where tsc says `"" | b`.
   One splitter, so one fix (see the `narrowing/logical-value-falsy-split` entry in
   `../reference/divergences.md`).
6. **Assignment-target evaluation order** — evaluate target-side flow effects before checking the
   RHS. The disabled `sr_deferred_ledger/b51_assignment_target_evaluation_order.ts` witness currently
   drops tsc's `TS2339` because the RHS sees the stale pre-target narrow type. The inverse official
   witness, `controlFlowAssignmentExpression.ts` stripped line 9, emits a surplus `TK2339` for the
    same ordering error.
7. **Semantic assignment products and loop discharge** — retain the actual checked source and
   contextual expression result separately from declared-target transfer. The retained
   `b44_as_const/assignment_contextual_result.ts`, `loop_semantic_dependencies.ts`, and
   `loop_query_local_discharge.ts` witnesses fail before the feature as well as on frozen M1.
   They remain future acceptance under this owner, not prerequisites for bounded `as const`.
   [ADR-0032](../decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md)
   specifies effect adoption and query-local loop discharge; only actual future initializer demand
   belongs jointly to [76](76-lazy-value-type-resolution.md).

The same pre-existing transfer gaps appear in `b44_as_const/flow_target_reduction.ts` at lines
6/35/40 and `assignment_semantic_reduction.ts` at lines 26/33/40. These mixed fixtures also contain
required feature assertions: deferring the old failures must not disable their newly recovered
errors or waive introduced invalid-write false positives. Exact-source comparisons and the retained
feature boundary are recorded in the [as-const sprint](../archive/sprint-2026-08-17-as-const.md).

## Approach / acceptance

One slice per sprint WU; corpus first per slice. The loop fixpoint discipline (never
memoize provisional seeds — invariants §1) applies to every new edge kind. Cross-check
tsc 6.0.3 --strict. The assignment-order slice must emit `TK2339` for the disabled witness, remove
the official line-9 surplus, and keep the no-target-side-effect control clean. The loop-form slice
must remove only the three named official `TK2339` records. Changing complex-RHS transfer belongs
to the separate semantic-product slice, not an incidental loop-edge cleanup. Its acceptance must
cover exact-copy errors, nested assignments, loop-local demand and exactly-once child diagnostics;
never memoize a provisional loop result or substitute exhaustion for successful recovery.

## Touch points

`crates/typokat-check/src/check/checker/flowgraph/` (loop edges, path keys), `crates/typokat-check/src/check/flow.rs`
(environment keying, invalidation), assignment-expression checking, and `for`/`for-of`/`do-while`
statement checking.

<!-- Origin: completion-roadmap review (2026-07-07); M23 deferral list (README known limitations). -->
