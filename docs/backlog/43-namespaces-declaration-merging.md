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
owns `TK1314`/`TK1315` context diagnostics where applicable, `TK2669` for global augmentation
outside an external or ambient module, and `TK2670` for a non-ambient `global` block without
`declare`. Enum/function `TK2567` sites and exact
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

Reserve `declare global` scope/records as disconnected metadata first. Give each augmentation an
originating lexical overlay, quarantine every `TK2669`/`TK2670` block, and promote only legal
interfaces, type aliases, and type-only namespaces through the ordinary `TypeGroupId` and
`NamespaceId` machinery. Once every legal group is complete, atomically link all user modules to
one compilation-global scope; module locals remain isolated and no second ambient resolver or
`Store` is introduced. Direct gates cover legal external/ambient contexts, invalid
placement/modifier contexts, exact diagnostic ownership, cross-file merging, and opposite input
order.

This item is deliberately limited to declarations without an inseparable value side. Global
variables, functions, complete class type/constructor pairs, and cross-file
class/function+namespace value payloads remain [`82`](82-declare-global-value-space.md).
That tail does not block `lib.es5.d.ts` loading: its ambient values are top-level declarations owned
by loader `14`, not `declare global` value-space publication. Corpus first; cross-check tsc 6.0.3
`--strict`. Structurally split the official-suite namespace gate after landing; never remove a
broad regex that also hides external-module or unrelated cases.

## Touch points

`src/binder/` (namespace slot activation, merging), `src/check/checker/annotations.rs`
(qualified names), `src/types/` (merged-interface identity).

<!-- Origin: completion-roadmap review (2026-07-07); architecture §4.1 requirement, prerequisite of 14. -->
