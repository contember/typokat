---
id: 44
title: satisfies and as-const semantics
---

# 44 — `satisfies` + `as const`

**Summary.** The `satisfies` half is implemented: it checks one-way assignability while retaining
the source type, and its official-suite syntax gate is open. The remaining `as const` half still
produces no readonly literal types (`o.a = 2` after `as const` passes; tsc: TS2540), so this backlog
item remains open.

The completed `satisfies` work is recorded in
[`sprint-2026-08-17-satisfies.md`](../archive/sprint-2026-08-17-satisfies.md). The ordered follow-up
is [`sprint-2026-08-17-as-const.md`](../sprints/sprint-2026-08-17-as-const.md).

## Problem

`as const` must produce the const-asserted type: literals retained (no widening), members
`readonly`, and array literals as readonly tuples. Sibling of backlog `33` (`as`-cast typing) —
same expression-typing path; keep the divergence ledger coherent across the two.

## Approach / acceptance

Corpus first: `as const` object/array/nested shapes, write-through-readonly `TK2540`, and
interaction with M30 contextual typing and literal widening. Cross-check tsc 6.0.3 `--strict` and
open the remaining `syntax:as-const` official-suite gate.

Delivery targets the bounded literal-owned feature, not complete TypeScript compatibility. General
cyclic-constraint/source-construction order is tracked by
[120](120-semantic-constraint-construction-order.md). Existing unrelated gaps may remain explicitly
owned; regressions introduced by this feature, including dropped ordinary diagnostics, may not be
waived under that deferral. The active sprint records the exact fixture/gate dispositions.
The user explicitly included the bounded source-construction dependency in
[ADR-0040](../decisions/0040-complete-registered-callables-at-source-demand-boundaries.md), including
constructed returns and early callable relations. Its 37-row execution proof is required before
closure; accepting the design does not accept a model-only implementation or all of backlog 120.

[ADR-0036](../decisions/0036-bound-contextual-class-publication-migration.md) retains bounded
literal-owned `as const` in instance fields but excludes general contextual class-initializer
preparation from this milestone. [123](123-contextual-class-initializer-publication.md) owns the
pre-existing contextual class gap; it remains release-blocking and is not proof of a correct legacy
path. No blanket new unsupported admission or unreviewed class implementation is accepted.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs` (both operators), the freshness/excess path,
`crates/typokat-types/src/types/repr.rs` (readonly array/tuple forms — coordinate with backlog `24`'s tuple
work), official-suite `OUT_OF_SCOPE_SYNTAX`.

<!-- Origin: completion-roadmap review (2026-07-07); probes: satisfies mismatch + as-const write both silent. -->
