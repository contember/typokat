---
id: 17
title: Incrementality (Phase 4, Salsa-style)
blocked-by: [./16-parallelism-type-universe.md]
---

# 17 — Incrementality (Phase 4)

**Summary.** Salsa-style incremental recomputation. Shares its load-bearing primitive (the blake3
stable hash) with parallelism Stage 2.

## Problem

typokat recomputes everything on each run. Incremental recomputation (re-check only what changed) is
the Phase 4 goal and the basis for IDE responsiveness.

## Approach / acceptance

Adopt a Salsa-style incremental model and finally **compute the blake3 stable structural hash** (the
interner is already shaped for it — `src/types/hash.rs`; see
[`../reference/invariants.md`](../reference/invariants.md) §2). The hash is shared work with
parallelism Stage 2 (item 16). Acceptance: an edit to one file re-checks only the affected work, with
results identical to a full check.

## Touch points

The stable structural hash (`src/types/hash.rs`); a Salsa-style query/recompute layer over the
pipeline.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term — Phase 4). -->
