---
id: 0027
title: Produce the library census from the project publisher
status: superseded by 0028
date: 2026-08-27
---

# 0027 — Produce the library census from the project publisher

## Context

[ADR-0018](0018-pin-library-owned-records-as-a-named-census.md) makes the named library-owned
record multiset a suite authority, not user-facing output or retained runtime state. After complete
recursive-alias evaluation, [ADR-0026](0026-retain-library-outcomes-after-alias-evaluation.md)
restored the current authority to 558 rows: 267 diagnostics and 291 incomplete surfaces.

WU4 first tried to compare that authority through the production project-local lowerer. The
lowerer must not publish a partial member or insert a recovery `any`, `error`, `unknown`, or
`never` into a Ready surface. Its incomplete staging and declaration-validation path were not yet
implemented, so a partial force-all experiment produced fewer records. The sprint treated that
implementation gap as evidence that sound publication and record inspection needed different
semantic evaluators.

Commit `0eb9344` consequently required a test-only `ForceAllRecordAudit` with its own `Store`,
interner, alias registry, query state, relation scheduling, recovery behavior, and recipe lowerer.
The implementation reproduced all 558 rows, but it grew into a second semantic evaluator with no
dependency on the production lowerer. It could remain green when production terminals or probes
were wrong, and after deleting the legacy complete-source route it would have become both the
fixture's producer and its only oracle.

The 267 diagnostics do not justify that split. They are principally relation-model outcomes over
supported declarations, while the incomplete rows identify unsupported source sites. Both belong
to validation work driven by the production publisher. A separate recovery evaluator tests its own
copy rather than the lifecycle being shipped.

## Decision

There is one producer for the future library-owned census: the project-local production publisher.

Test-only `force_all` expands the ordinary session's demand set to every admitted product group.
It uses the session's existing interner, semantic store, lowerer, SCC coordinator, terminals,
aliases, validation obligations, and durable cache. It does not create a scratch semantic universe
or lower the product a second time.

The production lowerer stages one typed incomplete record for every unsupported site it visits.
When a member is unsupported, it finishes the record-producing walk for that member and then omits
the whole member. It never publishes a partial signature and never substitutes a recovery type
into a Ready surface. The ordinary production validation path evaluates declaration heritage,
type-argument constraints, qualified paths, and other library validation roots affected by the
final project-local declaration universe. `force_all` schedules the same obligations for every
Ready library group.

The force-all receipt contains production-staged incompletes, production validation diagnostics,
and one normalized terminal row per product group: Ready with its observable surface census, or
Unavailable with its typed reason. Semantic probes are projected from those published terminals;
they are not produced by a fresh lowering run.

The current 558-row ADR-0018 census remains the starting baseline, not a number that a second
evaluator must reproduce. Acceptance is a named, attributed multiset comparison:

- a retained row must match exactly;
- every removed row must name production evidence: an omitted member with its staged incomplete
  rows, a typed Unavailable terminal, or a documented single-report collapse;
- every added diagnostic must have a named cause and a documented divergence;
- any unattributed removal fails.

The reviewed comparison becomes ADR-0018's authority at cutover. Before WU7 removes the legacy
compiler, `LibraryRecordCensus::compile_packaged_profile()` moves its test-only producer to the
project-local session. Ordinary checks still retain and report no library-owned record.

This decision narrowly supersedes ADR-0026's requirement that `ForceAllRecordAudit` reproduce the
558-row authority. ADR-0026's alias semantics, named outcomes, and 558-row starting census remain
accepted. It also supersedes the force-all audit boundary added to the active sprint by `0eb9344`.

## Consequences

- A production lowering, SCC, validation, or terminal regression changes the force-all receipt.
- The suite no longer maintains a second recipe lowerer, heritage checker, or recovery universe.
- Record staging and validation obligations become production capabilities required by ordinary
  augmented projects, rather than audit-only behavior.
- The final census may differ from 558 only through a reviewed named comparison. Count movement
  alone is neither accepted progress nor an automatic regression.
- Product-completeness tests remain at extraction time. Targeted canonical and metamorphic tests
  retain syntax facts such as an `abstract` constructor bit without introducing a second semantic
  evaluator or a second TypeScript printer.
- Test-only force-all does more work than an ordinary request, but it does so through the same
  owners and state transitions.

## Alternatives considered

**Retain and deduplicate `ForceAllRecordAudit`.** Rejected because sharing stateless helpers reduces
text duplication but leaves two independent recursive state machines. Every semantic fix can still
drift between them, and the audit still does not observe production publication.

**Use one lowerer with an audit or recovery mode.** Rejected because a mode that changes failure
and recovery semantics can publish a surface that ordinary sound publication would reject.

**Discard the 558-row baseline.** Rejected because ADR-0018's named rows remain valuable model-gap
evidence. The producer changes; the evidence is not silently thrown away.

**Require exact 558 from production without attribution.** Rejected because model improvements,
single-report normalization, and typed unavailability can move the set legitimately. ADR-0018
requires named causal review, not preservation of an unexplained integer.
