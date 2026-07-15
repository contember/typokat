---
id: 43
title: Namespaces (type side) + declaration merging
---

# 43 — Namespaces (type side) + declaration merging

**Summary.** Namespaces and declaration merging are absent. Architecture §4/§4.1 makes
both **mandatory core**: `lib.d.ts` itself uses `namespace`+`interface` merges (`Symbol`,
`globalThis`) and qualified names `N.T`, and interface+interface merging is ubiquitous in
ambient code. A hard prerequisite of `14`.

## Problem

Per §4.1 and [ADR-0009](../decisions/0009-ordered-declaration-groups-and-namespace-publication.md):
interface+interface, namespace(type container)+interface, function+namespace,
class+namespace (statics), and class+interface(+namespace); qualified type references `A.B` (tsc
`TS2503`/`TS2694` when unresolved); nested/reopened namespaces; and `declare global`. The binder's
multi-slot symbol design anticipated this — the namespace slot exists but is unused — but source
declaration identity must be separated from stable ordered type-group identity.

Valid `export as namespace` publication over either an `export =` or ordinary named-export external
module surface is owned by backlog `15`; its current WU0 witness is the `export =` form. This item
owns `TK1314`/`TK1315` context diagnostics where applicable and `TK2669` for global augmentation
outside an external or ambient module. Enum/function `TK2567` sites and exact
three-way enum/function/namespace legality belong to backlog `42`; this item owns namespace
placement `TK2434` and a non-permissive function+namespace surface.

## Approach / acceptance

Give every source declaration a unified lexical `DeclId`, keep value-storage identity distinct, and
build ordered `TypeGroupId` metadata dormantly before atomically switching `Symbol.ty` plus every
type consumer. Give every `NamespaceId` one public scope plus a private scope per reopening. Resolve
qualified names through public scopes without missing-segment fallback. Standalone interfaces
freeze complete source-order recovery surfaces, publish dependency SCCs behind a final-state
capability, and only then run typed pending heritage/conflict/relation obligations through
`SemanticQueryCoordinator`. Class+interface(+namespace) groups reuse the class SCC barrier and a
complete effective recovery-frame application identity. Function+namespace groups publish one
immutable callable object containing all callable rows and exported namespace value members.
Generic-header and member/overload/conflict recovery follows the committed strict-tsc corpus,
including typed invalid-group recovery with distinct parameter positions. A standalone namespace
stays a type container; only an existing function/class value may receive exported value members.

Reserve `declare global` scope/records as disconnected metadata first, then atomically link/publish
one compilation-global scope across files while module locals remain isolated; do not introduce
another ambient resolver or `Store`. Direct gates cover legal external/ambient contexts and
`TK2669` in scripts. Corpus first; cross-check tsc 6.0.3 `--strict`. Structurally split the official-
suite namespace gate after landing; never remove a broad regex that also hides external-module or
unrelated cases.

## Touch points

`src/binder/` (namespace slot activation, merging), `src/check/checker/annotations.rs`
(qualified names), `src/types/` (merged-interface identity).

<!-- Origin: completion-roadmap review (2026-07-07); architecture §4.1 requirement, prerequisite of 14. -->
