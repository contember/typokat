---
id: 88
title: Symbol declaration lists are fully re-sorted on every attach
blocked-by: []
---

# 88 — Symbol declaration lists are fully re-sorted on every attach

**Summary.** `attach_symbol_declaration` re-sorts a symbol's entire declaration list on every single
attach, so a heavily merged symbol costs O(k² log k). Invisible on the current bench corpora, but it
is exactly the shape of full `lib.d.ts` — the next roadmap item. Effort S.

## Problem

`src/binder/bind.rs:958` checks `row.declarations.contains(&declaration)` — itself O(k) — and then runs
a **full `row.declarations.sort_by_key(...)`** on every attach. The key closure is not cheap either: it
performs a `module_sources` (or `library_module_ordinals`) hash lookup plus a `declarations.get` on each
comparison, so the constant is large on top of the wrong complexity.

Measured with one interface reopened N times:

| N | binding phase |
|---|---|
| 1,000 | 6.87 ms |
| 4,000 | **99.44 ms** |

Log-log slope **1.94**, and at N=4,000 this single function is 84% of the binding phase. `declare function`
overload sets behave identically (99.79 ms).

It measures only 5.4 ms on the 6,249-file benchmark because that generated corpus merges almost nothing.
Real `lib.d.ts` does the opposite: `interface Array`, `interface Window`, `interface String` and the large
`declare function` overload sets are precisely large-k merge groups. Backlog `14` (full `lib.d.ts` loading)
will walk straight into this, so it should be fixed before that lands rather than diagnosed again from a
profile afterwards.

Two adjacent O(k²) guards in the same merge path, cheap to fix in the same change:
`push_placement`'s duplicate guard `entries.iter().any(...)` (`src/binder/namespace.rs:6031`; measured
5.52 ms at N=4,000, slope 1.84) and the same pattern in the namespace reopen path (7.95 ms).

## Approach / acceptance

The list is already almost sorted — declarations arrive in source order within a file — so replace the
full sort with a single binary-search insert, or mark the row dirty and sort once in `finish()`. Precompute
the sort key per `DeclId` instead of re-hashing inside the comparator. Replace the `contains` and
`any` guards with a per-group `FxHashSet<DeclId>`, or drop them if the walk already guarantees one push
per site (verify before removing — a duplicate attach would corrupt merge order).

Acceptance: a counter-based guard asserting attach work grows with the number of declarations, not with
declarations × group size, at two group sizes; declaration **order must stay byte-identical**, since merge
order determines which declaration wins and therefore diagnostic text — pin it on a merged interface, a
reopened namespace, and an overload set.

## Touch points

`src/binder/bind.rs` (`attach_symbol_declaration`), `src/binder/namespace.rs` (`push_placement` guard,
namespace reopen path).

<!-- Origin: binder complexity hunt, 2026-07-25 (findings 4 and 6). -->
