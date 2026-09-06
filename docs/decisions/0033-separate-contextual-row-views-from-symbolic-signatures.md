---
id: 0033
title: Separate contextual row views from symbolic signatures
status: accepted
date: 2026-09-06
---

# 0033 — Separate contextual row views from symbolic signatures

## Context

Enabling contextual arrow parameters in the as-const sprint exposed missing correlation between
parameters drawn from union-rest tuples. Generic rest signatures also require a generic source
signature with fixed positional `T[i]` types. Substituting broad constraint columns into that source
would accept invalid explicit specializations. Existing CFG assignment nodes cannot establish that
a parameter has no writes: unreachable, captured, and structurally skipped writes count too.

Independent design review confirmed the existing binder, query coordinator, immutable signature
representation, and CFG as the owners, but rejected bound normalization and CFG-only write census.

## Decision

Admit required, equal-width, mutable finite tuple rows from one rest operand or a generic rest
binder with that closed bound. Retain row identities, aggregate-rest origin, width, and complete
element IDs. Unsupported rows are not dropped to obtain an admissible subset.

The common arrow producer receives the contextual signature, not just its parameter list. It
retains its immutable generic descriptors as bound descriptors in the source function and publishes
symbolic indexed positions with the arrow's actual fixed parameter shape and inferred return.
It does not invent source binder symbols or fill the arrow's empty syntactic generic reservation.
Use existing whole-vector substitution if contextual instantiation requires remapped descriptors.

Keep three operations distinct:

- symbolic `T[i]` publication and alpha-aware relation;
- exact indexed evaluation after substitution of actual type arguments;
- coordinator-owned closed-bound projection for body reads and explicitly admitted domain proofs.

A bound column must never become an exact `T[i]` evaluation overlay or durable identity. Extend
only concrete finite tuple-row union projection at valid numeric positions. Unsupported shape and
query exhaustion remain distinct from ready projection. All existing poison, retry, cache, and
reason ownership rules remain binding.

Add immutable source-write facts through the existing source declaration traversal and finalized
binder scopes. Resolve pending target occurrences after declarations are bound, to canonical value
symbols. Include all assignment/update/destructuring/loop-head targets, dead regions, and captured
writes; member mutation alone is not a write to its base symbol. Traverse child expressions and
nested scopes structurally, without another semantic checker walk. Incomplete scope coverage is
not proof of absence of writes. Facts are source-owned, contain no RHS types or effects, and do not
rescan immutable library prefixes for each callback.

Enable a dependent parameter group only with complete write census and no participant writes;
defaults disable the initial admitted group. Use whole-row filtering and joins on the existing CFG
for literal discriminant guards and switches. Preserve ordinary direct narrowing, function-boundary
resets, and loop provisional discipline. Each contextual invocation owns its group and flow memo.
Argument-walk memo admission requires complete dependency identity or the existing non-memoizable
path. Rejected candidates restore the whole contextual invocation state.

Callable relation proves that every target argument row is accepted by a complete source
alternative. Preserve aggregate arity, readonly rejection, original receivers/returns, generic
binder alignment, and assumption/reason ownership. A concrete source may cover a generic rest
target only through the closed-domain proof when that binder has no other dependent use.
Do not turn a correlated row-rest source into independent Cartesian columns.

An inferred generic fixed-positional source is a different contract from its contextual row-rest
target. TypeScript permits its Cartesian calls without explicit type arguments; do not add a joint
inference restriction. Wrong explicit specializations and Cartesian assignments of a row-rest
source must still fail.

## Consequences

The sprint must prove source write coverage, symbolic identity before/after broad demand, explicit
specialization, row arity, generic relation direction, switch flow, and candidate rollback. Source
markers and direct identity/protocol tests are both required. This does not implement general
dependent destructuring, arbitrary indexed-access projection, or broader loop/exception CFG support.
Execution and completion evidence remain in the [sprint](../sprints/sprint-2026-08-17-as-const.md).

## Alternatives considered

- **Use each upper-bound column as the source type:** loses generic binders and exact `T[i]` meaning.
- **Use CFG writes or invalidate only after a write:** incorrectly enables correlation at earlier
  reads and misses captured/dead writes.
- **Return to untyped parameters or waive the new diagnostics:** hides the missing model rather
  than repairing the contextual route.
