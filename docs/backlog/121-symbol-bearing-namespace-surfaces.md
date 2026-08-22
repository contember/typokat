---
id: 121
title: Lower symbol-bearing callable namespace surfaces
blocked-by: []
---

# 121 — Lower symbol-bearing callable namespace surfaces

**Summary.** Map ordinary `TSSymbolKeyword` annotations through the existing callable namespace
surface authority so `Reflect.ownKeys` and equivalent namespace functions can publish complete
terminals. This is WU1 and the semantic-oracle prerequisite of the active
[`project-local library universe sprint`](../sprints/sprint-2026-08-22-project-local-library-universe.md).

## Problem

Callable namespace surface lowering currently withholds a terminal when a signature contains the
ordinary `symbol` keyword. The pinned acceptance fixture is
[`tests/cases/b121_symbol_namespace_surface/symbol_namespace_surface.ts`](../../tests/cases/b121_symbol_namespace_surface/symbol_namespace_surface.ts).
It remains disabled until the library lifecycle can absorb the semantic movement soundly.

The archived project-local index feasibility sprint proved that the syntax gap is not isolated.
Its removed materializer reported 44 changed `lib.dom.d.ts` record owners but retained no named
rows. WU0 could not reproduce that count: a consistent symbol fix left the 559-record packaged
census unchanged, and a controlled old-base/new-candidate symbol toggle produced zero changed or
missing DOM `TK2344` fingerprints. The historical count is therefore not an oracle. Copying frozen
fingerprints, narrowing an authenticated closure without proof, or making `symbol` test-only would
still hide a real semantic difference.

## Approach / acceptance

Lower `TSSymbolKeyword` through the ordinary surface type-syntax authority and its existing
`well_known().symbol` identity. Establish the correct complete-source meaning before the
project-local declaration product consumes it. Do not add a cast, parallel lowerer,
namespace-only semantic shim, alternate query route, or sparse-specific repair.

Acceptance requires:

- the pinned `b121_symbol_namespace_surface` corpus is enabled and reports its exact `TK2322` and
  `TK2345` witnesses;
- the packaged `Reflect` namespace/value terminal is `Ready`, including `Reflect.ownKeys`;
- every library-owned diagnostic/incomplete movement is reviewed and pinned exactly;
- complete-source semantic truth is established without copying frozen/sparse fingerprints;
- the named census has a reviewed before/after delta, including an explicit empty delta if unchanged;
- ordinary shared/cold parity and the surviving provider/standalone route gates pass.

## Touch points

Callable namespace surface type-syntax lowering, `WellKnownTypes`, the packaged Reflect census,
library-owned record fixtures, collision/fanout lifecycle tests, shared/cold parity, and the pinned
backlog-121 conformance corpus.

<!-- Origin: archived 2026-08-22 project-local library-index feasibility STOP; scheduled as WU1 of
     the project-local library universe sprint. -->
