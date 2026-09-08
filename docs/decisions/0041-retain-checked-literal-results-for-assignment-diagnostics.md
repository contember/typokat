---
id: 0041
title: Retain checked literal results for assignment diagnostics
status: accepted
date: 2026-09-08
---

# 0041 — Retain checked literal results for assignment diagnostics

## Context

M2 declared-target reduction correctly exposes narrower assignment targets, but also exposes old
excess/missing-property duplicate reports. Selecting all recursive excess diagnostics from the outer
relation's first failure drops independent nested and sibling errors. Literal-site selection needs
each site's checked source, not only the aggregate assignment type.

Raw and contextual array inference merge element types into a union. Swapping valid-extra and
wrong-extra elements preserves that aggregate TypeId but changes the correct diagnostic sites.
The existing excess traversal cannot reconstruct that information without another semantic walk.
The user approved retaining these already-checked identities in the existing inference result,
including raw/contextual memo and failure handling.

## Decision

### One checked result, produced by the existing walk

The existing expression match produces a checker-local `CheckedExpr`: root TypeId, legacy span and
an optional shared immutable literal product. `infer_expr` remains a tuple-projection adapter over
that same match. Do not duplicate dispatch, re-infer a child or add a global collection sink.

Supported object, array, parenthesized, conditional and logical value nodes retain their actual
occurrence edges. Property ordinals distinguish duplicate keys; array positions retain skipped slots.
Each child records its checked result before parent publication and its actual contribution after
publication/narrowing. Preserve module/scope, full original spans and the prepared context's original,
publication, shaping and contextual-this fields. Opaque expression forms remain opaque where the
existing excess traversal is opaque.

Build products linearly during the existing walk. Move children once and share immutable descendants
on memo hits. Identity is the source occurrence and its edge path, not the aggregate TypeId. No parent
pointers, global registry, provenance graph or syntax-to-union-arm matching are required.

`InitializerValue` retains the checked result and enforces matching root ID/span. Preserve wrappers
through both canonical inference and the initializer's early parenthesis path. Raw/contextual
object/array/tuple producers record child identities before normalization or unioning loses position.
Preserve existing publication roles and Existing/Satisfies policy.

### Raw/contextual selection, memo and retention

Argument and contextual memo values retain the checked product and existing contextual status.
Clone shared products rather than rebuilding children. Keep full environment keys, dependency checks,
refusals, budgets, argument-region lifetimes and effect frames. There is no optional collection policy.

`KeptRaw` preserves raw descendants but also any existing native literal-shaping update to the root.
`Rewalked` uses its contextual product. `Failed` and `RewalkedFailed` preserve existing recovery/effect
behavior but cannot authorize a ready site group from a partial product. Never attach raw children to
a contextual root or combine different environments. Opaque lifted tuple inputs cannot request a
group requiring unavailable literal sites.

Checked call/new argument vectors use the existing compacted successful-argument indexing. Retain
original AST argument indices separately for existing raw-record rematerialization. Native inference
receives the same projected IDs/spans. Payload collection does not introduce a new raw rewalk.

While a product is live, its roots include every checked ID, contribution and all four prepared-context
IDs recursively. If retained across semantic-root collection, enumerate them there. Deferred diagnostic
groups retain only prepared obligations and failure evidence, not the checked tree. Their census
includes every site's original source/target and additional target IDs in unavailable evidence.

### Prepare literal-site obligations without rechecking source

Extend the existing target/excess preparation traversal, zipped with checked occurrence edges.
Preparation borrows the product and returns `PreparedLiteralAssignment { root, unavailable_sites }`.
Retain independently prepared siblings when a later source/target is unavailable; rollback failed
preparation query writes. Missing sources have typed site evidence, never fabricated obligations.

| Edge | Validation endpoint |
|---|---|
| Root/container residual | Actual constructed root, including finalized contributions, versus original assignment target |
| Property/element | Child's checked expression ID versus its actual original child target |
| Parenthesis/conditional arm | Actual retained child product and original target |
| Eligible `??`/`||` left contribution | Retained non-nullish/truthy contribution, not the whole operand type |
| `&&` left | No independent contextual child obligation; root relation checks its false contribution |

Excluded value arms create no assignment-value obligations or unavailable taint. Their independently
owned lexical effects remain. Distinguish an excluded contribution from a failed required child.
Keep normalized shaping targets only for child discovery and excess candidates; reporting retains
original obligation endpoints. Include opaque scalar/call children when they have real child targets.

Undecomposable targets retain the pre-selector policy of existing excess candidates plus an ordinary
assignment obligation, not the rejected whole-No selector. Preserve existing union/intersection
eligibility and merged-key behavior; do not invent general union-arm diagnostic selection.

### Decide the group before publishing records

Schedule a dedicated literal-assignment group at the existing deferred assignment checkpoint.
At draining, the pure attempt evaluates children in source order and each site's original relation
once when endpoints exist, including the parent relation after invalid children. Use the existing
coordinator; no nested effectful retry or new relation engine.

The attempt returns `SemanticQueryAttempt<LiteralGroupOutcome>`, with these terminal alternatives:

- `Decided(Valid | Invalid(diagnostics))`;
- `Unavailable { diagnostics, sites }`;
- `Exhausted { diagnostics, failures, unavailable_sites }`.

Each query failure retains its actual obligation and exhaustion. Preparation failures retain their
actual spans, available targets and typed evidence. A bare outer Exhausted would lose sibling
diagnostics and the failure site. Continue independent siblings; failure dominates invalidity,
which dominates validity. Never make an unavailable parent committable because a sibling is invalid.

With definite outcomes, invalid children retain all their selected diagnostics and keep the parent
invalid. Suppress only that parent's residual/excess report. If children are valid, local excess wins
over local missing-only/Yes; select the first extra key at that literal. Other residual failures use
the existing assignment renderer. With no excess, retain the ordinary residual outcome. Separate
constraint/header/class obligations are not part of this suppression hierarchy.

Examples: wrong `left.value` and extra `right.extra` report both errors in either property order.
Two excess sibling literals report two errors. A wrong existing value suppresses only its own
literal's extra key. A missing nested property suppresses an unrelated outer extra. Array order swaps
site diagnostics without changing the aggregate array TypeId.

### Failure ownership and query transactions

Each attempt has one receiving query savepoint. Commit only definite Valid/Invalid results. Native
requests discard attempt decisions and rollback before materialization, then retry pure decisions
over retained obligations. Terminal unavailable/exhausted results rollback all query writes but
retain independent diagnostic values for reporting. No source or RHS effect is replayed.

Append terminal selected diagnostics once to their existing ticket, then report owned failures at
their actual sites. Missing-source evidence must identify the existing non-clean owner; it cannot
silently become an empty unavailable success. Already-owned class/alias poison remains suppressed.

For the new group, evaluation/alias budget and evaluation-cycle failures emit the existing TK2589
diagnostic at the failing site's span through the user-reporting ticket. Do not copy the legacy
ordinary-assignment reporter's silent evaluation-budget/cycle arms. Invalid nodes use the existing
invalid-evaluation incomplete; class projection failure retains its existing diagnostic/incomplete
policy. Preparation demand failures retain their demand owner. Do not duplicate an already-owned
failure or alter unrelated legacy obligation policies as part of this unit.

## Implementation and proof

The fence is checker-local: expression/result transport, contextual/raw argument memo adapters,
assignment preparation, deferred group/effect dispatch, and the retained-root enumeration in
`checker/decls/mod.rs`, plus focused tests. Inference engine, Store, query implementation, flow/boolean/
compound semantics, class publication and ADR-0040 source construction remain unchanged.

Retain the nested fixture's 20 markers, flat 14, operation eight and original M2 repairs. Add logical
contribution controls, raw/contextual hit and changed-environment tests, root-shaping compatibility,
compacted argument alignment, retained-root checks and sharing/growth assertions. Preserve both
array-root ID equalities and assert position-wise child IDs, contributions and original spans.

Force late preparation and late parent-query failure after an independently invalid sibling. Its
diagnostic must survive while all group query writes rollback and failure reporting names the correct
site. Test native retry separately from terminal failure. Preserve selected/rejected/raw effects,
native scaling bounds, publication/owner parity and the differential/repro/official gates.

## Consequences

This retains semantic data already computed at the owner that can still distinguish literal sites.
It adds local result memory and a deferred decision group. Shared descendants prevent repeated
subtree copying. It does not change the root type model or add a second initializer pipeline.

Reject any candidate whose memo hit mixes environments or failed products, whose failure drops an
independent sibling diagnostic, or whose parent becomes valid despite a child failure. Do not repair
such cases with a fallback rewalk, global lookup or first-reason filtering.
