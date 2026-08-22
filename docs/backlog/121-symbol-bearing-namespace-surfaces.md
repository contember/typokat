---
id: 121
title: Lower symbol-bearing callable namespace surfaces
blocked-by: [./113-measure-sparse-collision-lifecycle.md]
---

# 121 — Lower symbol-bearing callable namespace surfaces

**Summary.** Map ordinary `TSSymbolKeyword` annotations through the existing callable namespace
surface authority so `Reflect.ownKeys` and equivalent namespace functions can publish complete
terminals. Do this only after backlog
[`113`](113-measure-sparse-collision-lifecycle.md) resolves the sparse collision lifecycle.

## Problem

Callable namespace surface lowering currently withholds a terminal when a signature contains the
ordinary `symbol` keyword. The pinned acceptance fixture is
[`tests/cases/b121_symbol_namespace_surface/symbol_namespace_surface.ts`](../../tests/cases/b121_symbol_namespace_surface/symbol_namespace_surface.ts).
It remains disabled until the library lifecycle can absorb the semantic movement soundly.

The archived project-local index feasibility sprint proved that the syntax gap is not isolated.
Making `Reflect` Ready changed the existing sparse collision lifecycle: one Array-collision witness
activated 44 sealed `lib.dom.d.ts` record owners whose `TK2344` fingerprints were no longer emitted
by the candidate replay. The sealed baseline validator correctly selected complete-source fallback.
Copying those fingerprints, narrowing the authenticated replay closure without proof, or making
`symbol` test-only would hide a real semantic difference.

## Approach / acceptance

After backlog 113 records the DELETE/RETAIN decision, lower `TSSymbolKeyword` through the ordinary
surface type-syntax authority and its existing `well_known().symbol` identity. Do not add a cast,
parallel lowerer, namespace-only semantic shim, or alternate query route.

Acceptance requires:

- the pinned `b121_symbol_namespace_surface` corpus is enabled and reports its exact `TK2322` and
  `TK2345` witnesses;
- the packaged `Reflect` namespace/value terminal is `Ready`, including `Reflect.ownKeys`;
- every library-owned diagnostic/incomplete movement is reviewed and pinned exactly;
- the lifecycle selected by backlog 113 remains explicit: either the retained sparse route passes
  its authenticated collision/fanout receipts, or the selected complete-source collision fallback
  reports its fixed lifecycle receipt;
- the 44 activated DOM fingerprint case is an exact regression witness, not a blessed omission;
- ordinary shared/cold parity and the surviving provider/standalone route gates pass.

## Touch points

Callable namespace surface type-syntax lowering, `WellKnownTypes`, the packaged Reflect census,
library-owned record fixtures, collision/fanout lifecycle tests, shared/cold parity, and the pinned
backlog-121 conformance corpus.

<!-- Origin: archived 2026-08-22 project-local library-index feasibility STOP. -->
