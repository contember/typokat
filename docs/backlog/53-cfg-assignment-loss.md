---
id: 53
title: CFG drops assignments when restoring the flow cursor to a pre-state
---

# 53 — CFG assignment loss: `&&`/`||`/ternary, `switch` clauses, `while` tests, sequences

**Summary.** One root idea, four surfaces (2026-07-07 review, all probe-verified vs tsc
6.0.3): restoring `flow_cursor` to a saved pre-state is conservative against *narrowings*
but NOT against *assignments* — the pre state is narrower than reality once a branch
assigned, which is a dropped-error false negative. **HIGH.**

## Problem

- **`&&`/`||`/ternary RHS/arms** (`flowgraph.rs:379-408`): after building the RHS the
  cursor resets to `pre`, discarding Assignment nodes. `let x: string | null = null;
  x = null; b && (x = "s"); const y: null = x;` — tsc TS2322, typokat silent. Hits the
  idiomatic `cache || (cache = make())`.
- **`switch` clause bodies** (`flowgraph.rs:220-221`, fallthrough entry `:210-213`,
  clause `break` edges `:114-123`): post-switch rejoin uses `pre`; an in-clause `x = null`
  is invisible after the switch (tsc TS2322, typokat silent); fallthrough entry and
  no-loop `break` edges have the same shape.
- **`while` test assignments** (`flowgraph.rs:284-287`): condition nodes antecede the
  loop *label*, orphaning Assignment nodes created by the test — `while (x = next())`
  misses the after-loop error and FPs inside the body.
- **Sequence expressions** (`flowgraph.rs:370-371` catch-all + no `infer_expr` arm):
  `(x = "s", 0)` is neither flow-modeled nor checked; following code checks against a
  stale state.

## Approach / acceptance

Replace every pre-state restore with a join of the real branch-end cursors (`&&`:
`join(cond_false, rhs_end)`; `||`: `join(cond_true, rhs_end)`; ternary: join of arm
ends; switch: join of clause ends + correct fallthrough/break edges; while test:
antecede condition nodes with the post-test cursor — its chain still reaches the label,
preserving the fixpoint). Model sequence expressions in both the flow builder and
`infer_expr`. Corpus first (the four probe families above + regression fixtures for the
verified-clean loop fixpoint); cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/checker/flowgraph.rs` (logical/conditional/switch/while builders),
`src/check/checker/expr.rs` (sequence expressions), m23 corpus extension.

<!-- Origin: cross-cutting soundness review 2026-07-07 (flow reviewer F1-F4), leader-verified. -->
