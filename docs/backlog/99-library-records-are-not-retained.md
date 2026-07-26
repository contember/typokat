---
id: 99
title: The frozen base does not retain the library's own diagnostics
blocked-by: []
---

# 99 — The frozen base does not retain the library's own diagnostics

**Summary.** Compiling the 82-file profile produces **265 diagnostics and 610 incompletes**, and then
drops them. `OwnedLibraryRuntimeState` has no record store and `FrozenLibraryBase` adds only root
names, prefixes and identity, so no library-owned outcome can be reported or inspected from a
published base. [ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md) requires those
outcomes to be "preserved exactly"; today they are *computed* exactly and then discarded. WU7's CLI
cutover has nowhere to put them. Effort M, mostly design. Blocks nothing yet, blocks WU7.

## Problem

`ledger.finish()` materializes all 875 records inside `compile_owned_injected_frontend`. On the
production path they die when that function returns. The evidence blobs that used to hash them were
a byte *projection*, not retention — and they are gone with the snapshot
([ADR-0017](../decisions/0017-compile-the-default-library-from-source.md)). Even the replay index's
`baseline_records` holds per-owner SHA-256 digests, not records.

This predates the snapshot removal; it was simply invisible while nothing consumed a base in
production. It stops being invisible the moment `src/prelude.ts` is replaced.

**The records are not user-facing errors.** They are typokat's own model gaps against a library the
real `tsc` checks clean. So the requirement is *not* "report them" — it is that they must not be
silently approximated away, and that their count and content stay measurable so a regression in the
model shows up as movement in that set. Backlog [`98`](./98-library-diagnostic-count-delta.md) is a
live example of what happens when the only witness is a digest.

## The design question

Three shapes, and the choice is a product decision, not an implementation detail:

1. **Retain the records in the base.** Honest and directly satisfies ADR-0011's wording. Costs
   memory in every process for data no user reads.
2. **Retain a structured summary** — the `(code, file, span, id)` multiset without messages — and
   recompute full text only on demand. Cheap; keeps the regression signal that `98` needs.
3. **Retain nothing in production; pin the full set in the suite.** Zero runtime cost. This is
   effectively today's behaviour made explicit, and it is what let `98` drift for 102 commits, so it
   needs the suite pin to be a code multiset rather than a hash before it is defensible.

Whichever is chosen, the CLI must not surface library-owned diagnostics as user errors, and the
choice must be recorded — ADR-0011's "preserved exactly" clause needs either satisfying or narrowly
superseding.

## Approach / acceptance

Decide the shape, then: the library's diagnostic and incomplete set is inspectable by an explicit
means; a change to the checker that adds or removes a library-owned outcome is visible as a named
`(code, site)` difference rather than a moved integer; and no library-owned diagnostic reaches
ordinary CLI output. Verify against the WU7 cutover, not in isolation.

## Touch points

`src/check/checker/library_compiler.rs` (`OwnedLibraryRuntimeState`, `compile_owned_injected_frontend`),
`src/library/base.rs`, `src/library/provider.rs`, `src/driver.rs`,
`docs/decisions/0011-freeze-pinned-default-library-base.md`.

<!-- Origin: found while cutting artifact generation off the cold path, 2026-07-26. The evidence
     blobs were hiding the absence: they proved the records were computed, never that they were kept. -->
