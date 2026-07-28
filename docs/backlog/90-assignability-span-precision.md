---
id: 90
title: Assignability diagnostics report the declarator instead of the expression
blocked-by: []
---

# 90 — Assignability diagnostics report the declarator instead of the expression

**Summary.** `TK2322`/`TK2741` point at the start of the declarator rather than the offending
sub-expression, so a wide initializer sends the reader to the wrong column. Introduced silently on
2026-07-14 and unnoticed since. Effort S–M.

## Problem

A bisect (2026-07-25) comparing diagnostic output across `d5c73eb` → HEAD found the outputs identical
as multisets — same files, lines, codes, messages, same code histogram — with exactly one behavioural
delta: **assignability diagnostics lost column precision.**

```
- errors-1000.ts(105,47): TK2741 …      - (106,45): TK2322 …      - (107,60): TK2322 …
+ errors-1000.ts(105, 7): TK2741 …      + (106, 7): TK2322 …      + (107, 7): TK2322 …
```

Affected: **153 of 514** diagnostics on `errors-1000`, **1,593 of 5,374** on `errors-10000` — all of
them `TK2322`/`TK2741`. Cause: `a7923b6` ("feat: cut over immutable class semantics") re-routed
diagnostic ownership through lexical-owner tickets, which attribute a diagnostic to the owning
declarator rather than to the expression that failed.

Nobody noticed for eleven days, because the conformance corpus matches on `// error[TK…]: substring`
markers — code and message text, not column. `tsc` reports the offending expression, so this is also
a divergence that is not recorded in `docs/reference/divergences.md`.

Severity is real but bounded: the error is still on the right line with the right message, so it is a
quality regression, not a soundness one. On a wide object-literal initializer, though, the caret lands
tens of columns from the actual mismatch.

## Approach / acceptance

Carry the failing expression's span through the lexical-owner attribution instead of collapsing to the
owner's start — the ticket decides *who owns* the record, which need not decide *where it points*.
Check the equivalent paths for excess-property, argument, and return-position diagnostics: the bisect
only sampled the `errors` corpus, so the affected set may be wider than `TK2322`/`TK2741`.

Acceptance: fixtures pinning the column for an assignment whose initializer is wide (so declarator
start and expression start differ unmistakably), cross-checked against real `tsc --strict`; the
existing corpus stays green. If any case must keep the declarator span, record it in
`docs/reference/divergences.md` rather than leaving it undocumented.

Consider also extending the conformance harness to optionally assert columns — the marker format is
substring-based today, which is precisely why this hid.

## Touch points

`crates/typokat-check/src/check/checker/lexical_events.rs` (owner tickets), `crates/typokat-check/src/check/checker/assignment.rs`,
`src/diagnostics/`, `tests/cases/` fixtures, `tests/conformance.rs` if column assertions are added.

<!-- Origin: bisect of the multi-file regression, 2026-07-25 (section 4, incidental). -->
