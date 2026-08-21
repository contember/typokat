---
id: 119
title: Separate owned class preparation from atomic publication
blocked-by: []
---

# 119 — Separate owned class preparation from atomic publication

**Summary.** Extract only owned preparation results from `publish_class_surfaces` and retain one
visible atomic commit facade. Non-blocking checker maintenance, effort M.

## Problem

`publish_class_surfaces` coordinates interface/class group preparation, class member lowering,
namespace payloads, SCC publication, metadata commitment, group freezing, and validation in one
large method. Narrow class work must navigate the whole transaction.

Atomic class publication is a binding invariant. A generic phase object that owns or retains
`&mut Pass` would spread publication-sensitive authority and can become self-referential with the
interner/binder it borrows. `lower_class` is also a cohesive cross-element state machine: overload,
accessor, constructor, initializer, and poison state cross syntax elements. Splitting it by element
kind would create a larger mutable context rather than a smaller design.

## Approach / acceptance

Identify preparation steps that can return private owned values with no retained `Pass`, `Resolver`,
or factory borrow. Introduce a `PreparedClassPublication`-style result only for those values. Keep
`publish_class_surfaces` as the visible dependency-order loop and atomic commit sequence; any step
that emits an event, publishes a class/group, or depends on a live checker borrow remains in that
facade.

Before implementation, write the proposed result's field and ownership list and mark the exact
creation/destruction boundary of every `Pass` borrow. Reject an extraction that needs callbacks, an
ordered effect replay enum, wider semantic field visibility, or more mutable context than the
current input/result records.

Acceptance requires identical class/type-group publication order, event ownership, diagnostics,
nominal identities, retained callable rows, namespace attachment, and cache admission across class,
interface, recursion, overload, accessor, static, generic, and attached-namespace fixtures. Do not
split `lower_class` as part of this item.

## Touch points

`crates/typokat-check/src/check/checker/classes/publication.rs`, class/type-group publication tests,
namespace attachment tests, event/replay audits, and conformance/official-suite class families.

<!-- Origin: 2026-08-20 development-friction audit; checker decomposition refutation. -->
