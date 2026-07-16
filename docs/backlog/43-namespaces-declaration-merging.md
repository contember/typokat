---
id: 43
title: Namespaces (type side) + declaration merging
---

# 43 — Namespaces (type side) + declaration merging

**Summary.** Type-side namespaces and declaration merging are substantially shipped. One
architecture stop remains before backlog `14` may start: standalone ambient namespace value
metadata and qualified value access. The pinned WU6 proof is
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).

## Problem

ADR-0009 now backs ordered declaration groups, public/private namespace scopes, qualified type
paths, interface merging, function/class keep-pairs, and legal global type publication. These
surfaces are order-independent and preserve immutable publication. The WU6 proof also demonstrates
all 28 ES5 interface+`declare var` pairs, repeated `Date`/`Number`/`String` interfaces, namespace
public-sibling lookup, and local `interface Array<T>` heritage.

The remaining `Intl` failure is value-side: `declare namespace Intl` publishes its type container,
but typokat records `decl/module-declaration/self` because there is no standalone namespace value
metadata or qualified value receiver. `Intl.CollatorOptions` works; `Intl.Collator()` does not.
This is not an emit or runtime-initialization requirement, and it must not be approximated with a
fabricated object or a second resolver.

Valid `export as namespace` publication over either an `export =` or ordinary named-export external
module surface is owned by backlog `15`; its current WU0 witness is the `export =` form. This item
owns `TK1314`/`TK1315` context diagnostics where applicable, `TK2669` for global augmentation
outside an external or ambient module, and `TK2670` for a non-ambient `global` block without
`declare`. Enum/function `TK2567` sites and exact
three-way enum/function/namespace legality belong to backlog `42`; this item owns namespace
placement `TK2434` and a non-permissive function+namespace surface.

## Remaining approach / acceptance

First record and independently review an architecture decision that explicitly supersedes the
ADR-0009 standalone-namespace boundary. It must specify namespace value identity, qualified value
receiver construction, publication timing, cross-space coexistence, and interaction with existing
function/class namespace augmentation. No production implementation starts before that gate.

Then add a spec-first corpus for standalone ambient namespace value calls/member reads, reopenings,
shadowing, missing members, value/type coexistence, and opposite declaration order. Publication
must remain query-free and atomic; qualified lookup must retain public-path no-fallback semantics;
no published `TypeId` may mutate. The pinned proof becomes GO for backlog `14` only when the sole
backlog-43 residual and the owned `deep.Intl.value` mismatch disappear without hiding or
misassigning any remaining diagnostic/incomplete outcome.

Global variables, functions, complete class type/constructor pairs, and cross-file
class/function+namespace value payloads remain [`82`](82-declare-global-value-space.md).
That tail does not block starting `lib.es5.d.ts` loading: its ambient values are top-level
declarations owned by loader `14`, not `declare global` value-space publication. Corpus first;
cross-check tsc 6.0.3 `--strict`. Structurally split the official-suite namespace gate after
landing; never remove a broad regex that also hides external-module or unrelated cases.

## Touch points

`src/binder/namespace.rs`, `src/binder/symbol.rs`, `src/check/checker/namespace_values.rs`,
`src/check/checker/decls/resolve.rs`, `src/check/checker/statements.rs`, and
`src/check/checker/mod.rs`. The proof lives in
`tests/fixtures/lib-es5-6.0.3/readiness.toml` and `tests/lib_es5_readiness.rs`.

## Shipped evidence

The main namespace/merging implementation and review fixes precede WU6. Proof commits `b424e74`,
`5951968`, and `3f641ea` pin and enforce the current NO-GO boundary. Do not close this item, mark
criterion `A-namespaces-declaration-merging` complete, or unblock `14` until the superseding
architecture decision and implementation pass independent adversarial review.

<!-- Origin: completion-roadmap review (2026-07-07); architecture §4.1 requirement, prerequisite of 14. -->
