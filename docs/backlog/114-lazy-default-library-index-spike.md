---
id: 114
title: Evaluate a project-local lazy default-library index
blocked-by: [./113-measure-sparse-collision-lifecycle.md, ./115-prepared-project-single-parse.md]
---

# 114 — Evaluate a project-local lazy default-library index

**Summary.** Test whether an immutable symbolic library index plus project-local demand publication
can replace the frozen semantic base without recreating sparse replay or adding a permanent fourth
lifecycle. Non-blocking architecture spike, effort XL.

## Problem

`FrozenLibraryBase` avoids compiling the 82-file default library for every check in a repeated
process, but it also fixes the project around a shared semantic universe and requires special
collision machinery. A lazy library is a plausible simplification only if the shared artifact is
immutable and store-independent while every project owns its binder and semantic store.

The current components cannot be rearranged into that design:

- the binder checkpoint is non-cloneable and collision merging uses sparse graph/symbol/group
  overlays;
- type-group and class publication is one eager, consume-once epoch;
- `DeclaredTypeRecipe` embeds universe-local `TypeId` and `TypeParamId` values and supports only a
  bounded annotation subset;
- event ordering reserves the complete lexical inventory before semantic work;
- library bridges and the global object are derived after complete publication and may pull a large
  common dependency closure.

This is distinct from backlog [`76`](76-lazy-value-type-resolution.md). Item 76 resolves inferred
types for source declarations such as functions, values, accessors, and static members. It does not
provide a store-independent default-library representation or a declaration-group materializer.
The two designs may share cycle and terminal-state rules, but neither blocks the other semantically.

## Approach / acceptance

Build a dual-path prototype beside the current provider. The shared `LibraryIndex` may contain only
immutable symbolic facts:

- exact source/profile identity and stable source sites;
- binder names, declaration groups, and dependency/SCC edges;
- stable symbolic declaration, generic, nominal, and lexical-event identities;
- store-independent lowering recipes that cover the selected library slice.

Each checked project must own its ordinary binder view, `Interner`/`Store`, relation and flow caches,
semantic bridges, global object, and a publication state machine such as
`Unplanned -> Reserved -> Resolving(SCC) -> Ready | Unavailable`. User augmentations join the
complete declaration group before its first semantic publication. Classes, interfaces, namespaces,
and callable/value surfaces publish atomically. Provisional identities or types must never enter
relation, flow, or diagnostic caches.

Start with a bounded representative slice, then expand only after it passes. Full-profile acceptance
requires:

1. `force_all` materializes all 82 files without eager semantic fallback and reproduces the named
   library census, bridge identities, and normalized semantic probes;
2. library/user collisions and augmentations produce the same terminal groups in either file order,
   with no sparse semantic prefix replacement;
3. reverse and randomized demand orders produce identical diagnostics, incomplete records, event
   order, and semantic probes;
4. recursive class/interface/namespace SCCs expose one complete terminal or one typed unavailable
   terminal, never a partial surface;
5. measurements record index-build cost, project binder creation, demanded/published groups, store
   rows, cold and repeated-batch time, and peak RSS;
6. the production route retains no owner-site semantic replay plan disguised as an index.

Kill the design if common roots materialize most of the profile, a project binder view recreates the
current sparse overlay, a store-independent recipe layer becomes a second near-complete AST without
broader value, demand order is observable, or the old and new lifecycles must coexist permanently.

If the spike passes, write an ADR that explicitly supersedes the relevant parts of ADR-0011,
ADR-0017, and ADR-0020 before production cutover. Delete `FrozenLibraryBase` only after full parity
and repeated-consumer performance gates pass.

## Touch points

Default-library profile/index construction, binder group inventory, declaration lowering,
`TypeEnvironmentState`, class/namespace publication, event ledger, library identities/global object,
provider routing, census/probe fixtures, and a superseding ADR.

<!-- Origin: 2026-08-20 development-friction audit; lazy-library proposal and adversarial refutation. -->
