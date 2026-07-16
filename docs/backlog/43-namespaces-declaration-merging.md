---
id: 43
title: Namespaces (type side) + declaration merging
---

# 43 — Namespaces (type side) + declaration merging

**Summary.** Type-side namespaces and declaration merging are substantially shipped. One
implementation remains before backlog `14` may start: standalone instantiated namespace value
metadata and ordinary qualified value access under
[ADR-0010](../decisions/0010-publish-instantiated-standalone-namespace-values.md). The pinned WU6 proof is
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

ADR-0010 is accepted and narrowly supersedes ADR-0009's standalone type-container-only boundary.
Every instantiated standalone `NamespaceId` owns one stable `ValueStorageId` and one complete
immutable structural `ObjectType` shared across reopenings. Nested instantiated namespaces publish
bottom-up; non-instantiated namespaces have no value. Construction is query-free and atomically
reaches typed `Ready | Unavailable`; ordinary symbol value lookup consumes only terminal
publication. Function/class namespace groups retain their existing owner. Exported `const` is
readonly; exported `let`, `var`, function, class, and nested namespace properties are mutable. No
runtime/emit/loader or `declare global` value support is added.

Add and separately commit the strict-tsc corpus before implementation: namespace root reads and
aliases, calls/member reads, reopenings/opposite orders, equal-shape distinct storage identities,
nested/dotted bottom-up publication, ambient/explicit/private visibility, non-instantiated no-value,
missing/private diagnostics, exact mutability, and function/class existing-owner controls. Direct
gates must prove group-level instantiation, one storage/type per namespace, complete parent
unavailability, zero queries/partial publication, and deterministic event order. Root
assignment/update (`N =`, `N++`, `++N`, `N--`), root call/new, and pure type-only root
alias/read/call/new/member access require strict-tsc parity or precise documented non-43 owners;
ordinary value machinery is not accepted without those negative gates. The unavailable ledger is:
type query `52`, inferred initializer/function return `76`, enum `42`, import/import-equals `15`,
duplicate `18`, TDZ/use-before-declaration `47`, and class/static dependency cycle implemented or
owned by `76` by default. Every cause withholds the whole parent and leaves no broad `43` fallback.
An exact two-file forward/reverse `check_project` gate must preserve storage/type/publication,
diagnostics, and the full EventStore tuple order without deduplication, truncation, suppression, or
completion drift. The pinned proof
becomes GO for backlog `14` only when the sole backlog-43 residual disappears,
`deep.Intl.value` becomes its expected `TK2322`, and every non-43 outcome remains explicit.

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
criterion `A-namespaces-declaration-merging` complete, or unblock `14` until ADR-0010's spec,
implementation, repeated readiness proof, and independent adversarial review pass.

<!-- Origin: completion-roadmap review (2026-07-07); architecture §4.1 requirement, prerequisite of 14. -->
