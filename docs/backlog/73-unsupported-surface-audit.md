---
id: 73
title: Surface-accounting emission tail
blocked-by: []
---

# 73 — Surface-accounting emission tail

**Summary.** Finish making the AST-surface accounting executable end to end: the inventory,
validators, incomplete outcome, and the wired emissions shipped in the 2026-07-10
completeness-accounting sprint; this item now owns the surfaces that are inventoried and owned
but still exit clean without a record.

## Shipped (2026-07-10 sprint — do not redo)

The systematic infrastructure this item originally demanded is live: the machine-validated
surface inventory (`tests/surface/inventory.toml`, 148+ records over 14 dispatch enums,
compile-time E0004 drift tripwires in `src/surface.rs`), the first-class incomplete outcome
(`record_incomplete` → `incomplete[<id>]` rendering → CLI exit `3`, official-suite
`OOS:unsupported` with preserved diagnostic diffs), the `incomplete[…]` conformance-marker
harness, and emissions for: expression child slots (template interpolations, computed keys,
object/array spreads, elisions, spread arguments), statements/declarations (try/catch/finally
traversal, catch params, for-of assignment targets, enum/namespace/module/export drops),
annotations (typeof, predicates, this-type, import-type, keywords, literal types, tuple members,
qualified names, type-parameter defaults), signatures (property/method computed keys), and class
members (static blocks, accessors, index signatures, computed keys, heritage type args,
implements clauses).

## Remaining scope (this item)

1. **`infer_expr` expression-shape tail — inventoried, not emitting.** The remaining `_ => None`
   expression *shapes*: `update-expression` (`x++`), `non-null` (`x!`), `optional-chain`
   (`a?.b`), `await`, `yield`, `tagged-template`, `satisfies`, `instantiation-expression`
   (`f<T>`), `import-expression`, `bigint-literal`, `regexp-literal`, `class-expression`,
   `private-field-access`, `private-in-expression`. **Granularity finding (WU3):** `x++` reaches
   `infer_expr` via the for-loop `update` slot, so an indiscriminate `_ => None` emission would
   demote essentially every for-loop in the official corpus — a low-value, high-noise flood.
   Decide the emission granularity (which shapes truly hide a nested error worth flagging)
   before wiring the tail; do not blanket-emit the whole arm. Implementation semantics for
   binary/template/spread/iteration stay with [`71`](./71-expression-inference-fn-tail.md).
2. **Binder incomplete channel.** The binder has no `record_incomplete`; its drops are accounted
   at the stmt-check layer today. Acceptable while the layers agree; revisit if a binder-only
   drop with no checker counterpart appears.
3. **Cross-surface wrapper ties.** `requires_slots` validates children only within the same
   `(role, surface)`; the `decl/class-declaration/self` ↔ `class/class-heritage/*` /
   `class/implements-clause/*` tie is by-record, not machine-enforced (WU7-E note). Extend the
   validator if wrapper claims should be machine-checked.
4. **Cosmetic:** `annotation-lower/type-query/self` never emits (redundant with
   `type-query/typeof`); prune or wire on the next inventory touch.

Acceptance: every surface above either emits its inventory identity, is flipped supported with a
fixture witness, or is explicitly re-owned; the official-suite audit for each wiring round is
aggregated by identity with zero spurious emissions.

## Touch points

`src/check/checker/expr.rs`, `src/surface.rs`, `tests/surface/`, `tests/cases/b73_surface_accounting/`,
`tooling/official-suite/scoreboard.txt` (audited re-baseline per wiring round).

<!-- Origin: post-sprint MVP-readiness audit, 2026-07-10; rescoped at completeness-accounting sprint closure (same day) — infrastructure shipped, emission tail remains. -->
