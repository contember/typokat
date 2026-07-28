---
id: 82
title: declare global value-space publication
blocked-by: []
---

# 82 — `declare global` value-space publication

**Summary.** Publish value-bearing declarations from legal `declare global` augmentations by
extending the shipped compilation-global type-only surface. This is a safe-direction parity tail,
not a `lib.es5.d.ts` loading blocker.

## Problem

The shipped namespace/declaration-merging substrate deliberately publishes only interfaces, type
aliases, and type-only namespaces from `declare global`.
Global variables and functions therefore remain unavailable as values. A global class is deferred
wholesale because its type identity and constructor/static value surface must publish as one atomic
dual-space pair. Cross-file class/function+namespace groups also cannot stage namespace
value/static payloads into a single immutable owner draft. Treating those declarations as ordinary
module locals would leak or split identity; attaching them after publication would violate the
existing class/function freeze barriers.

The pinned ES5 library does not depend on this path. Its ambient variables and constructor values
are top-level declarations loaded by [`14`](14-libdts-loading.md), rather than members of a
`declare global` augmentation.

## Approach / acceptance

Reuse the shipped legal-only augmentation overlays and compilation-global symbol identities.
Stage global variable storage, callable rows, complete class type/constructor/static surfaces, and
admitted class/function+namespace payloads across files before the existing immutable publication
barriers; publish each owner once. Invalid `TK2669`/`TK2670` blocks remain quarantined. Do not add
standalone namespace runtime objects, valid UMD publication, or a second global resolver/`Store`.

The spec-first corpus must cover variable reads, function calls/overloads, class construction and
statics, both namespace-augmentation orders, cross-file reopening and opposite input order,
module-local same-name isolation, recursive references, and proof that body completion cannot
replace an augmented callable/class surface.

## Touch points

Global value binding and storage in `src/binder/`; class/function namespace staging and immutable
publication in `crates/typokat-check/src/check/checker/`; focused cross-file conformance and direct publication tests.

<!-- Origin: WU5 architecture review, sprint-2026-07-15 namespace/declaration merging. -->
