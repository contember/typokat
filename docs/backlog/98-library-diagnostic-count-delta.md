---
id: 98
title: Eight library diagnostics went missing between two stale pins
blocked-by: []
---

# 98 — Eight library diagnostics went missing between two stale pins

**Summary.** The pinned library evidence said the 82-file profile emits **273** diagnostics in
91,453 bytes. Source compilation emits **265** in **125,251** bytes. Nobody knows which eight went,
or why, because the pin was 102 commits stale when the discrepancy surfaced. Fewer records with 37 %
more bytes is the signature of a rendering change plus deduplication, not of silent suppression —
but that is an inference, not evidence, and dropped diagnostics are this project's sharpest bug
class. Effort S–M, almost all of it measurement.

## Problem

`library_compiler_separates_runtime_product_from_evidence` compared source compilation against pins
regenerated at `90ff28d`. It failed continuously from some point after that until the snapshot
removal re-pinned it to source truth on 2026-07-26:

| | pinned (`90ff28d`) | source truth (2026-07-26) |
|---|---|---|
| diagnostics, records | 273 | **265** |
| diagnostics, bytes | 91,453 | **125,251** |
| diagnostics, sha256 | `34cc5c2c…` | `79ef18a2…` |
| incompletes | 610 / 97,796 | 610 / 97,796 — **unchanged** |

Bytes per record went 335 → 473. Incompletes did not move at all, which argues against a broad
regression in what the library reaches.

The 07-24 sprint entry recorded that "**Regeneration and the pin family in the artifact module
remain open**", so the staleness was known and deferred; it was never attributed. That module was
later removed with the shipped snapshot under ADR-0017.

**What has been ruled out.** `243a878` ("report a nested contextual error once, not `2^depth`
times") was the obvious suspect, since it removed 2,008 duplicate diagnostics from user corpora with
zero distinct codes lost. It is not the cause: the count is already 265 at `ddfd649`, its immediate
parent. The change is somewhere in the remaining ~100 commits between `90ff28d` and `ddfd649`.

**The eight cannot be named from what was retained.** [ADR-0018](../decisions/0018-pin-library-owned-records-as-a-named-census.md)
built the census this item asked for, and looking backwards with it settles that much: the `90ff28d`
witness was three SHA-256 constants plus the 21 MB `canonical.snapshot` that ADR-0017 deleted, so no
readable form of those records survives anywhere. Two facts the census did surface, which narrow the
inference without proving it:

- `INCOMPLETES_IDENTITY` at `90ff28d` is **byte-identical** to today's incompletes digest. So the
  record encoding and rendering path is unchanged for incompletes; whatever grew the diagnostics blob
  from 91,453 to 125,251 bytes is specific to *diagnostic* rendering, not a shared encoding change.
- In today's census, **99 of the 265 diagnostics are exact duplicates** — 265 collapse to 149
  distinct `(code, site)` pairs, while all 610 incompletes are distinct. A dedup-shaped change moving
  the count by 8 with zero distinct pairs lost is entirely consistent with that.

## Approach / acceptance

The forward half is **done**. ADR-0018 shipped `LibraryRecordCensus` and
`tests/fixtures/library-owned-records.txt`: the pin is now a named `(code, site)` multiset, so the
next drift says what drifted, and a `-` line without a matching `+` is a dropped diagnostic by
construction. That closes step 1 and the pin clause of the original acceptance.

What is left is backwards-looking and bounded by the evidence: **either** find a route to the
`90ff28d` record text (rebuilding that commit and running an equivalent census there is the only
candidate — the snapshot is gone, but the compiler is not), **or** accept that the eight are
unattributable and record that in [`divergences.md`](../reference/divergences.md) with the two facts
above as the reason. Do not bisect on the count; if the rebuild route works, bisect on the distinct
code multiset, because the question was never "did the number change" but "did any diagnostic stop
being reported".

Acceptance: the eight are named and attributed to a commit — or the attempt is closed with the
retention gap named as the reason it is impossible, so nobody re-opens it a third time.

## Touch points

`crates/typokat-library/src/compiler.rs` (the evidence pins),
`crates/typokat-check/src/check/checker/library_compiler.rs` (`canonical_library_evidence`),
`docs/reference/divergences.md`.

<!-- Origin: surfaced by the snapshot removal, 2026-07-26, which forced the stale pins to be
     rewritten to source truth. The delta predates the removal; it is not caused by it. -->
