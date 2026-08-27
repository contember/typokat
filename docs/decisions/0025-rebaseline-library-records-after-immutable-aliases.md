---
id: 0025
title: Rebaseline library records after immutable recursive aliases
status: accepted
date: 2026-08-27
---

# 0025 — Rebaseline library records after immutable recursive aliases

## Context

[ADR-0018](0018-pin-library-owned-records-as-a-named-census.md) pins the default library's
checker-owned records as a named multiset. Its current 558 rows contain 267 diagnostics and 291
incomplete surfaces. The diagnostic part is model-gap debt against a TypeScript 6.0.3 profile that
real `tsc --strict --target es2025 --noEmit --noLib` checks clean.

[ADR-0024](0024-represent-recursive-aliases-as-immutable-applications.md) replaced guarded alias
re-entry through a recovery `error` type with immutable alias applications and published bodies.
That correction removes 97 `TK2430` interface-heritage diagnostics and 51 `TK2344` constraint
diagnostics. All 148 were downstream false positives caused by the old recovery frontier. The 291
incomplete surfaces and the unrelated `TK2503` and `TK2304` diagnostics do not move.

The first implementation also produced one new `TK2456` for the legal recursive conditional alias
`Awaited`. That row is a regression, not a new baseline: the conditional check surface must still
detect direct cycles, while the true and false branches are productive recursive boundaries.
Consequently the transient 411-row result is not eligible for pinning. After that regression is
fixed, the named census is 410 rows: 119 diagnostics and 291 incompletes.

WU4 also needs a test-only project-local `ForceAllRecordAudit`. Preserving the old 558 rows there
would require deliberately recreating the obsolete recovery `error` frontier after the semantic
checker stopped using it. Such an audit would measure historical behavior instead of the checker it
is meant to inspect.

## Decision

We will rebaseline ADR-0018's named census from 558 to 410 only after the `Awaited` conditional
recursion regression is fixed.

The accepted named diff is exactly:

- remove 97 `TK2430` rows;
- remove 51 `TK2344` rows;
- add no diagnostic row;
- change no incomplete row.

The resulting diagnostic multiset contains 104 `TK2430`, 13 `TK2344`, one `TK2503`, and one
`TK2304`. The incomplete multiset remains 291 rows. The fixture update must expose every removed
path, site, code, and message in review; count-only blessing is forbidden.

The project-local `ForceAllRecordAudit` will target this current 410-row named authority. It may use
audit-only recovery for still-existing checker gaps, but it must not emulate the removed recursive
alias recovery frontier. Its scratch semantic state remains isolated from publication as specified
by the active project-local library-universe sprint.

## Consequences

- Immutable recursive aliases improve the general checker and the project-local candidate through
  one semantic model.
- The named census continues to measure current model-gap debt rather than preserving resolved
  debt for numerical stability.
- The `Awaited` fix is a hard prerequisite. A 411-row result fails the census.
- Future semantic improvements may move the pin only through another explicit named diff under
  ADR-0018's process.
- WU4 force-all parity targets 410 rows and still requires exact path, owner, span, code or surface
  id, and content equality.

## Alternatives considered

- **Emulate the old recovery frontier inside `ForceAllRecordAudit`.** Rejected because it would
  intentionally manufacture 148 false positives that the semantic checker no longer produces.
- **Keep immutable aliases only in the project-local lowerer.** Rejected because it would create two
  alias models and violate ADR-0024's shared guarded-recursion plan.
- **Leave the pin at the transient 411 rows.** Rejected because the added `Awaited` `TK2456` is a
  confirmed regression.
