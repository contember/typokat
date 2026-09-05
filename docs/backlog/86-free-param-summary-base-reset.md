---
id: 86
title: Free-param summary cache discards its sealed base on any mutation
blocked-by: []
---

# 86 — Free-param summary cache discards its sealed base on any mutation

**Summary.** `FreeParamSummaryCache::align_with` resets its **base** map, not just its local delta.
The sole project-local `CheckerSession` does not freeze or fork a semantic interner, so this no
longer affects production default-library checks. The reset remains relevant to explicit layered
interner consumers and tests. Re-verify that boundary before changing it. Effort S.

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

**Current route boundary.** The 2026-07-25 finding depended on the retired shared semantic base.
Production now builds one project-local interner and never calls `Interner::freeze_as_base` or
`fork_delta`. Any fix must therefore justify itself against a remaining explicit layered-interner
consumer rather than default-library route cost.

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

`crates/typokat-types/src/types/intern/mod.rs` (`FreeParamSummaryCache::align_with`,
`DerivedGraphCache::align_with`) and
`crates/typokat-types/src/types/substitute/mod.rs` (`compute_application_summaries`).

<!-- Origin: type-store complexity hunt, 2026-07-25 (finding 3). -->
