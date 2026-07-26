---
id: 0015
title: Make private collisions snapshot-native publication epochs
status: superseded by 0017
date: 2026-07-23
---

# 0015 — Make private collisions snapshot-native publication epochs

## Context

[ADR-0011](0011-freeze-pinned-default-library-base.md) established the immutable default-library
base and a private same-universe collision path.
[ADR-0013](0013-replay-private-library-collision-closures.md) replaced complete semantic
reconstruction with affected-owner replay, but retained a fresh decode and complete 82-file source
parse/bind for every private run.
[ADR-0014](0014-authenticate-private-replay-prefixes-at-separate-boundaries.md) then separated
semantic-prefix authentication from a continuable source-binder checkpoint. That design is sound,
but its remaining production boundary cannot meet the active sprint's performance contract.

The current quiet-host medians give a collision run about 155–170 milliseconds to remain at least
2× faster than native TypeScript. The measured median-like projection for snapshot startup plus a
small user check is already 145–150 milliseconds; it is a planning lower bound, not a p95 claim.
The sprint's actual decision remains the matched per-row confidence/p95 gate against native
TypeScript. Reconstructing the library binder from all embedded sources adds about 35 milliseconds
even before selective semantic replay, so the previous design is arithmetically outside even the
median budget. Separately, the admitted snapshot's measured cold-start p95 is about 119 milliseconds
against a 120-millisecond startup gate, leaving only about one millisecond of startup margin;
duplicating decode or shifting eager work into initialization is not harmless.

The semantic population is large: 45,925 replay owners in 45,241 SCCs across 82 files. A direct
`Array` collision reaches only 24 owners, while a known wide collision reaches 2,228 owners. This
distribution favors work proportional to the authenticated affected closure, but only if production
does not first scan, clone, parse, bind, or reconstruct the complete population.

Reusing the admitted base is more subtle than sharing immutable `TypeId` rows. A user declaration
can append to a library-owned declaration group, symbol, namespace, value storage, or global object.
The effective global object is especially broad: script and `declare global` values may change it
even under a previously unique spelling, and explicit `globalThis` augmentation must invalidate
every dependent owner. The collision binder therefore needs identity-preserving prefix mutation
semantics without mutating the shared base or inventing a second semantic publication authority.

This is a measured architectural cutover, not a claim that the mechanism is implemented.

## Decision

### Admit one collision capability with the frozen base

Successful production admission of `FrozenLibraryBase` also produces an unforgeable
`AuthenticatedCollisionSeed`. It is a sealed capability constructed only by the admitting
`LibraryBaseProvider`, never from caller-provided rows or independently decoded bytes. It gives a
private collision epoch immutable access to:

- the authenticated Store, interner, binder, declaration-value, runtime-metadata, and semantic
  identity prefixes;
- the exact next-ID counters and canonical source/order metadata; and
- compact authenticated root, owner, dependency, SCC, source-site, binder-provenance, and baseline
  indexes needed for preflight and replay, including owner-indexed baseline event and record slices.

The production collision hot path neither compiles nor rebinds all 82 library sources and does not
independently decode the canonical snapshot. `compile_binder_checkpoint` remains a generation,
reproducibility, differential, and full-source-oracle facility. It is not invoked by a successful
production collision route.

The capability and every private epoch are process-local. They expose no stable serialized identity,
cannot be transferred across processes, and never permit mutation of the admitted shared base.
Every mutable table, suffix, override set, mask, ledger, event replacement, and cache belongs to
exactly one epoch and is never shared with another epoch or the base.
The existing one-process collision containment remains in force until measured memory and
concurrency evidence supports a later change.

### Continue binding through sparse copy-on-write prefix rows

A collision epoch presents the authenticated binder prefix through typed copy-on-write tables.
Untouched prefix rows remain shared and immutable. The first legal change to a library-owned group,
symbol, namespace, value storage, scope, or other mutable binder aggregate creates a private sparse
replacement for that exact row. User declarations and all genuinely new binder objects allocate in
an append-only suffix.

This is not an untyped overlay or a map keyed only by names. Every row kind has a typed override,
prefix references retain their exact authenticated IDs, and suffix allocation starts at the
authenticated counters. Canonical library order remains the prefix order; user `ModuleOrdinal`,
`UnitSlot`, source, declaration, event, and record ordering retain ADR-0011's deterministic
caller-order rules. A user fragment merging into a library declaration receives a suffix-local
`DeclId` while the merged slot keeps its authenticated library identity.

All binder reads and writes in the collision epoch, including reads performed by lookup,
declaration merging, namespace publication, global-object assembly, scope traversal, and
validation, go through one prefix-or-override view. Production code cannot retain or use a raw
base aggregate reference as a bypass. Structural interning probes the authenticated
base buckets first, then the epoch-local suffix buckets; an existing unaffected shape keeps its
base `TypeId`.

### Run route selection, user binding, and replay in one ordered lifecycle

The collision lifecycle is ordered and fail-closed:

1. Parse the complete user project and run the shared external-module classifier, exhaustive
   per-binding-leaf census, and global-object census without creating a binder row, event
   reservation, semantic row, cache entry, or other route-visible mutation.
2. Convert that census through the authenticated root/binder indexes into a conservative allowed
   set of prefix rows and root slots. This is an over-approximation, not the replay seed.
3. Bind the entire user project through the epoch's private copy-on-write binder. Record an exact
   per-binding-leaf mutation ledger for every prefix row created, replaced, or extended and every
   suffix allocation. No library declaration is rebound.
4. Before semantic construction, certify that every prefix mutation in the exact ledger is a member
   of the conservative allowed set and that suffix IDs and counters begin at the authenticated
   boundaries. Any mismatch fails closed.
5. Derive the replay roots from the exact mutation ledger plus mandatory conservative census
   obligations. In particular, every possible script/`declare global` value contribution and every
   explicit `globalThis` extension seeds the authenticated `GlobalObject` owner even when no
   existing root spelling changed. This combined seed can conservatively include extra owners; it
   is not described as exact.
6. Walk the authenticated reverse-dependency index to a fixed point over SCCs, producing affected
   and pending masks.
7. Parse only the embedded library files containing affected source sites and rehydrate only those
   semantic declarations. These ASTs are inputs to lowering only: the authenticated library binder
   prefix is never reconstructed or rebound.
8. Run affected library owners and all user owners through the ordinary checker `Pass`, validate
   the epoch, and publish once atomically.

Destructuring and mixed declaration statements are classified, bound, and certified per binding
leaf rather than by a statement-wide heuristic. The complete user project is bound before replay
closure selection because one leaf can legally alter a library-owned group, namespace, value
storage, scope, or global-object contribution observed by another user file.

### Replay one masked publication epoch

The collision epoch then:

1. marks affected library owners and their terminals pending while retaining immutable unaffected
   prefix terminals;
2. rehydrates only the affected embedded library source sites selected by the closure;
3. lowers affected and new owners through the ordinary checker `Pass`;
4. allocates changed structural and semantic rows only in append-only replacement suffixes; and
5. atomically publishes the complete private epoch only after binder-ledger, owner-mask, event,
   record, identity, reachability, and terminal validation succeeds.

No OXC AST is persisted in the snapshot or frozen base. A bounded implementation may parse an
affected file once and visit only its indexed sites, but retaining ASTs or serialized construction
recipes across runs requires a later decision.

The affected and pending masks select work; they do not publish it. The existing `Pass` remains the
sole lowering and semantic publication authority. Neither the provider, decoder, replay planner,
collision binder, nor an overlay may patch a terminal, identity, declaration value, global object,
or runtime metadata entry directly. There is one atomic publication boundary and no second
publication authority.

Every published root and runtime identity must resolve either to a proven-unaffected immutable
prefix row or to a replacement/new suffix row from the ordinary publication pass. A stale affected
prefix terminal is never query-visible. Old structural rows may remain unreachable; no reachable
private result may mix old and replacement meanings for one affected identity.

Every epoch starts with empty query, evaluator, relation, projection, substitution, and flow caches,
including empty cycle/provisional state. It never mutates a cache owned by the base or another
epoch. An immutable cache entry may be imported only when its complete dependency set is
authenticated and proven disjoint from the affected and pending masks; otherwise it is recomputed
locally. Every cache lookup checks the masks before accepting a prefix-derived answer. A cache hit
cannot make a pending owner observable, skip an ordinary `Pass` publication phase, suppress a
reserved event, or bypass record cardinality validation.

### Replace affected baseline events and records sparsely

The authenticated seed indexes baseline library events and records as deterministic slices by
replay owner. Unaffected owners retain their immutable slices. Selecting an affected owner installs
an epoch-local sparse tombstone for its complete baseline slices before semantic replay, so no
affected baseline event or record is visible while that owner is pending.

Ordinary `Pass` replay writes append-only replacement slices for affected owners and ordinary new
slices for user owners. Finalization validates each replacement against its owner and baseline
cardinality/digest contract, then merges retained unaffected slices, affected replacements, and user
records in exactly the order produced by the normalized full-source oracle: canonical library-owner
order for library slices and the existing four-key replay order for user records. A baseline row is
either retained once or tombstoned and replaced; it can never be emitted alongside its replacement
or become visible through a cache hit.

Tombstones, replacement indexes, and validation work are sparse over affected owners and new
output. Final rendering may perform work proportional to records that are actually part of the
public result, but hidden baseline records are not a reason to scan all owner slices. The hot path
may not allocate, clear, reserve, or linearly initialize a base-sized event/record bitmap or table.

Physical collision work must be independent of the total 45,925 owners, 45,241 SCCs, and 82 source
files. Route execution may index directly into the admitted immutable structures, but it may not
linearly scan, clone, decode, parse, bind, reserve, lower, validate, or publish the complete base.
Its variable work is bounded by the derived replay seed, exact mutation ledger, affected closure,
affected source sites, replacement rows, user suffix, and emitted records. The 24-owner `Array`
case must remain correspondingly small. The 2,228-owner wide case is a required constant-factor
stress witness, not permission to fall back to base-sized work.

The same bound applies to auxiliary state: the epoch cannot allocate, zero, reserve, or initialize
bitsets, maps, vectors, terminal tables, cache domains, or validation ledgers at base cardinality.
Sparse or chunked structures must allocate in proportion to touched rows, affected owners, imported
proven-unaffected cache entries, and the user suffix.

### Keep the complete source compiler as containment, not production success

The full embedded-source compiler remains complete and executable as:

- the canonical artifact and authenticated index generator;
- the reproducibility oracle for binder and semantic prefixes;
- the differential correctness oracle for every collision family; and
- a fail-closed containment path for an unmodelled dependency, unsupported replay owner, invalid
  mutation ledger, or other internal mismatch.

A collision family is admitted only after a generation-time and adversarial differential proof.
For every direct root, declaration-merge, namespace, value/function/class, destructuring, unique
global-object, explicit `globalThis`, module `declare global`, and wide-closure family, the proof
runs both caller orders where more than one user input exists. Before either candidate begins
semantic construction, it compares:

- continuation from the decoded copy-on-write seed; and
- continuation from the exact source-generated library checkpoint through the authoritative
  combined project binder.

For the same input order, the proof requires equal normalized binder shape and exact authenticated
prefix boundaries, suffix IDs, next-ID counters, per-binding-leaf mutation ledger, scopes, roots,
group/namespace membership, and value storages. Opposite orders must satisfy ADR-0011's normalized
per-source equivalence while preserving each invocation's specified caller-order ordinals. Any
mismatch rejects the artifact or collision family before semantic replay; a later equal diagnostic
set cannot excuse binder drift.

A fallback result may preserve correctness during development or controlled containment, but any
approved collision or fanout benchmark sample that compiles/rebinds the complete library,
independently decodes the snapshot, or takes the full-source fallback is an automatic NO-GO. Route
receipts must make this observable.

This narrowly supersedes:

- ADR-0011's requirement that a private collision own a complete freshly compiled
  library-plus-project binder and interner;
- ADR-0012's collision-specific requirements that `LibraryCompiler` compile packaged sources for
  every private collision, that those sources be rebound with the entire user project before
  lowering, and that the admitted shared snapshot never seed collision execution;
- ADR-0013's requirement that each private run independently decode the snapshot and parse/bind the
  complete 82-file profile; and
- ADR-0014's requirement that production replay authenticate a freshly compiled continuable
  library-binder prefix before user binding.

Their unaffected constraints remain binding: the exact embedded profile, conservative
selection-before-mutation, same-universe declaration merging, private identity isolation, one
compiler/checker lowering authority, append-only structural identity, deterministic ordering,
event ownership, authenticated dependency closure, library-ledger validation, memory ceiling,
full-source oracle, and prohibition on an unaugmented false-clean base. In particular, ADR-0012's
canonical artifact admission and packaging, explicit reproducible generation, authoritative source
compiler, typed route-unaware decoder, exact asset retention, schema/identity pinning, and
no-runtime-regeneration/fail-closed initialization rules are unchanged. Packaged sources remain
required for generation, differential proof, the full-source oracle, and containment, rather than
for every successful production collision.

### Defer lazy and parallel execution until they preserve this boundary

Runtime demand-lazy admission of the default library is not selected. It would move work across the
measurement boundary, complicate complete ambient semantics and diagnostics, and consume an already
narrow startup margin without solving same-universe collisions. A later lazy design is admissible
only if the base remains one complete authenticated semantic product and misses cannot introduce a
second publication path.

Parallelism is an execution option, not the explanation for meeting the single-collision budget.
Independent user checks may later run concurrently over the immutable admitted seed, and affected
parsing or lowering may be parallelized only with deterministic IDs, records, failure precedence,
and one atomic `Pass` publication result. The collision row must first satisfy its CPU-work and
physical-work bounds without relying on unrelated fanout.

## Consequences

- A successful collision no longer pays base-sized decode, source parsing, or binding before doing
  useful affected work, making the sprint's collision budget arithmetically attainable.
- The base/provider API gains a sealed collision capability and the binder gains typed sparse
  prefix-row replacement semantics. These are deliberate architecture changes with broader
  implementation and review cost than the fresh-universe design.
- Snapshot generation must authenticate enough compact binder and source-site provenance to prove
  sparse continuation without storing ASTs, and every admitted collision family gains a
  pre-semantics differential binder proof against the exact source checkpoint.
- Correctness validation becomes stricter: preflight, the exact binder mutation ledger, the affected
  closure, cache dependencies, sparse event/record replacement, replacement reachability, and
  ordinary publication must agree before any private result escapes.
- Memory and work scale with the affected closure and user suffix rather than the complete library,
  but wide collisions still require durable memoization and low per-owner constants.
- Full-source compilation remains maintained and tested even though it is excluded from successful
  production benchmark routes.
- Lazy loading and parallelism remain available as later measured optimizations; neither may weaken
  admission, identity, ordering, or sole-publication-authority constraints.

## Falsifiability

Reject or supersede this architecture if any of the following occurs:

1. A snapshot-native collision differs from the normalized full-source oracle for any legal merge.
2. The exact mutation ledger contains a prefix row absent from conservative preflight, or an actual
   changed root/row is absent from the ledger.
3. A required pre-semantics differential proof disagrees with the exact source checkpoint on
   normalized binder shape, prefix/suffix identity, counters, ledger, scopes, roots, or value
   storages.
4. The implementation performs user binding before conservative route selection, computes replay
   closure before the complete user mutation ledger is certified, or rebinds any library declaration
   in the production collision route.
5. A published root, global-object member, terminal, runtime identity, declaration value, or record
   reaches stale affected state.
6. Correctness requires the provider, replay planner, binder overlay, or snapshot decoder to publish
   semantic rows outside the ordinary `Pass`.
7. A collision hot-path sample independently decodes the snapshot, invokes
   `compile_binder_checkpoint`, parses or binds all 82 files, scans/clones all 45,925 owners or
   45,241 SCCs, or otherwise performs work proportional to the complete base.
8. The direct `Array` case does not remain proportional to its 24-owner closure, or the 2,228-owner
   wide witness exceeds the sprint budget because of base-sized rather than closure-sized work.
9. Any required collision or fanout sample takes the full-source fallback, lacks an unambiguous route
   receipt, exceeds the 512 MiB ceiling, or misses the sprint's ≥2× confidence/p95 gate.
10. Opposite user input order changes normalized per-source semantics, identity shape, diagnostics,
   or records beyond ADR-0011's permitted caller-order ordinals.
11. Sparse prefix replacement permits shared mutation, cross-project leakage, raw aggregate access
    that bypasses the prefix-or-override view, non-deterministic ID allocation, or a private ID that
    is interpreted outside its process-local epoch.
12. A per-epoch cache is shared or accepts an entry with an affected/unproven dependency, or a cache
    hit bypasses a pending mask, publication phase, event reservation, or record check.
13. An affected baseline event/record remains visible, appears beside its replacement, loses
    deterministic full-source ordering, or requires scanning hidden baseline output.
14. Selective parsing cannot cover every authenticated affected owner without persisting an AST or
    reparsing the complete profile.
15. Startup work added by the collision capability causes the 120-millisecond p95 admission gate to
    fail.

## Alternatives considered

### Retain the fresh decode plus complete source-binder checkpoint

This is the ADR-0013/0014 design and remains a valuable differential construction path. It loses as
the production hot path because the measured median-like 145–150 millisecond planning lower bound
plus roughly 35 milliseconds of complete source binding already exceeds the 155–170 millisecond
median budget before semantic replay. The matched confidence/p95 benchmark remains the actual gate.

### Rebuild the complete library for every collision

This is the simplest same-universe correctness oracle and remains the containment fallback. Its
approximately 2.6-second measured cost cannot satisfy the performance contract.

### Patch decoded semantic terminals directly

This could be mechanically smaller than ordinary replay, but it creates a second publication
authority and can expose stale identities, declaration values, runtime metadata, or global-object
members. It is rejected.

### Persist complete ASTs or construction recipes

This avoids selective reparsing but violates the AST-free frozen product, inherits OXC ownership
constraints, enlarges startup state, and creates a second semantic model to authenticate. Indexed
source sites make it unnecessary unless future measurements falsify selective parsing.

### Make the default library demand-lazy

This may improve some startup workloads, but it does not by itself preserve complete ambient
diagnostics or solve global declaration merging. It is deferred until a concrete design preserves
one admitted semantic product and demonstrates an end-to-end benefit.

### Use parallelism to hide complete-base work

Parallel parsing, binding, or fanout may reduce wall time on a many-core host, but it does not remove
the redundant CPU, memory, initialization, or one-process containment cost and cannot rescue the
single-collision arithmetic. It remains a follow-on optimization over the snapshot-native epoch.
