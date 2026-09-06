---
id: 0037
title: Retain construction-owned sources for class initializer validation
status: accepted
date: 2026-09-06
---

# 0037 — Retain construction-owned sources for class initializer validation

## Context

Pass context preparation can infer `{ key: "alpha" }` while query-free class construction has
published `{ key: string }` for the same field. Validating the fresh lexical source removes the old
TK1360 without correcting the field used by later reads: an invalid downstream write becomes clean.
[ADR-0036](0036-bound-contextual-class-publication-migration.md) permits deferring general contextual
class inference, not introducing this new false clean.

The field's final type cannot recover every validation source. Array collection loses individual
contributions; an inferred arrow body crosses a separate return-publication boundary. Existing
`SatisfiesCheckNode` groups contain only the later lexical walk's sources. This establishes the
independently consumed source/publication distinction anticipated by
[ADR-0035](0035-finalize-literal-types-at-publication-owners.md).

Independent design and step review support a flat retained input, not another expression graph.
A leader oracle also rules out substituting constructor body types into declared-return checks:
doing so would reintroduce a false positive already fixed by Pass context preparation.

## Decision

During existing successful unannotated class initializer construction, retain the source identities
needed by existing inferred-publication validation boundaries. The ordinary lexical walk consumes
these immutable inputs when it creates its existing checks; no second source walk or check tree.

| Eligible consumer | Construction-owned input |
|---|---|
| Whole `satisfies` obligation and successful operator result | Exact completed operand before enclosing field publication |
| Inferred object member or array/tuple element child check | Finalized individual contribution before collection |
| Nested inferred container residual | Completed container; reuse a matching contribution when possible |
| Unannotated inferred-`satisfies` expression-arrow return child | Actual body source before inferred-return finalization |
| Enclosing function/member/operator residual | Completed function source, with its declared return when present |
| Explicit return annotation checks and their recursive body walk | **No retained substitution:** use fresh lexical actual sources |
| Annotated fields or failed/unsupported initializer construction | **No fabricated receipt:** preserve existing lexical checking |

### Ownership and eligibility

- Rows contain one source `TypeId` and an exact original occurrence/boundary identity. Scope by
  source and existing class/member identity. Full spans and boundary kind distinguish chained
  operators sharing a start offset; select before diagnostic reanchoring.
- Capture only the existing admitted initializer recursion and only required boundaries. Do not
  admit conditional/logical expressions or rest/default/generic/block arrow forms through capture.
- Complete rows only for a successful initializer; discard its provisional rows on failure without
  losing successful siblings. Keep them checker-local for the compilation/body lifetime, with valid
  store/template IDs. Storage and capture work are bounded by visited validation boundaries.
- Install immutable, non-destructively reusable inputs around the existing field Body execution.
  Restore the prior scope on every exit. Nested scopes and repeated/candidate attempts cannot steal
  or consume another field's inputs. Annotation types are never initializer-source receipts.
- Eligibility follows the checking owner, not syntax alone. Suspend retained substitution throughout
  an explicitly declared return body's recursive lexical walk, including nested `satisfies` results.
  Its enclosing function residual may still use the completed retained function source.
- `KeptRaw` alone is not proof of successful lexical preparation: it also represents failed
  preparation/rewalking. Preserve the private success/failure distinction before source selection.
  A receipt cannot resurrect failed lexical work, produce a successful partial result, or suppress
  a child diagnostic. Real `unknown` remains distinct from unavailable/error recovery.

### Unchanged semantic boundaries

Select eligible sources before creating `AssignObligation`/`SatisfiesCheckNode` values. Keep original
targets, excess checks, child-before-residual grouping, reanchoring, EventStore ownership, and raw/
contextual effect selection. Do not append a second broad-source check or edit finished effects.

Class construction remains query-free and publication complete, immutable and atomic. Receipt-only
roots must not be added to `ClassDraft::identity_roots`: that traversal schedules dependency SCCs,
not merely storage auditing. This decision does not authorize dependency graph or class order changes.
Use existing class body-view/retaining-walk memo exclusion where sufficient; prove it. Only a
demonstrated gap permits narrow active-scope exclusion or complete identity in the existing key.

## Acceptance and alternatives

Preserve exactly one legacy TK1360 at `(4,14)` in `publication_class_lexical_source.ts`, with zero
incompletes. The correct downstream TS2322 remains asserted separately under backlog 123. The
`publication_class_validation_*` fixtures pin nested contributions, distinct/chained operators,
inferred versus declared arrows, annotated fields, template IDs and unsupported siblings in both
orders. Their legacy TK1360 expectations are explicit compatibility constraints, not tsc parity.

Direct tests must pin source IDs, published field IDs, original lookup versus reporting spans,
colliding offsets across classes/files, template binder identity, failure/discard/scope restoration,
exact EventKeys/effect counts, memo exclusion and unchanged construction graph/SCC scheduling.
Preserve repaired scalar/enum roles, broad class positives and all prior contextual tests. Re-review
the complete cumulative unit and rerun required differential/ordinary gates before acceptance.

Whole-field/root-only retention loses child identity or reporting ownership. A fresh lexical source
cannot stand in for a different published source. Re-running class inference duplicates semantic
work. A second retained check tree duplicates existing reporting ownership. General class-context
preparation remains backlog 123; this decision retains sources, not a new preparation capability.
