---
id: 123
title: Prepare contextual class initializer publication inputs
blocked-by: []
---

# 123 — Prepare contextual class initializer publication inputs

**Summary.** Supply effective contextual inputs to class initializer publication without exposing
partial class surfaces. Effort XL; release-blocking semantic work, separate from bounded `as const`.
The delivery boundary is [ADR-0036](../decisions/0036-bound-contextual-class-publication-migration.md),
with the conservative compatibility amendment in
[ADR-0038](../decisions/0038-reject-inconsistent-contextual-class-publication.md).

## Problem and verified witnesses

The query-free class inferer receives raw lowered annotation IDs. A projected property can retain
`keyof` or a conditional rather than its effective literal context. The distinction matters:
`"alpha"` under a finite key union must remain non-widening, while a broad string context widens it.

- `tests/cases/b123_class_contextual_publication/external_key_context.ts`: strict tsc accepts the
  initializer and rejects copying then writing another key. The pre-feature checker and frozen
  R1a, rejected-container and ADR-0035 candidates instead report an initializer TK1360.
- `tests/cases/b123_class_contextual_publication/self_dependent_context.ts`: strict tsc rejects the
  final write with TS2322; the same four checker versions miss it entirely.
- `tests/cases/b44_as_const/publication_class_self_context_control.ts`: the broad conditional
  counterpart is clean in all four versions and tsc. Do not claim unsupported admission preserves
  its behavior.
- `tests/cases/b44_as_const/publication_class_context_controls.ts`: broad index keys, readonly
  annotations, declared returns, regular sources and unused deferred properties must not regress.
  The whole file is clean in the three feature candidates; the pre-feature binary is incomplete
  at its `as const` expression, so it is not a whole-file pre-feature compatibility receipt.

Completed external interface objects do not make every nested operation ready. A conditional can
require the same class whose field is being inferred. The normal query helper also requires class
and alias registries staged only after class drafts finish.

ADR-0038 explicitly accepts conservative initializer errors until this item supplies correct class
publication inputs. The active B44 `publication_class_conservative_{external,self}.ts` guards are
both tsc-clean but report one TK1360 with the retained-source candidate. Exported/external and
self-dependent contexts are affected; removing downstream uses does not remove the false positive.
The broad controls stay clean. These new errors are not restored legacy behavior or completion of
the downstream TS2322 witnesses above. Ready rest-parameter projection bugs are not covered here.

## Approach / acceptance

Design the semantic preparation and construction schedule first. An owned initializer-context
payload and a genuine ready-input query scope may cover external contexts, but cannot by themselves
solve self-dependency. Retain annotation inputs once; do not discover or lower declarations twice.
Use the existing coordinator rather than reproducing `keyof` or conditional evaluation.

Keep original annotation authority separate from publication and shaping views. Consume only
contextual positions reached by admitted source syntax; unused deferred properties must not force
demand. Preserve symbolic parameters rather than replacing them by constraints. Prove context
independence before skipping preparation for mode-retaining or regular sources.

Acceptance includes the preserved fixtures, aliases and reversed declaration order, self/mutual
class dependencies, ready broad/finite alternatives, negative writes, exactly-once effects and
typed failure rollback. Construction remains query-free, complete class surfaces publish atomically,
and failed preparation promotes no query-cache writes. Any needed change to those invariants requires
its own explicit architectural decision, not a fallback or an inferred authorization from this item.

Do not charge semantic scheduling to [119](119-class-publication-preparation.md), whose acceptance
requires unchanged behavior and publication order. Coordinate with
[76](76-lazy-value-type-resolution.md) only after demonstrating the actual dependency.

## Touch points

`checker/classes/publication.rs`, initializer annotation seeding, owned construction inputs,
`checker/classes/initializer.rs`, and the existing coordinator's preparation environments.
