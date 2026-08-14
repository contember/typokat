---
id: 89
title: Multi-file scaling lacks a benchmark gate and ADR policy
blocked-by: []
---

# 89 — Multi-file scaling lacks a benchmark gate and ADR policy

**Summary.** A 615× multi-file regression survived nine days of active performance work because its
local-layer scans and multi-file growth were unguarded. The local-layer probes and a real
owner-attachment scaling guard shipped in `d1a1ca5` and `904642f`. The benchmark gate and the policy
for compilation-wide structures remain open. Effort M.

## Problem

A bisect (2026-07-25) traced the multi-file collapse to `a7923b6` (2026-07-14). The same bug class
then landed **repeatedly** — `owner_at`, `set_placement_syntax`, the `namespace_fragment` back-patch,
`classify`, `fill_namespace_value_attachments`, `attach_type_decl_owners` — always the same shape:
*scan a whole-project collection once per item*. Nothing in the test suite or the benchmark could see
any of them.

**Shipped guard — local-layer scans and owner attachment.** `d1a1ca5` added matched local-layer scan
probes across the full-view accessors and made the real `attach_type_decl_owners` offender fail its
RED scaling spec. `904642f` then bounded owner attachment by each module's `DeclId` range, reducing
the witnessed rows from 2,410 to 241 at eight modules and from 8,082 to 449 at sixteen. It also
widened the probes to cover the iterator forms that could otherwise evade the guard. This closes the
original local-layer instrumentation gap; it does not make every future whole-project collection
observable automatically.

**Open gap 1 — no multi-file scaling row.** The archived
[`2026-07-21 full-lib sprint`](../archive/sprint-2026-07-21-full-lib-performance-cutover.md)
measured only the single-file library profile. `tooling/bench/typobench.py` does have a `modules`
family, but it is not run as a gate, so nothing failed when the 6,249-file corpus went from 0.307 s
(2026-07-09, *faster* than tsgo's 0.374 s) to 188.7 s.

**Open gap 2 — ADR-0008 authorized the compilation-wide `LexicalReservations` table with no complexity or
scaling consideration recorded at all.** A whole-project structure whose lookups are per-item is a
scaling decision, and it was taken without one.

## Approach / acceptance

1. Add a multi-file scaling row to whatever runs as a gate: at minimum, assert the per-file cost of
   an N-file project does not grow with N (the decisive control is a fixed program split across
   varying file counts — a regression shows as `t = a + b·M` with `b > 0` at constant total size).
2. When an ADR introduces a compilation-wide structure, require a scaling note: what is scanned, by
   what, how often.

Acceptance: the multi-file gate fails when a synthetic per-item project scan makes per-file cost grow
with project size, and the ADR template or review checklist requires the scaling note for every new
compilation-wide structure. The shipped local-layer and owner-range specs stay green.

## Touch points

`tooling/bench/` gating, `docs/decisions/` template or review checklist.

<!-- Origin: bisect of the multi-file regression, 2026-07-25 (findings 6-8). -->
