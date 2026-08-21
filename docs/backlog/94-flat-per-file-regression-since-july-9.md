---
id: 94
title: A flat 3x per-file regression sits under the modules exponent
blocked-by: []
---

# 94 — A flat 3× per-file regression sits under the modules exponent

**Summary.** On 2026-07-26, `modules-100000` cost **0.997 s / 372 MB** at the then-HEAD endpoint
`a0a5a6c` against **0.30 s / 154 MB** at `f065e89` (2026-07-09) on the identical corpus. The dated
bisect found four eager per-declaration substrate steps. Merge-key interning and participant sharing
shipped in `727fcf4`; lazy completion storage shipped in `c3c3549`. The evidence predates the
source-backed default-library cutover, so remeasure the current baseline before ranking the remaining
work. The recorded 0.35 s / 200 MB target and historical evidence remain. Effort L.

## Problem

At the measured 2026-07-26 endpoint, `c8fc029` removed the last superlinear term (exponent 2.54 →
1.07). What remained there was a large flat constant:

| | median | peak RSS | vs tsgo |
|---|---|---|---|
| `f065e89` (2026-07-09) | **0.30 s** | **154 MB** | tsgo 0.3741 s — **typokat won** |
| `a0a5a6c` (2026-07-26 endpoint) | 0.997 s | 372 MB | tsgo ~0.30 s — 3.3× slower |

Both endpoints measured on the same box on 2026-07-26 with a corpus proven md5-identical
(`696772268a1328475b0bd8895c364b73`) throughout; `tooling/bench/typobench.py` is byte-identical at
both commits and its generator has no RNG.

### The steps

Bisected on **peak RSS**, which is near-deterministic, using `modules-20000` (1,249 files) — it
carries the full signature at 1/5 the cost.

| # | commit | date | ΔRSS @1,249 files | per-file | verdict |
|---|---|---|---|---|---|
| 1 | `6c82ead` | 07-10 | none | +14 % time | deliberate soundness fix — keep |
| 2 | `a7923b6` | 07-14 | +13.8 MB | 11.4 KB/file | deliberate intent, accidental representation |
| 3 | `b6ecfa4` | 07-15 | +8.4 MB | } 15.1 KB/file | accidental |
| 4 | `fe61867` | 07-15 | +11.1 MB | } combined | accidental |
| 5 | diffuse 07-16 → 2026-07-26 endpoint | | +14.2 MB, no step >4.4 MB | ~7.7 KB/file | unattributed |

Per-file constants were confirmed at 312 / 1,249 / 2,499 files (10.9–11.9 KB/file for step 2;
25.2–27.2 KB/file cumulative through step 4) and extrapolate to 213 MB at 6,249 files against a
**measured** endpoint delta of 213.5 MB — so the model is validated, not assumed.

**Step 1 is a false RSS positive** (arena-bucket quantization: +4.7 MB at one size, zero or negative
at three others) but its ~14 % time cost is real and consistent at every size.

**RSS and wall clock disagree about where the step is, and that is a finding, not noise.** `a7923b6`
introduced *both* this per-file memory cost and the attachment-fill exponent (the item `c8fc029`
fixed), so between 07-14 and 07-25 the exponent dominates at every corpus size and wall clock cannot
be attributed commit-by-commit in that window. `c8fc029` separates them cleanly: at 1,249 files it
takes wall 4.79 s → 0.192 s with RSS unchanged (81.4 → 81.6 MB). Two independent regressions.

### Mechanisms

- **`a7923b6`** — project-wide lexical event *preallocation* (`crates/typokat-check/src/check/checker/lexical_events.rs`,
  `events.rs`), mandated by [ADR-0008](../decisions/) for deterministic replay order. Every statement
  site eagerly reserves **three** entries (`event`, `deferred`, `incomplete`) in one checker-wide
  `BTreeMap<EventKey, Completion>` where `Completion` wraps a `Vec<CheckerRecord>` — ≥56 B plus
  B-tree node overhead per slot, allocated whether or not anything is ever recorded, for every file.
  The `ClassInstance` type itself is cheap; the preallocation is the cost.
- **`b6ecfa4`** — `DeclarationTable`: one dense row plus one hash entry per source declaration,
  eagerly materialized by `source_declaration_occurrences(program)` — a full AST walk the checker
  then repeats.
- **`fe61867`** — `collect_project_namespace_metadata` wired into `add_module`: a third full walk per
  file producing an 11-field `MergeParticipant` per declaration, keyed by a `MergeKey` that **owns a
  `String`** (`name.to_string()` for every declaration in the project). Its own commit message says
  the metadata is recorded "without publishing it to production resolution" — built eagerly before
  anything consumed it. The 2026-07-26 endpoint walked each file ~9 times before the checking walk.
- **`53ad026`** (07-23) — every hot arena became `LayeredVec`/`LayeredMap`; `get` became a
  compare + subtract + `Arc<[T]>` deref, map `get` became `base.get().or_else(local.get)`. Only
  +1.5 MB, so it was a **time** multiplier applied uniformly across all eight measured phases. At
  the 2026-07-26 endpoint, the then-production route left the layered base empty. That assumption
  predates the source-backed cutover and requires fresh measurement.

**The 2026-07-26 bisect did not implicate library loading.** Its measured startup floor was 5.5 MB /
~0 ms under the route that existed then. The source-backed cutover shipped later, so this is not a
current startup or route claim. Re-establish the baseline before choosing or ranking a fix.

### Subsequent shipped reductions

- **Merge substrate (`727fcf4`).** `MergeKey` names are interned and
  `MergeRecord.declarations` shares the placement participant vector. On the dated modules route,
  `modules-100000` moved from 364.8 to 353.2 MB and from 1.022 to 1.001 s. The measured attribution
  was −3.9 MB from interning and −7.3 MB from sharing. Diagnostics were unchanged and the official
  suite reported zero regressions.
- **Lexical completion storage (`c3c3549`).** Reservations no longer materialize a completion slot
  at every record position. Records are stored only when completed, while the unfinished-position
  audit and deterministic replay order remain enforced. `modules-100000` moved from 353.3 to
  319.1 MB and from 1.002 to 0.866 s; diagnostics were unchanged and the official suite reported
  zero regressions.

These commits reduced the dated endpoint but did not meet the historical ≤0.35 s / ≤200 MB target.
They also do not establish a baseline for the current source-backed route.

The largest step (+19.5 MB combined) is **behaviourally inert at its own boundary**: 356 fixtures plus
`errors-10000.ts` produce byte-identical output across it, 4,661 diagnostic lines each. It is
substrate for namespace work that landed later. `a7923b6`'s boundary was checked by the earlier
bisect — identical diagnostic multiset, one column regression, which is [`90`](./90-assignability-span-precision.md).

## Approach / acceptance

The 2026-07-26 evidence implicated all of these substrates and found that no single one covered the
165 MB gap. Two measured pieces have since shipped. Fresh measurement must establish whether the
remaining mechanisms still dominate before any further implementation.

> **Correction (2026-07-26).** An earlier revision ranked "namespace substrate (~94 MB)" first with
> `MergeKey` interning as its lead bullet. The ~94 MB is `b6ecfa4` **plus** `fe61867` — the
> `DeclarationTable` *and* the namespace metadata — whereas those bullets touch only part of the
> latter. Direct sizing (`size_of`: `MergeParticipant` 64 B, `MergeRecord` 160 B, `MergeKey` 32 B,
> `MergeClassification` 80 B; ~526 B retained per declaration across placement, record and index)
> puts the **whole merge substrate at ~30 MB, 8 % of the 369 MB**, of which interning plus dropping
> the duplicated participant vector reach ~9 MB and the risky singleton-record work another ~10 MB.
> Cross-checked against a measured point per the discipline this item exists to enforce: the model
> gives 5.9 MB at 1,249 files against the bisect's measured +11.1 MB for `fe61867`, so the merge
> substrate is ~53 % of that step and the model does not exceed it. The ranking below is corrected.

Remaining candidates from the dated ranking, to be reordered only after the fresh measurement:

1. **`DeclarationTable`** (`b6ecfa4`) — one dense row plus one hash entry per source declaration,
   eagerly materialized by a full AST walk the checker then repeats. Size it before scoping the fix.
2. **Walk deduplication** — the dated route walked each file about nine times; reducing that overlaps
   with the declaration-table work.
3. **Singleton namespace merge records.** `727fcf4` removed the participant copy but retained one
   merge record per non-merging declaration. Those records cannot simply be dropped: five production
   loops iterate the record set
   (`crates/typokat-check/src/check/checker/namespace_values.rs:2875`,
   `crates/typokat-check/src/check/checker/library_compiler/collision_plan.rs:27`,
   `crates/typokat-binder/src/binder/namespace.rs:3933`,
   `crates/typokat-binder/src/binder/namespace.rs:5216`, and
   `crates/typokat-binder/src/binder/references.rs:797`), so synthesising them on demand is a
   behaviour change, not a representation change.
4. **Collapse the layered indirection when `base_len() == 0`** — a time-only candidate at the dated
   endpoint; current routes require fresh measurement.
5. **Diffuse tail.** The unattributed ~7.7 KB/file tail from 07-16 onward needs its own pass if the
   fresh profile still shows it.

Before implementation, remeasure `modules-100000` at fresh HEAD under the current source-backed
route. Acceptance remains ≤0.35 s and ≤200 MB, which is what the dated comparator evidence says is
required to beat `tsgo` on this
family again; diagnostics byte-identical over `tests/cases/` and `errors-10000.ts`; official-suite
ratchet at 0 regressions. Do not weaken ADR-0008's replay determinism to get there.

## Touch points

`crates/typokat-binder/src/binder/declaration.rs`
(`DeclarationTable`), `crates/typokat-binder/src/binder/namespace.rs` (`MergeKey`, `MergeParticipant`,
`collect_project_namespace_metadata`), `crates/typokat-types/src/types/layered.rs`, `tooling/bench/`.

<!-- Origin: complexity hunt of the modules knee, 2026-07-25, which closed the exponent question and
     exposed this underneath. Bisected 2026-07-26 on peak RSS over f065e89..a0a5a6c with build-input
     equivalence proven. Endpoints independently reproduced by the leader. -->
