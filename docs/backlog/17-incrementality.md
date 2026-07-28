---
id: 17
title: Incrementality (Phase 5, Salsa-style)
blocked-by: [./16-parallelism-type-universe.md]
---

# 17 — Incrementality (Phase 5)

**Summary.** Salsa-style incremental recomputation. Shares its load-bearing primitive (the blake3
stable hash) with parallelism Stage 2.

## Problem

typokat recomputes everything on each run. Incremental recomputation (re-check only what changed) is
the Phase 5 goal and the basis for IDE responsiveness.

## Approach / acceptance

Two stages (peer-reviewed 2026-07-09; see `docs/ideas/phpstan-architecture-lessons.md`, proposal D):

- **Stage A — batch result cache** (the tsc `--incremental`/`.tsbuildinfo` shape; PHPStan's
  `ResultCacheManager` is the same design): per-file content hash + an inverted import graph +
  the blake3 stable hash of each file's **checked exported type surface** as the cascade cutoff —
  a body-only edit whose export-surface hash is unchanged does not cascade to dependents. The
  cutoff must be **semantic** (hash the checked surface, never declaration syntax — TS infers
  export types from bodies), and propagation runs a worklist to fixpoint (a dependent's re-check
  may change *its* surface hash). Serves batch/CI re-runs and exercises the stable hash shared
  with parallelism Stage 2 (item 16) on a cheap target.
- **Stage B — Salsa-style query layer** over the pipeline, for the IDE-responsiveness goal
  (warm process, sub-file granularity). Stage A's surface hashes carry over (tsserver reuses the
  same signature machinery).

Either way this finally **computes the blake3 stable structural hash** (the interner is already
shaped for it — `crates/typokat-types/src/types/hash.rs`; see [`../reference/invariants.md`](../reference/invariants.md)
§2). Acceptance: an edit to one file re-checks only the affected work, with results identical to a
full check.

## Touch points

The stable structural hash (`crates/typokat-types/src/types/hash.rs`); the Stage A result cache (file hashes, inverted
import graph, export-surface hashes); a Salsa-style query/recompute layer over the pipeline.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term — formerly Phase 4 before real-project scale was split out). -->
