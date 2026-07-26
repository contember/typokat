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

The 07-24 sprint entry recorded that "**Regeneration and the pin family in `src/library/artifact.rs`
remain open**", so the staleness was known and deferred; it was never attributed.

**What has been ruled out.** `243a878` ("report a nested contextual error once, not `2^depth`
times") was the obvious suspect, since it removed 2,008 duplicate diagnostics from user corpora with
zero distinct codes lost. It is not the cause: the count is already 265 at `ddfd649`, its immediate
parent. The change is somewhere in the remaining ~100 commits between `90ff28d` and `ddfd649`.

## Approach / acceptance

Do not bisect on the count. Bisect on the **distinct code multiset**, because the question is not
"did the number change" but "did any diagnostic stop being reported".

1. Add a probe that dumps the library's own diagnostics as sorted `(code, file, line, message)`
   rather than a count and a digest. A digest tells you something moved; it never tells you what.
2. Diff that dump across `90ff28d..HEAD`. If every distinct `(code, site)` present at `90ff28d` is
   still present, the delta is deduplication and rendering — record it and close.
3. If any `(code, site)` disappeared, that is a dropped diagnostic. Find the commit, cross-check the
   site against real `tsc 6.0.3 --strict`, and treat it as a soundness regression.

Acceptance: the eight are named and attributed to a commit, and either justified in
[`divergences.md`](../reference/divergences.md) or fixed. The evidence pin becomes a code multiset,
not only a count and a hash, so the next drift says what drifted.

## Touch points

`src/library/compiler.rs` (the evidence pins), `src/check/checker/library_compiler.rs`
(`canonical_library_evidence`), `docs/reference/divergences.md`.

<!-- Origin: surfaced by the snapshot removal, 2026-07-26, which forced the stale pins to be
     rewritten to source truth. The delta predates the removal; it is not caused by it. -->
