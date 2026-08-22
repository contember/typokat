---
id: 0023
title: Own default-library semantics per project
status: proposed
date: 2026-08-22
---

# 0023 — Own default-library semantics per project

## Context

Production currently has three default-library semantic lifecycles: a frozen shared base for
ordinary projects, a private sparse collision epoch for selected augmentations, and complete-source
compilation for standalone checks and fallbacks. Their shared semantic prefix was built before the
complete user declaration set existed.

The project-local index feasibility run exposed the ownership risk. Correcting ordinary `symbol`
lowering made the packaged `Reflect` surface available while the candidate and frozen control had
different semantic construction paths. The removed prototype reported 44 changed DOM record owners
but retained no named rows. A WU0 reconstruction found the packaged 559-record census unchanged
under a consistent fix and found zero missing/changed DOM TK2344 rows under a controlled
old-base/new-candidate symbol toggle. The historical count is therefore not an oracle. The durable
finding is that multiple construction paths made it impossible to attribute the disagreement.

Typokat still needs repeated-project performance. It may therefore share immutable declaration
facts, but it must derive meaning only after each project's complete ambient/global declaration set
is known. The proposal remains unaccepted until its semantic, ownership, and performance gates all
pass and the old production lifecycle is deleted.

## Decision

We will share declarations, not semantic meaning.

The process-wide default-library product may contain exact profile/source identity, owned names,
declaration forms, stable source sites, symbolic syntax recipes, canonical order, and dependency/SCC
facts. It contains no OXC AST or arena reference, `Binder`, binder checkpoint, `Store`, `Interner`,
universe-local ID, semantic terminal, global-object product, event, cache, replay owner, baseline
fingerprint, or mutable cell. It is derived from the one canonical declaration inventory while the
owning AST is alive; it does not introduce a second semantic AST walk or order authority.

Each project will:

1. parse every user source once;
2. instantiate a fresh binder from the immutable library declaration product;
3. bind all user scripts and module `declare global` fragments into that binder;
4. finish canonical global groups, namespace/value attachments, and global-object contributors;
5. allocate and publish semantic state only on project-local demand.

All binder and semantic identities are project-local. Integer `TypeId` equality remains structural
equality only inside one project-local universe.

Only the checker `Pass` may lower and publish semantic state or diagnostics. A query reserves its
complete dependency SCC before lowering any member. Terminal state is
`Unresolved -> Resolving(SCC) -> Ready | Unavailable`. Provisional results enter no relation,
evaluator, projection, flow, diagnostic, or global-object cache. A `force_all` operation may exist
only as a test oracle.

Standalone and repeated checks will use one `CheckerSession` lifecycle. They may share the exact
immutable declaration product and nothing mutable. Production will have no library collision
classifier, sparse route, complete-source fallback, collision-free semantic route, or call-order
switch.

The old lifecycle may coexist before cutover only as a test-only shadow/control. No public flag,
error fallback, name-based branch, or performance fallback may select between the architectures.
Cutover accepts this ADR and deletes the old lifecycle in the same reviewed change. A failed
candidate leaves this ADR proposed and the current production route intact; it does not authorize a
hybrid.

## Acceptance and falsification

Reject the candidate or leave this ADR proposed if any of these is true:

- the shared product needs retained AST, universe-local state, a second declaration walk, or a
  second semantic publisher;
- project-local results differ from TypeScript 6.0.3 or the complete-source oracle on the frozen
  symbol/Reflect, named library-census, augmentation, merge, order, global-object, recursion,
  module, parse-failure, and isolation corpus;
- `force_all` differs from ADR-0018's named census or complete-source semantic probes;
- source or demand order changes normalized results, provisional state becomes observable, or any
  mutable identity/cache/augmentation leaks between projects;
- standalone and repeated routes differ semantically;
- any production hybrid or collision-name branch remains.

The performance gate uses authenticated comparable lifecycles. It covers cold `fast-clean`,
`fast-errors`, `collision`, and `fanout`, plus isolated in-process 32- and 687-project panels. Every
panel must have typokat/tsgo median and p95 speedup, and a one-sided 95% bootstrap lower bound,
strictly greater than `1.00`. Typokat median peak RSS must be no greater than tsgo on every binding
panel and every typokat sample must remain below 512 MiB. The candidate must also beat the frozen
current-provider 687-frame wall time and peak RSS. One, two, and 32 concurrent sessions must remain
deterministic and isolated.

The repeated comparator creates one isolated native tsgo Program/checker per frame. It may share
only immutable source/parse products that tsgo's real implementation shares. Repeated tsgo process
startup is not an eligible comparison.

## Consequences

- A library-name collision stops being a semantic lifecycle concept.
- Project binding repeats work that the frozen base currently shares. Demand-driven semantic SCC
  publication must recover that cost without weakening eager declaration assembly.
- The symbolic declaration product becomes a new authenticated architecture boundary.
- The complete-source route remains the simple semantic oracle until cutover.
- A performance miss requires profiling and optimizing this ownership model, or a new ADR. It does
  not permit frozen semantics or sparse replay to return through a fallback.

At cutover this ADR will narrowly supersede ADR-0011's frozen semantic base, prefix/delta, singleton
semantic provider, collision preflight, and private-rebuild boundaries; wholesale supersede
ADR-0013, ADR-0014, and ADR-0020; narrowly supersede ADR-0017's eager complete semantic compilation
into `FrozenLibraryBase`; and narrowly supersede ADR-0021's split semantic lifecycle. ADR-0017's
exact source/profile authority and no-artifact rule, ADR-0018's named census, ADR-0022's one
manifest/inventory authority, and the unaffected parsing/report-order contracts remain binding.

## Alternatives considered

- **Repair sparse replay.** It preserves more warm reuse but retains the pre-project semantic base,
  multiple publishers, collision classification, and replay correctness surface.
- **Compile complete source eagerly for every project.** It is the strongest simple oracle, but it
  repeats the entire semantic library even when a project demands only a small part.
- **Keep frozen semantics with project overlays.** It still creates two meanings and two publication
  paths for one ambient declaration universe.
- **Load library sources per demanded name.** It cannot know the complete same-name ambient merge
  group before the first semantic query.
