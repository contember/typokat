---
id: 0004
title: Preserve prelude value types through the canonical checker pipeline
status: accepted
date: 2026-07-11
---

# 0004 — Preserve prelude value types through the canonical checker pipeline

## Context

The M28 source-backed prelude is checked before the user module, but it currently
fills and carries only type declarations. Its pass has an empty `DeclTypes` table,
does not check prelude value declarations, and discards that table before building the
user pass. A prelude `declare const console` would therefore bind a visible value name
whose lookup falls back to the error type; member access and calls on that receiver
silently suppress the very diagnostics the declaration should enable.

Backlog `38` needs a small value-side ambient surface (`console` and numeric `Math`)
for useful early real-world signal. A second loader, a special global-name lookup, or
a project-specific shim would fork the ambient path that backlog `14` must eventually
replace. Primitive wrapper and array instance members are separate model work and
remain excluded.

## Decision

Extend the existing source-backed prelude/checker pipeline to lower prelude value
declarations and preserve their `DeclId → TypeId` entries into the user pass. The
single-file and serial-project entry points must use the same handoff. The user module
continues to resolve the prelude through its parent scope, so ordinary user value/type
shadowing remains unchanged.

The handoff is limited to declaration typing already modeled by the checker. It does
not introduce another loader, a special global lookup, a new type universe, primitive
boxing, or array instance-member semantics.

## Consequences

- `src/prelude.ts` can safely declare non-generic ambient values whose types are
  already representable; their calls and member access are checked rather than silently
  error-typed.
- The checker must preserve declaration-index alignment across prelude and user passes
  in both single-file and project modes, and tests must prove bad arguments/results do
  not disappear through an error receiver.
- Backlog `14` retains one replacement path: it expands the same prelude mechanism
  rather than coexisting with a second ambient loader.
- Primitive members and `Array<T>` methods remain explicitly out of scope until their
  own representations exist.

## Alternatives considered

- **Add values to `src/prelude.ts` without a handoff.** Rejected: inherited lookup
  resolves to the error type and creates silent false negatives.
- **Add a global-name special case or separate ambient loader.** Rejected: it forks
  the architecture and would make backlog `14` maintain two paths.
- **Defer all value-side ambient work to full `lib.d.ts`.** Rejected: the approved
  bounded early-signal slice has a long enough lead time to repay this localized,
  canonical extension.
