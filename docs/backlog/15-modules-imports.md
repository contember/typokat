---
id: 15
title: Modules / imports / module resolution (whole-repo checking)
blocked-by: []
---

# 15 — Modules / imports / module resolution

**Summary.** Whole-repo checking; also where I/O cost dominates. The correctness-first local
relative slice shipped as M29; the remaining work is full resolver breadth plus parallelism
**Stage 2** cross-file identity.

## Problem

M29 added a serial `check_project` path for provided local relative `.ts` files with named
imports/exports. Real projects still need package/tsconfig resolver breadth and, for parallel
checking, a cross-file type-identity strategy.

## Approach / acceptance

Implement this in two explicit slices:

1. **Correctness-first whole-repo slice (shipped as M29):** local relative module resolution +
   import/export wiring runs serially in a single type universe. Acceptance: multi-file fixtures
   with imports/exports check correctly. This slice proves semantics before solving the
   shared-interner problem.
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
