---
id: 122
title: Complete signature candidate position and fixing policy
blocked-by: []
---

# 122 — Complete signature candidate position and fixing policy

**Summary.** Complete ordinary inference policy for candidate position, contextual parameter
consumption and compound callee return roles. Effort L; distinct from shipping bounded `as const`.

[ADR-0039](../decisions/0039-own-inference-modes-and-context-mappers-at-native-call-boundaries.md)
pulls a necessary bounded subset into the active repair: independent-input preparation and compatible
upper bounds for independently typed callback-input-only targets. General variance/priorities,
post-fix candidate exclusion, suffix scheduling/freezing and conditional substitution modeling remain
here. This scope decision is not completion of the repair or this backlog item.

## Problem

The as-const review exposed existing generic inference differences that do not require complete
source-construction compatibility to reproduce. A parameter merely appearing in a contextual
signature does not mean a source callback consumed it. Likewise, a callee returning `T`, an object
containing `T`, or an intersection/conditional involving `T` need not widen candidates identically.

Retained witnesses in `tests/cases/b44_as_const/`:

- `contextual_parameter_fixing.ts`: pre-feature and frozen M1 share surplus errors at lines
  9/11/19/22 and a surplus `TK2345` at 33, involving omitted, annotated, unrelated or unconsumed
  parameter context and suffix inference.
- `inference_candidate_positions.ts`: readonly-derived and mixed-contribution positions miss
  errors at 9/16/19 and reject valid uses at 8/15/18/21. Compound callee return roles reject valid
  uses at 28/34. The existing required error at 31 must survive any classifier change.

These are mixed fixtures, not permission to disable all their tests. Direct enum-result widening,
newly restored contextual body errors and consumed-parameter negative controls remain required by
the [archived as-const sprint](../archive/sprint-2026-08-17-as-const.md).

## Approach / acceptance

- Pin retained callee return role, original target position and actual contextual parameter
  consumption independently. Cover omitted and annotated source parameters, nested callbacks,
  unrelated binders, union/intersection/conditional returns and order-reversed contributions.
- Prepare each contribution under the shared policy before combination. Keep exact literal IDs;
  no enum-specific exception, post-call result repair or fixture-sensitive dispatch.
- Keep contextual fixing candidate-local, with selected/rejected overload and repeated-call
  controls. A return context alone must not pretend a source parameter consumed a binder.
- Coordinate structural contribution identity with [111](111-structural-inference-literal-provenance.md).
  [110](110-contextual-return-literal-inference.md) separately owns expected-return evidence and
  dependent-default priority. [120](120-semantic-constraint-construction-order.md) separately owns
  cyclic semantic construction order; do not absorb it without a demonstrated dependency.
- Cross-check strict TypeScript 6.0.3, preserve ordinary diagnostics, and run independent review,
  the randomized contextual differential corpus and the official regression gate.

## Touch points

Shared inference candidate collection/preparation, signature return-role metadata and the existing
arrow parameter producer. Reuse the same inference and coordinator owners, not a parallel pipeline.
