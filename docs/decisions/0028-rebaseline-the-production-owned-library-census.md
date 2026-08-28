---
id: 0028
title: Rebaseline the production-owned library census
status: accepted
date: 2026-08-28
---

# 0028 — Rebaseline the production-owned library census

## Context

[ADR-0027](0027-produce-library-census-from-the-project-publisher.md) replaced the rejected
test-only recovery evaluator with one production-owned census. Its named-diff gate retained the
558-row [ADR-0018](0018-pin-library-owned-records-as-a-named-census.md) authority as a starting
baseline until the production publisher could explain every change.

The reviewed production receipt contains 709 records: 322 diagnostics and 387 incompletes. The
named multiset is `558 - 8 + 159`. The eight removals have direct production evidence:

- `certified-intrinsic-marker` accounts for the six former `intrinsic` keyword incompletes. The
  production publisher recognizes those certified marker declarations instead of reporting an
  unsupported keyword.
- `published-type-parameter-default` accounts for the `SafeArray` type-parameter default row. The
  default is now represented and published.
- `global-augmentation-region` accounts for the `declare global` row. The production binder owns
  the region and publishes it through the project-local declaration universe.

Of the 159 additions, 104 are exact unsupported sites exposed by the production lowerer's complete
record walk. They are previously hidden model gaps, not new recovery behavior. The other 55 are
validation diagnostics over supported published surfaces: 21 `TK2344` constraint results and 34
`TK2430` heritage results. TypeScript 6.0.3 accepts the pinned library profile, so these diagnostics
are relation-model debt. They are not evidence of a validation scheduler or product-lowering bug.

The same receipt contains 3,406 terminal rows: 3,377 Ready and 29 Unavailable. Its
RSS-independent digest is `9a7360705ad8d51c47a3ab7475e2d95059ddd8033dc87ea410fde1f1c0668981`.
The exact test ran in 1.81 seconds with 84,188 KiB peak RSS under a 1 GiB runtime limit.

## Decision

The production-owned 709-row multiset is the sole library-census pin after the reviewed cutover.
The suite will not retain the 558-row legacy oracle as a second authority.

The eight removed rows are accepted only with the three evidence tags above. The 104 added
incompletes remain named, typed model-gap evidence. The 21 `TK2344` and 34 `TK2430` additions remain
named relation-model divergences with backlog owners; they are not repaired by changing scheduling,
omitting supported surfaces, or adding recovery types.

ADR-0027 is superseded by this decision. Its one-producer architecture remains binding: test-only
`force_all` uses the ordinary project-local session, lowerer, coordinator, validation path,
interner, store, and published terminals. It never creates a second evaluator, resolver, semantic
universe, or recovery model. Unsupported production sites still stage typed incomplete records,
whole-member and atomic-group failure rules still apply, and no library record enters user output.

Future changes to the 709-row authority require another named multiset review. A count-only change
is never sufficient. Removed rows require production evidence; added diagnostics require a named
cause and a divergence owner.

## Consequences

- The census measures the system that ships instead of preserving outcomes from a deleted
  evaluator.
- Complete walking makes 104 previously hidden unsupported sites explicit and prevents silent
  model gaps.
- The 55 added diagnostics stay visible as relation-model debt rather than being hidden by
  publisher or scheduler exceptions.
- Terminal counts and the receipt digest provide independent lifecycle and determinism checks, but
  the named record multiset remains the semantic authority.
- Historical comparisons can reconstruct the 558-row starting point from git and ADR-0018; active
  tests carry only the 709-row production authority.

## Alternatives considered

**Keep both 558 and 709 as active pins.** Rejected because two authorities invite a compatibility
shim or a second evaluator and leave future changes ambiguous.

**Drop the 104 newly exposed incompletes.** Rejected because complete production walking is the
mechanism that proves unsupported children are not silently skipped.

**Suppress the 55 validation diagnostics in the scheduler.** Rejected because the scheduler is
correctly evaluating supported declarations through the ordinary relation path. The disagreement
belongs to the relation model.

**Restore recovery values to reproduce 558.** Rejected because `any`, `error`, `unknown`, or
`never` recovery can make an unsupported surface appear Ready and disconnect the census from
production semantics.
