---
id: 89
title: Nothing guards against per-item scans of whole-project state
blocked-by: []
---

# 89 — Nothing guards against per-item scans of whole-project state

**Summary.** A 615× multi-file regression survived nine days of active performance work because two
guard rails are missing: the layered containers instrument only their *base* layer, and no benchmark
row exercises multi-file scaling. Both are cheap. Effort M.

## Problem

A bisect (2026-07-25) traced the multi-file collapse to `a7923b6` (2026-07-14). The same bug class
then landed **repeatedly** — `owner_at`, `set_placement_syntax`, the `namespace_fragment` back-patch,
`classify`, `fill_namespace_value_attachments`, `attach_type_decl_owners` — always the same shape:
*scan a whole-project collection once per item*. Nothing in the test suite or the benchmark could see
any of them.

**Gap 1 — the scan probe covers the wrong layer.** `src/types/layered.rs:332` instruments `iter()`
with `record_full_view_base_scan_for_test(self.base.len())`, guarding scans of the frozen *library*
base. But `local_iter()` at `:339` — the user-project delta, which is exactly what every one of the
quadratics above scans — has **no probe at all**. The guard rail watches the layer that is sealed and
ignores the layer that grows.

**Gap 2 — no multi-file scaling row.** `docs/sprints/sprint-2026-07-21-full-lib-performance-cutover.md`
measured only the single-file library profile. `tooling/bench/typobench.py` does have a `modules`
family, but it is not run as a gate, so nothing failed when the 6,249-file corpus went from 0.307 s
(2026-07-09, *faster* than tsgo's 0.374 s) to 188.7 s.

**Gap 3 — ADR-0008 authorized the compilation-wide `LexicalReservations` table with no complexity or
scaling consideration recorded at all.** A whole-project structure whose lookups are per-item is a
scaling decision, and it was taken without one.

## Approach / acceptance

1. Add a `local_iter()`/`local_values_mut()` scan probe mirroring the existing base probe, and assert
   in binder/checker specs that per-module and per-declaration work is O(module) and O(1) — not
   O(project). Make this the standing guard: any new whole-project scan on a per-item path fails a
   test rather than a benchmark nine days later.
2. Add a multi-file scaling row to whatever runs as a gate: at minimum, assert the per-file cost of
   an N-file project does not grow with N (the decisive control is a fixed program split across
   varying file counts — a regression shows as `t = a + b·M` with `b > 0` at constant total size).
3. When an ADR introduces a compilation-wide structure, require a scaling note: what is scanned, by
   what, how often.

Acceptance: a synthetic regression (deliberately re-introducing a per-item project scan) is caught by
the new probe assertions, not just by wall clock.

## Touch points

`src/types/layered.rs` (the `local_iter` probe), binder/checker scaling specs, `tooling/bench/`
gating, `docs/decisions/` template or checklist.

<!-- Origin: bisect of the multi-file regression, 2026-07-25 (findings 6-8). -->
