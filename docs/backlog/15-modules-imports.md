---
id: 15
title: Modules / imports / module resolution (whole-repo checking)
blocked-by: []
---

# 15 — Modules / imports / module resolution

**Summary.** Whole-repo checking; also where I/O cost dominates. This is parallelism **Stage 2**.

## Problem

typokat checks a single file. Real projects need modules, imports, and module resolution to check
across file boundaries.

## Approach / acceptance

Implement this in two explicit slices:

1. **Correctness-first whole-repo slice:** module resolution + import/export wiring may run
   serially or in a single type universe. Acceptance: a multi-file fixture with imports/exports
   checks correctly. This slice proves semantics before solving the shared-interner problem.
2. **Parallel/cross-universe slice:** crossing file boundaries under per-file parallel checking is
   parallelism **Stage 2**: cross-file type identity via the stable structural hash, or a shared
   *growing* interner (the §3.4 knot — architecture §8.2). Acceptance: cross-file type identity holds
   under parallel execution.

Do not silently mix the two: if the first slice ships without Stage 2, document that whole-repo
checking is correct but not yet using the final parallel type-universe strategy.

## Touch points

Module resolution + import/export binding; cross-file type identity (stable structural hash or shared
interner — architecture §8.2); `driver::check_files` once Stage 2 begins.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
