---
id: 86
title: Free-param summary cache discards its sealed base on any mutation
blocked-by: []
---

# 86 — Free-param summary cache discards its sealed base on any mutation

**Summary.** `FreeParamSummaryCache::align_with` resets its **base** map, not just its local delta, so the
first user `<T extends …>` can discard free-param summaries retained in a frozen library base. The
source-backed production cutover has shipped. Route impact now requires fresh-HEAD verification
because standalone complete-source and persistent shared-provider routes have different lifecycles.
The reset itself remains live. Effort S.

## Problem

`crates/typokat-types/src/types/intern/mod.rs:147-154` resets `self.base = Arc::new(FxHashMap::default())` whenever the
semantic graph mutates. Its sibling `DerivedGraphCache::align_with` (`crates/typokat-types/src/types/intern/mod.rs:98-104`)
clears only `local`, which is the correct behaviour: `base` is populated exclusively by
`freeze_as_base` over an **immutable sealed prefix**, so a mutation of the local delta cannot
invalidate it.

The mutations that trigger it are ordinary: `set_type_param_constraint` (`crates/typokat-types/src/types/store.rs:592`) —
once per constrained generic binder — and `fill_reserved_type_batch`
(`crates/typokat-types/src/types/intern/mod.rs:682`) —
once per interface SCC.

**Current route question.** The 2026-07-25 finding predated the source-backed production cutover.
The standalone CLI now uses complete-source compilation, while persistent consumers may acquire the
shared provider base. Re-verify at fresh HEAD whether each route presents a non-empty summary base
when the mutation occurs and measure any resulting `compute_application_summaries` re-walk. Do not
infer current route cost from the retired prelude or snapshot lifecycle.

## Approach / acceptance

Delete the `base` reset; make the two `align_with` bodies identical. Then prove the invariant rather
than assuming it: `base` is only ever written by `freeze_as_base` over a sealed prefix, so no local
mutation can invalidate an entry in it.

Acceptance: a test that freezes a base, performs each mutation kind (`set_type_param_constraint`,
`fill_reserved_type_batch`), and asserts the base summaries survive — plus a counter showing
`compute_application_summaries` does not re-walk frozen subgraphs after a user generic is introduced.
Existing summary semantics must be byte-identical; this is a cache-retention change, not a semantic
one.

## Touch points

`crates/typokat-types/src/types/intern/mod.rs` (`FreeParamSummaryCache::align_with`, `DerivedGraphCache::align_with`),
`crates/typokat-types/src/types/substitute/mod.rs` (`compute_application_summaries`), standalone
complete-source and persistent shared-provider routes.

<!-- Origin: type-store complexity hunt, 2026-07-25 (finding 3). -->
