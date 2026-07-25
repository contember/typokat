---
id: 93
title: Namespace attachment fill scans every merge once per module
blocked-by: []
---

# 93 — Namespace attachment fill scans every merge once per module

**Summary.** `fill_namespace_value_attachments` walks the project's whole merge set for each module,
so it costs `Θ(modules × declarations × c(declarations))`. It is **the** superlinear term left by
`202f0bc` — **5.88 s of that commit's remaining 6.86 s on `modules-100000`, 86 % of wall clock** —
and on this corpus it produces zero targets, so all of it is waste. Effort M. **Highest-value item
in the perf ladder.**

## Problem

`bind_namespace_value_attachment_members` (`src/binder/namespace.rs`) walks
`state.namespaces.local_merges()` in full and filters to the merges belonging to the module being
filled. Every module therefore pays for every declaration in the project.

Before `202f0bc` each module saw only the merges accumulated *so far*, summing to `D × (M+1)/2`.
Hoisting classification out of the per-module loop means all `M` fills now see the full `D`, so the
term became `D × M` — **about 2× more attachment rows than before**, on the project path and the
pre-existing library path alike. That was a deliberate, measured trade: classification's per-row cost
(clone, sort, `classify_group`, `placement_issues`, `String` re-key) dwarfs the attachment filter, so
removing `M` classifications while doubling this scan was still a 28× net win. The counter is
recorded but deliberately left unbounded by the guard in `202f0bc`, precisely so it can be tightened
here.

Measured (`NamespaceFinalizationWorkForTest.attachment_merge_rows`, 192 groups / 576 placements):

| split | before `202f0bc` | after |
|---|---|---|
| 8 modules | 2,592 | 4,608 |
| 64 modules | 18,720 | 36,864 |

Both are exactly `D × M` after, `D × (M+1)/2` before.

**This term is the entire knee.** Directly measured on `modules-100000` (6,249 files) by skipping the
pass in an instrumented build: **6.86 s → 0.98 s, a 5.88 s delta = 86 % of wall clock**, with the
counter reading `fills=6249 rows=390,531,255 targets=0` — 390 million rows producing **nothing** on
this corpus. Removing it takes the residual exponent to **1.03–1.12, flat out to 12,000 files**;
every other phase scales 4.9–6.6× for 6.25× more files. There is no second superlinear term here.

**There is a third factor beyond `M × D`: per-row cost rises with `D`.** Each row touches the
`MergeRecord` header and pointer-chases into its out-of-line `declarations: Vec`
(`namespace.rs:3657`, the first thing `namespace_value_attachment_disposition` does). At ~190 B per
merge the table leaves this box's 16 MiB L3 around `D ≈ 60–90 k`, exactly where `modules-100000`
sits. Split fixed at `M = 500`, varying only `D`:

| `D` | rows | fill time | ns/row |
|---|---|---|---|
| 24,010 | 12.0 M | 0.055 s | 4.53 |
| 48,010 | 24.1 M | 0.221 s | 9.17 |
| 96,010 | 48.1 M | 0.955 s | 19.86 |
| 192,010 | 96.2 M | 2.075 s | 21.57 |

So the true cost is `Θ(M · D · c(D))` with `c` rising ~4.8× across the range.

> **Correction (2026-07-25).** This item originally claimed ~1.5 s of 6.74 s, from measuring
> `0.246 ms/module` at `D = 37,500` and extrapolating linearly to 6,249 modules. That extrapolation
> crossed the cache boundary above: the real per-module cost at `D = 62,474` is **0.903 ms**, 3.7×
> higher, and `1.54 × 3.7 ≈ 5.7 s`. Same measurement, wrong extrapolation — a per-row cost that is
> itself a function of the input size cannot be extended linearly. Anyone re-estimating this must
> quote ns/row at the target `D`, not at a convenient one.

**It is not only a wasted scan — it is the whole project's fill repeated `M` times.** On a
namespace-bearing corpus (200 files, one `function` + `namespace` merge each) the counter reads
`fills=200 rows=120,000 targets=40,000` — 40,000 = 200 fills × **200** targets, i.e. every module
re-derives and re-applies the entire project's target set. The redundancy factor is exactly `M`, and
it also pays `M × (N log N)` in the `sort_by_key`/`dedup_by_key` at `namespace.rs:4247-4259`.

**Not a benchmark artifact — real code is hit harder.** The corpus's 6-declarations-per-file
uniformity makes the term clean to measure, not large. At a realistic 30 declarations/file the fill
is already 60 % of a **2,000-file / 60 k-declaration** project (1.656 s of 2.750 s), and 70 % on a
1,600-file namespace-using corpus — worse than the bench corpus, which has zero namespaces and so
pays only the scan, never the target re-derivation.

## Approach / acceptance

Index merges by the scope (or module) that owns them, so a fill visits only its own, and apply it to
both the project and library paths — `try_add_library_modules` has the identical shape.

Acceptance: tighten the existing guard in
`src/binder/namespace.rs::namespace_finalization_reprocesses_the_project_once_per_project_not_once_per_module`
to bound `attachment_merge_rows` the way the other three counters are already bounded — flat in the
split, so 576 at both 8 and 64 modules. `modules-100000` should reach **~0.98 s** with the exponent
flat at ~1.05 out to 12,000 files; report the exponent, not just the time.

Diagnostics must not move: diff full rendered output over all `tests/cases` fixtures and the
merge-spanning corpora **in both file orders**, since merge order decides which declaration wins.
Note the `modules` corpus is worthless as diagnostic evidence — it emits nothing and derives zero
targets — so the namespace-bearing corpora are the real gate.

Two hazards, both concentrated in one place:

- **A merge spanning modules** (namespace continued in a second file; `function` in A, `namespace`
  in B) must still fill *both* modules, or the second file's members lose `local_symbol` /
  `symbol.value`. Today's "apply the project-wide set, M times" is accidentally safe here; "apply my
  set, once" is not, unless the index is keyed so every participating module sees the merge.
- `targets.sort_by_key(span.start, DeclId)` then `dedup_by_key(declaration)`
  (`namespace.rs:4247-4259`) currently sorts across the whole project. Shrinking the set changes the
  dedup neighbourhood, which is precisely the mechanism by which merge order decides the winner —
  hence the both-orders requirement above.

## Touch points

`src/binder/namespace.rs` (`bind_namespace_value_attachment_members`,
`fill_namespace_value_attachments`), `src/binder/bind.rs` (`finalize_pending_namespaces`,
`try_add_library_modules`).

<!-- Origin: WU2 of the checker-scaling sprint, 2026-07-25. The inversion was predicted in the WU2
     RED guard's commit message before the fix landed, then measured after it. -->
