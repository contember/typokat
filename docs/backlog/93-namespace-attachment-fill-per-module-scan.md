---
id: 93
title: Namespace attachment fill scans every merge once per module
blocked-by: []
---

# 93 — Namespace attachment fill scans every merge once per module

**Summary.** `fill_namespace_value_attachments` walks the project's whole merge set for each module,
so it costs `Θ(modules × declarations)`. It is the residual superlinear term left by
`202f0bc`, and worth roughly 1.5 s of that commit's remaining 6.74 s on `modules-100000`. Effort M.

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

Both are exactly `D × M` after, `D × (M+1)/2` before. Held at 37,500 declarations and varying only
the split, the after-binary's per-module term is **0.246 ms at D=37,500 against 0.0565 ms at
D=8,000 — 4.4× for 4.7× more declarations**, i.e. linear in `D`, which is this scan's signature. On
the uniform 6-declarations-per-file bench corpus that makes it `Θ(M²)`.

The scan is *not* the whole remainder: the `modules-100000` curve's exponent is ~1.0 up to 600 files, 1.41
at 2,499 and 2.59 from 2,499→6,249. This term accounts for ~1.5 s of 6.74 s; the rest of that knee is
unattributed and needs its own hunt before anyone assumes this item closes the gap.

## Approach / acceptance

Index merges by the scope (or module) that owns them, so a fill visits only its own, and apply it to
both the project and library paths — `try_add_library_modules` has the identical shape.

Acceptance: tighten the existing guard in
`src/binder/namespace.rs::namespace_finalization_reprocesses_the_project_once_per_project_not_once_per_module`
to bound `attachment_merge_rows` the way the other three counters are already bounded — flat in the
split, so 576 at both 8 and 64 modules. Diagnostics must not move: diff full rendered output over all
`tests/cases` fixtures and the merge-spanning corpora **in both file orders**, since merge order
decides which declaration wins. `modules-100000` should drop by ~1.5 s; report what the exponent does,
because that is the real question.

## Touch points

`src/binder/namespace.rs` (`bind_namespace_value_attachment_members`,
`fill_namespace_value_attachments`), `src/binder/bind.rs` (`finalize_pending_namespaces`,
`try_add_library_modules`).

<!-- Origin: WU2 of the checker-scaling sprint, 2026-07-25. The inversion was predicted in the WU2
     RED guard's commit message before the fix landed, then measured after it. -->
