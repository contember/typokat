---
id: 86
title: Free-param summary cache discards its sealed base on any mutation
blocked-by: []
---

# 86 — Free-param summary cache discards its sealed base on any mutation

**Summary.** `FreeParamSummaryCache::align_with` resets its **base** map, not just its local delta, so the
first user `<T extends …>` will discard every free-param summary of the frozen 82-file library base.
Latent today, a hard cliff at the ADR-0012 library-base cutover. One-line fix, effort S.

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

**Why it is invisible right now:** the production CLI still bootstraps `crates/typokat-check/src/prelude.ts` rather than a
frozen library base, so `base` is empty and clearing it costs nothing. After the ADR-0012 cutover the
base holds the whole 82-file library, and the first user generic with a constraint throws all of it
away — forcing `compute_application_summaries` to re-walk library subgraphs for the remainder of the
run. That is precisely the work the shipped snapshot exists to avoid.

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
`crates/typokat-types/src/types/substitute/mod.rs` (`compute_application_summaries`), the library base cutover path.

<!-- Origin: type-store complexity hunt, 2026-07-25 (finding 3, latent). -->
