---
id: 0042
title: Qualify deferred class member results
status: accepted
date: 2026-09-08
---

# 0042 — Qualify deferred class member results

## Context

Eight required as-const class-reference cases have a ready class identity during initializer
construction but no published member meaning. Existing class SCC publication accepts complete
symbolic field types and follows their class dependencies. It does not certify member existence
or access permission. Reading mutable drafts or scheduling another initializer pass would violate
the construction boundary.

Ordinary indexed types and references inside literal-owned const syntax have different literal-mode
operations. An unresolved selector cannot stand in for the exact regular literal merely because its
current downstream behavior happens to reject a wider assignment. The user approved an immutable
result-qualified selector, evaluated by the existing post-publication coordinator.

## Decision

### Construction and admission

The existing query-free initializer walk may construct a `DeferredIndexedAccess` for a static,
nonoptional class-member reference inside literal-owned const syntax. Its immutable result operation
is `Preserve` or `RegularLiteral`. Ordinary indexed-type constructors retain `Preserve` and their
existing policies. Direct assertion operand admission remains separate from this nested producer.

The existing scope-aware annotation reservation may seed a direct non-generic class reference from
its authenticated canonical class header. The reservation owner proves the declaration's parameter
list is genuinely empty; an empty supplied argument vector alone is insufficient. Preserve successful
annotation-cache reuse and existing non-admitted/failed fallback. The pure factory does not discover
declarations, read drafts, or perform queries.

Selectors enter the actual instance template. Existing identity traversal follows both operands and
class arguments; the SCC finisher publishes complete symbolic surfaces atomically. Both declaration
orders and non-heritage SCCs use this path. No receipt-only roots, extra source walk, initializer
completion queue, general class readiness or class rescheduling is introduced.

### Identity and transport

The operation participates in structural hash and collision equality. Preserve it through substitution,
inference/mapped rewrites, query rebuilding, derivation, cloning and frozen-store forks. It adds no
TypeId edge; reference census still visits both operands.

Both unresolved-selector identity shortcuts require equal operations before comparing operands:
`contextual_deferred_indexed_accesses_are_identical` in the relation engine and the deferred arm of
`identical_retry_attempt` in the coordinator. This includes alpha-aligned indices and both directions.
Different operations do not prove unequal successfully evaluated values; existing regular/widening
relation equivalence still applies after evaluation.

### Qualified completion

For `RegularLiteral`, the existing query planner completes selected outer operations and union
contributions, combines completed IDs, then shallowly regularizes the result. Both shallow and rebuilt
selector paths record their result only after this completion succeeds. Following overlays or applying
regularization once to an unresolved result is insufficient.

Stop this operation at object, function, array, tuple, readonly and class containers. Preserve their
selected TypeIds and mutable children without evaluating unrelated fields. A recursive selected
container is a stopping boundary, not itself a selector-computation cycle. Keep canonical enum member
and family identity and existing reserved-enum preflight.

Use existing planner projection/evaluation machinery, bounded work accounting and typed frontier/taint
handling. A repeated or unresolved qualified computation cannot become a successful completed identity
or durable memo entry. Self/mutual selected-value cycles and budget exhaustion retain typed failure;
failed attempts promote no evaluator, projection or relation writes. Retrying uses existing query
transaction boundaries. There is no second evaluator or construction-time query.

Every `Preserve` route retains its existing behavior, including object/index type-parameter handling.
Outer `Ready(object)` proves only outer shape; selected-leaf completion remains separately demandable.
Publication preflight validates reachable publication state and does not replace selector completion.

### Lexical effects and validation

The existing Body owner checks missing/private/protected access exactly once, even in unused classes.
The indexed kernel remains visibility-blind; a recovered selected type never authorizes source access.
Retain original diagnostic origins, scope restoration and ADR-0037 construction-source selection,
including its lexical-failure guard, annotated-field authority and declared-return exclusions.

Poison/publication preflight precedes same-ID and cached positive/negative results. A successful selector
projection cannot resurrect a failed lexical source or turn unavailable input into fabricated recovery.
Observational lookup flags do not control correctness.

## Consequences and acceptance

The write boundary is existing reservation/admission, selector representation and transport adapters,
planner completion, the two identity shortcuts and focused proof tests. The thirty other bounded M3
rows retain their own contract. General class preparation remains owned by backlog 123.

Required proofs before acceptance:

- All eight original rows, both declaration orders, earlier-field receivers, mutable referenced children
  and exactly-once missing/private/protected diagnostics in unused consumers.
- Class-field scalar and enum copies plus exact demanded result IDs. Ordinary scalar indexing's existing
  false positive remains a classified baseline gap, not an expected new semantic rule.
- Equal/alpha-aligned selector operands with both operations and comparison directions; operation-preserving
  substitution/rebuilds/frozen-fork interning and ordinary indexed-type controls.
- Finite selected chains, self/mutual cycles, budgets and retry rollback; source poison before identity and
  both cache outcomes; selected-leaf demand with observational lookup enabled and disabled.
- A non-heritage SCC with an unrelated back-reference succeeds without selecting unrelated fields.
- Preserve the reviewed thirty-row M3 packet, direct-enum assertion boundaries and class validation gates.

## Alternatives

Plain deferred indexing loses the const-owned result operation. Globally regularizing indexed types
changes ordinary indexing. A private finisher selection stage introduces same-SCC scheduling and
another initializer-finalization owner. Mutable draft reads violate immutable publication. The qualified
selector instead carries the missing operation through the existing symbolic publication/query path.
