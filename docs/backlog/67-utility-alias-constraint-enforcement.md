---
id: 67
title: utility/prelude alias type-parameter constraints unenforced (TK2344)
---

# 67 — utility/prelude alias type-parameter constraints unenforced (TK2344)

**Summary.** Track C silent-FN (small). Prelude utility aliases still omit their
lib-style type-parameter constraints, so a violating type argument evaluates instead of
erroring — a dropped tsc `TS2344`.

## Problem

Verified vs `tsc 6.0.3 --strict`:

```ts
type R = ReturnType<number>;
```

tsc reports `TS2344`: "Type 'number' does not satisfy the constraint
'(...args: any) => any'." typokat evaluates `ReturnType<number>` to `never` and is
**clean** — the constraint check never runs. M32 made the rest-parameter shape
representable, but the embedded prelude still uses unconstrained aliases so it does
not introduce permissive `any[]` into the checker. Documented in
[`../reference/divergences.md`](../reference/divergences.md) (Utility types) as an
under-report.

Generalize: any prelude or user alias whose declared constraint is omitted or silently
dropped will skip its `TK2344` check on instantiation — the audit surface is
"constraint dropped ⇒ argument accepted".

## Approach / acceptance

**Acceptance spec (ready):** the disabled
[`tests/cases/sr_deferred_ledger/b67_utility_constraint.ts`](../../tests/cases/sr_deferred_ledger/b67_utility_constraint.ts)
fixture pins `ReturnType<number>` → `TK2344` plus the `ReturnType<() => string>` clean control.

Ensure the alias type-argument `TK2344` check runs against utility constraints without
weakening the checker with a permissive `any[]` shortcut (the M24 machinery already
does this for representable constraints). Guard against
regressions: a **dropped/unrepresentable** constraint must not silently accept a
violating argument — either enforce a modeled subset or over-report, never pass.

Acceptance: `ReturnType<number>` → `TK2344`; `ReturnType<() => string>` clean;
`Parameters<number>` / other constrained utility aliases likewise once they are added.
Cross-check `tsc 6.0.3`. Extend `m28_utility_types/` (or a focused corpus dir).

## Touch points

`src/prelude.ts` (the alias constraints), the alias-instantiation `TK2344` path
(`src/check/…` / `src/relate/…`), `m28_utility_types/` corpus,
`docs/reference/divergences.md`.

<!-- Origin: 2026-07-07 divergence-ledger audit (verified vs tsc 6.0.3). -->
