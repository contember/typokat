---
id: 0014
title: Authenticate private replay prefixes at separate boundaries
status: accepted
date: 2026-07-23
---

# 0014 — Authenticate private replay prefixes at separate boundaries

## Context

[ADR-0013](0013-replay-private-library-collision-closures.md) accepts a fresh private snapshot seed,
complete source parse/bind, and selective semantic replay. Its initial wording said the fresh
combined binder reproduces the complete snapshot ID prefix before user binding. That conflates two
independent authorities: binding allocates binder identities but does not reproduce `TypeId`,
`TypeParamId`, or `ClassId` rows. Those semantic prefixes come only from authenticated snapshot
decode.

The distinction is observable in the current binder lifecycle. Library modules allocate in
canonical file order, while dormant value storages and compilation-global state are finalized later.
Binding a user module before the exact library-finalization checkpoint can shift library-owned IDs.
Comparing the final combined binder is also too late: a legal user merge deliberately appends a new
declaration fragment to a library-owned group, symbol, or namespace.

The same review found a second ambiguity. The snapshot decoder must not withhold affected terminals
based on a route. Route-sensitive decoding would make it a second semantic authority. Affected
semantics also cannot rewrite decoded Store rows or interner keys without violating immutable
structural identity.

## Decision

Private replay authenticates two prefixes at two separate construction boundaries.

1. Fresh canonical snapshot decode authenticates and reconstructs the complete semantic prefixes:
   Store/interner rows, type-parameter and class counters, published terminals, declaration values,
   checker runtime metadata, semantic identities, and the replay index. Decode is route-unaware and
   produces the same complete product as ordinary base initialization.
2. Fresh source parse/bind first reaches a library-only continuable checkpoint through the exact
   generation binder path. It performs all dormant-storage and compilation-global finalization, then
   byte/digest-compares the canonical binder section and root-slot projection with the admitted
   snapshot and compares every binder/source/value-storage counter. At this checkpoint there are
   zero user binder rows, user event reservations, and semantic allocations.
3. Only after both branches join and both prefix checks pass may the compiler resume the same binder
   with the complete user project. New declarations use suffix IDs. A colliding declaration's new
   `DeclId` is suffix-local while its resolved type/value/namespace slot is the existing library
   slot in that private binder.

The compiler consumes the fully decoded semantic product into private construction state. It marks
the authenticated affected-owner closure pending and inherits unaffected terminals. Existing decoded
Store rows and interner keys remain immutable. Replayed semantics allocate append-only replacement
rows where a structure changes, and ordinary compiler publication makes those replacements
reachable from affected terminals. Old rows may remain only when no published root, terminal,
runtime metadata entry, or semantic identity can reach them.

The decoder does not select owners, filter terminals, patch identities, or observe user syntax.
`LibraryCompiler` and the checker `Pass` remain the only code that converts the replay mask plus ASTs
into a complete published environment.

This narrowly supersedes ADR-0013's overbroad “complete ID prefix” binder claim and clarifies its
semantic-seed wording. It does not change ADR-0013's replay algorithm, authenticated dependency
index, conservative routing, containment, full-source oracle/fallback, or performance falsifiers.

## Consequences

- The private path needs a new continuable library-only binder checkpoint shared with snapshot
  generation, rather than reusing the collision-free WU4 continuation.
- Prefix validation becomes exact and early enough to reject ID drift before user mutation.
- A legal merge may mutate the fresh private binder's group composition, but never decoded structural
  type identity.
- Optional overlap of fresh decode and library parse/bind remains safe because the branches meet only
  at authenticated prefix comparison.

## Alternatives considered

### Compare only the final combined binder

Rejected because correct user merges intentionally change prefix-owned group composition, making a
final byte comparison both too late and impossible.

### Let fresh binding validate semantic IDs

Rejected because binding does not allocate or reconstruct Store, type-parameter, class, publication,
or semantic-identity rows.

### Decode only unaffected terminals

Rejected because route-sensitive semantic reconstruction makes the decoder a second authority and
cannot be validated as the same canonical snapshot product.
