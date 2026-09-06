---
id: 0038
title: Reject inconsistent contextual class publication conservatively
status: accepted
date: 2026-09-07
---

# 0038 — Reject inconsistent contextual class publication conservatively

## Context

General contextual class preparation is deferred under
[ADR-0036](0036-bound-contextual-class-publication-migration.md). The construction-owned validation
inputs in [ADR-0037](0037-retain-class-initializer-validation-sources.md) expose a consequence:
if an immutable field contains `{ key: string }` but its target later resolves to
`{ key: "alpha" | "beta" }`, validation of that published source must fail.

Independent review and leader oracles establish newly rejected **valid** programs in both domains:
an exported class using an external interface's `keyof`, and a class-dependent conditional context.
The self-dependent error remains when the class is unused or its copied property receives only the
original literal. This is not merely moving tsc's downstream diagnostic to an earlier location.

Keeping both the broad immutable publication and old cleanliness would retain the source mismatch
that caused a newly false-clean external-keyof program. Preparing the correct field instead requires
the class semantic work explicitly deferred to backlog 123. The leader chooses this bounded
compatibility reduction under the user's authorization to decide and finish the sprint autonomously.

## Decision

Narrowly supersede ADR-0036's old-cleanliness requirement and ADR-0037's restored-legacy-only
acceptance assumption **for contextual class preparation gaps**. Both ADRs otherwise remain binding.

- Validate eligible inferred class initializer sources actually used by immutable publication.
  If those sources fail the effective contextual target, emit the existing relation diagnostic at
  its existing owner, even when the older checker and tsc accept the initializer.
- This covers external and class-dependent contexts, including exported classes and the supported
  Bundler route. Eligibility is semantic, not a list of self references, exports, syntax spellings,
  source offsets or downstream uses. Do not add blanket unsupported admission or deliberate errors.
- Accept only the conservative discrepancy caused by unavailable effective context during class
  construction. Ready-source producer mistakes, source-scope leaks, lost diagnostics, duplicate
  checks, failure resurrection and wrong explicit-annotation authority remain implementation bugs.
- Preserve broad/ready contexts, literal-owned const/reference boundaries, ordinary readonly modes,
  and ADR-0037's declared-return body exclusion. Construction stays query-free; no partial surface,
  post-publication repair, query callback or second evaluator is authorized.

## Acceptance and consequences

`publication_class_conservative_external.ts` and `publication_class_conservative_self.ts` retain
one TK1360 each at `(3,13)` and `(3,14)`, with zero incompletes; strict tsc accepts both whole files.
Keep the broad self-dependent and exported controls clean, and verify the equivalent supported
three-file project with colliding local offsets: only the narrow producer reports, with no notices.
Unused and same-value-write self variants have the same conservative diagnostic.

The unchanged backlog-123 fixtures still require the correct final TS2322 and no initializer error.
This decision does **not** complete that release-blocking item or claim improved tsc parity. Record
the new false positives in the divergence ledger and identify them separately in replay comparisons;
do not use a broad allowlist or rebaseline unrelated fixture/scoreboard expectations.

The independent ready-rest arrow finding is excluded from this amendment. A source parameter typed
`number[]` instead of the contextual positional `number` is a producer defect, not unavailable
class context. It must be repaired and the complete cumulative implementation reviewed again.

## Alternatives

Preparing class-dependent contexts is the eventual correctness fix, but expands the construction
work deliberately excluded here. Validating a different lexical source preserves the demonstrated
publication mismatch. Skipping only self-dependent/exported targets or inspecting downstream uses
would be asymmetric suppression. Conservative validation is less compatible but keeps one source
of published meaning until general contextual class preparation is implemented.
