---
id: 113
title: Replace the frozen library lifecycle with project-local semantic universes
blocked-by: []
---

# 113 — Replace the frozen library lifecycle with project-local semantic universes

**Summary.** Replace `FrozenLibraryBase`, collision routing, and sparse semantic replay with one
project-owned binder and demand-driven semantic universe over a shared immutable declaration
product. This work is scheduled by the active
[`project-local library universe sprint`](../sprints/sprint-2026-08-22-project-local-library-universe.md).
Research architecture work, effort XL.

## Problem

The default library currently has three semantic lifecycles:

1. a frozen shared base with an ordinary project suffix;
2. a frozen shared base with a private sparse collision epoch;
3. complete-source compilation for standalone checks.

The split exists because library semantics publish before the user project's complete global
declaration set is known. Sparse replay repairs affected frozen meaning afterward. It therefore
makes a lifecycle optimization part of every binder row, semantic table, identity, event owner,
cache boundary, and query. The 2026-08-20 development-friction audit counted 526
layered/frozen/sparse references across 22 production files; `fork_sparse_delta` alone appeared 72
times across 13 files.

The selected target moves the ownership boundary instead of optimizing this repair. The process may
share exact packaged sources and an immutable AST-free declaration product. Each project owns its
complete binder, global merge, semantic identities, type store, terminals, global object, caches,
events, and diagnostics. Declaration discovery and binding are eager; semantic publication is
demand-driven by dependency SCC through the existing checker `Pass`.

## Existing evidence

The archived
[`project-local library-index feasibility sprint`](../archive/sprint-2026-08-22-project-local-library-index-feasibility.md)
proved that a store-independent declaration index is feasible for a real 12-group slice. It stopped
before augmentation and performance evaluation because making `Reflect` Ready contradicted a
false-clean oracle. Its removed candidate also reported 44 DOM record owners without retaining
their names. WU0 could not reproduce that historical count, so only the symbol/Reflect truth and a
fresh named census are binding prerequisites.

The immutable WU1
[`lifecycle receipt`](../archive/project-local-library-index-feasibility-2026-08-22/lifecycle-baseline.json)
has SHA-256 `950d5d7b38da7e7b5a0bc87da7ed423968b151ebf731c451021216c1d1880a18`.
Under `cpu-lease run -n 2 --no-smt --`, it authenticated all 687 official-suite worker frames:
395 private, 247 shared, 42 user-parse-rejected, three complete-source fallbacks, and zero preflight
rejections. Base initialization took 1.551 s total and peak worker RSS was 144,109,568 bytes.

WU1 corrected the symbol-bearing Reflect surface in the complete-source semantic authority. The
named census removed exactly one obsolete Reflect incomplete: diagnostics stayed at 267,
incompletes moved from 292 to 291, and total records moved from 559 to 558. The declaration product
must consume this truth without copying frozen fingerprints or implementing a sparse-specific
repair.

## Approach / acceptance

The active sprint must deliver:

- one full-profile owned declaration product with no AST or universe-local semantic state;
- one eager project-local binder that joins all library and user global declarations before
  semantic publication;
- one project-local demand state machine that reserves dependency SCCs and publishes only final
  Ready or typed Unavailable terminals through `Pass`;
- one `CheckerSession` lifecycle for standalone and repeated checks;
- exact parity with `tsc`/complete-source oracles, including Reflect and the named library census;
- authenticated cold and repeated comparisons that strictly beat the pinned native `tsgo`
  comparator in wall time without using more median peak RSS; and
- vertical deletion of the frozen/sparse/collision lifecycle after cutover.

The current provider remains only as a temporary test control. Missing a performance gate requires
profiling and optimizing the project-local representation, binder, query granularity, or scheduling.
It does not authorize retaining sparse replay or shipping a frozen/complete-source hybrid.

There is one shipped outcome: the project-local lifecycle passes all semantic and performance gates
and the old lifecycle is deleted. If the fixed ownership boundary itself proves unsound or cannot be
represented without shared semantic state or a second publisher, stop and record that architecture
finding rather than inventing a compatibility layer.

## Touch points

Canonical declaration inventory, a new immutable declaration product, frontend prepared projects,
binder/global merging, checker semantic publication/query coordination, library provider/session,
driver/CLI, lifecycle tests, the official-suite batch, `tooling/full-lib-bench`, a pinned in-process
`tsgo` comparator, architecture/invariants, and superseding ADRs.

<!-- Origin: 2026-08-20 development-friction audit, the 2026-08-22 feasibility STOP, and the
     explicit project-local research direction selected on 2026-08-22. -->
