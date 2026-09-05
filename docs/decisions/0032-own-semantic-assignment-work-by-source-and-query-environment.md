---
id: 0032
title: Own semantic assignment work by source and query environment
status: accepted
date: 2026-09-06
---

# 0032 — Own semantic assignment work by source and query environment

## Context

Syntax-derived assignment types lose errors from downstream inferred copies. Copying the complete
RHS type instead leaks readonly metadata into mutable targets. A declaration reset is not always
safe over-reporting: resetting a loop result from `1 | 2` to declared `1 | 2 | 3` accepts an invalid
`copy = 3`. Assignment-expression values also retain contextual tuple/object/callback information
that differs from both a raw RHS walk and the following target-flow type.

The [sprint](../sprints/sprint-2026-08-17-as-const.md) pins these behaviors against TypeScript 6.0.3.
Its stronger loop witnesses need an initializer declared later in the same loop body. Independent
design review additionally disproved a simultaneous equation fixed point as a TypeScript-fidelity
criterion: a generic back-edge call can produce a different result in the loop's trial query and
its final lexical check.

## Decision

Keep structural CFG construction, but register semantic assignment occurrences instead of guessing
RHS types in that pre-pass. Facts belong to exact source occurrences and authenticated checking
environments, including raw/contextual policy, candidate ownership, and loop-query observations.
Separate three results:

- the assignment expression's checked value under its actual contextual policy;
- the assigned source type used for target reduction;
- the target-flow transfer, including explicit incompatible-write recovery.

Retain a non-union declared target surface. Reduce union targets to compatible **declared members**,
then require whole-source coverage; definite incompatibility recovers to the declaration. Preserve
TypeScript's semantic-boolean-union and fresh-boolean rules. Exhaustion is not incompatibility or a
successful reset. All reduction probes remain coordinator-owned; the existing lexical assignment
obligation remains the sole mismatch-reporting owner.

### Retained source products

Use one shared initializer/assignment producer for early demand and normal lexical checking.
Authenticate single-storage local initializer handles from the existing source declaration
manifest, with exact declaration identity, scope, storage, kind, and lexical tickets. Do not add a
semantic declaration walk or infer a type from an RHS syntax whitelist.

Separate source-owned effect capture from adoption. A retained product includes dependencies,
semantic results, held lexical work, and explicit pending publication actions. Readers borrow its
type; exactly one selected lexical owner adopts its completed work. Semantic trials under different
loop/candidate environments may run more than once; their records are not already committed output.
Final reporting must not adopt a trial's missing diagnostic when final operands differ.

Producer-local writes cover named declaration observations/publication actions, initializer
availability, annotation-reservation consumption, alias metadata, and expression/assignment/flow
facts. Coordinator queries retain their existing savepoints. Candidate rollback discards products,
writes, and adoption state together. Never snapshot all of `Pass`, rewind type-parameter identities,
mutate published class/type registries, or delete/deduplicate committed records.

### Availability and loop policy

Early local demand is limited to authenticated single-identifier `let`/`const` producers in the
same finalized function/module flow boundary and candidate environment. Chains resolve transitively
through normal expression semantics. Already-terminal external/class/callable surfaces are usable;
a provisional `unknown` function return is not a terminal inferred signature.

This capability does not compute forward function bodies, cross-file inferred values, accessor or
static construction, namespace publication, merged `var` owners, or new nested callable owners.
Those retain the broader backlog-76 boundary. Pending work within the admitted frontier must resolve,
not become a replacement incomplete for an acceptance diagnostic. An excluded owner, a genuine
declaration-inference cycle, or an actual resource limit has typed non-clean exhaustion and one
defined owner; ordinary loop re-entry is not a declaration-inference cycle.

Follow TypeScript's incomplete-flow discharge policy with query-local expression environments,
not a simultaneous fixed point over every expression and loop equation. Unresolved trial operands
cannot enter durable declarations, callable returns, obligations, or contextual/query caches. The
loop producer explicitly discharges a result for its observation environment; cache dependencies
retain that environment and replay transitively. Final lexical call checks use their final inputs.

Start implementation by proving one future-local and one call product's capture/adoption lifecycle.
If source-owner execution requires mutations outside the closed producer boundary, use loop-local
pending producer scheduling over the same retained products rather than a broad state snapshot or
whole-body replay. This is a scheduling choice, not a second inference pipeline.

## Consequences

The checker gains a bounded local producer-availability capability, not a general lazy declaration
engine. The old blanket claim that complex-RHS resets are sound over-reporting is superseded by this
transfer contract. The binding flow invariant must reflect this contract at production cutover.
ADR-0008's immutable class construction and single-owner lexical effects remain unchanged.

Acceptance includes contextual assignment results, mutable declared unions, invalid writes,
boolean modes, loop trial versus final-call diagnostics, candidate rollback, transitive memo
dependencies, and zero durable promotion after exhaustion. Direct lifecycle tests complement source
markers. An accepted architecture does not establish implementation completeness or close B44.

## Alternatives considered

- **Keep syntax-derived RHS facts:** misses modeled calls, members, composites, and later producers.
- **Copy RHS identity or reset on missing facts:** loses declared surface semantics or downstream
  errors. Clearing caches afterward cannot repair an already published inferred declaration.
- **General equation solver:** differs from the pinned TypeScript loop behavior even for fully
  explicit callable signatures; it is not a fidelity improvement.
- **General lazy declarations or whole-body replay:** exceeds the required owner frontier or loses
  mutation/effect ownership. Source-order pending scheduling is a valid bounded alternative only
  when it uses retained products and suspends dependent publication, not just diagnostics.
