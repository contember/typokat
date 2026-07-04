# archive

Shipped sprints (each carrying its `OUTCOME` header — that's the record) and the
rare backlog/spec item with standalone reference value. Items arrive here by
`git mv`; they are **not** edited afterward.

**Default is delete, not archive.** The archive is not a graveyard for everything
that ships — only what genuinely helps a future reader. The git log holds the rest.

- [`mvp-plan.md`](mvp-plan.md) — how M0–M6 (the runnable MVP) were scoped. Shipped; kept for the layer-boundary rationale.
- [`sprint-2026-06-24-impl-bugs.md`](sprint-2026-06-24-impl-bugs.md) — current-impl bugs F3–F5 (backlog 01–03) fixed; item 04 found to be a non-bug. Kept for the item-04 finding + the per-WU review catches.
- [`sprint-2026-06-28-object-interface-signatures.md`](sprint-2026-06-28-object-interface-signatures.md) — backlog 05 / F1: object & interface method/call/construct signatures shipped. Kept for the in-subset narrowing rationale, the three review-caught false negatives, and the four accepted official-suite over-reports.
- [`sprint-2026-07-04-class-completeness-ctor-accessibility.md`](sprint-2026-07-04-class-completeness-ctor-accessibility.md) — backlog 06 + 20: TK2416/TK2515/TK2654 class completeness + TK2673/TK2674 constructor accessibility. Kept for the base-keyed bivariance finding (review round 1), the tsc 6.0.3 probe tables, and the official-suite header-alignment harness fix.
- [`sprint-2026-07-04-unstructured-flow-narrowing.md`](sprint-2026-07-04-unstructured-flow-narrowing.md) — backlog 07 / M23: the flow-node CFG as the single narrowing model. Kept for the pre-pass rationale (back edges before resolution), the provisional-fixpoint memoization guard, and the review-caught destructuring-reset dropped error.
