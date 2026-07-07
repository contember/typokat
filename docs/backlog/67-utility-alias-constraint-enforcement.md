---
id: 67
title: utility/prelude alias type-parameter constraints unenforced (TK2344)
blocked-by: [./24-rest-elements-in-type-model.md]
---

# 67 — utility/prelude alias type-parameter constraints unenforced (TK2344)

**Summary.** Track C silent-FN (small). A prelude/utility alias whose type-parameter
**constraint** is not representable in the current model (it uses rest parameters) has
that constraint **silently dropped**, so a violating type argument evaluates instead
of erroring — a dropped tsc `TS2344`.

## Problem

Verified vs `tsc 6.0.3 --strict`:

```ts
type R = ReturnType<number>;
```

tsc reports `TS2344`: "Type 'number' does not satisfy the constraint
'(...args: any) => any'." typokat evaluates `ReturnType<number>` to `never` and is
**clean** — the constraint check never runs. Root cause: the prelude
`ReturnType<T extends (...args: any) => any>` constraint uses a **rest parameter**,
which is out of the type model (backlog `24`), so it degrades and the `TK2344`
argument check against it is skipped. Documented in
[`../reference/divergences.md`](../reference/divergences.md) (Utility types) as an
under-report.

Generalize: any alias (prelude or user) whose declared constraint is unrepresentable
or silently dropped will skip its `TK2344` check on instantiation — the audit surface
is "constraint dropped ⇒ argument accepted".

## Approach / acceptance

Depends on `24` (rest elements) making `(...args: any) => any` representable. Once the
constraint is a real type, ensure the alias type-argument `TK2344` check runs against
it (the M24 machinery already does this for representable constraints). Guard against
regressions: a **dropped/unrepresentable** constraint must not silently accept a
violating argument — either enforce a modeled subset or over-report, never pass.

Acceptance: `ReturnType<number>` → `TK2344`; `ReturnType<() => string>` clean;
`Parameters<number>` / other rest-constrained aliases likewise once `24` lands.
Cross-check `tsc 6.0.3`. Extend `m28_utility_types/` (or a focused corpus dir).

## Touch points

`src/prelude.ts` (the alias constraints), the alias-instantiation `TK2344` path
(`src/check/…` / `src/relate/…`), `m28_utility_types/` corpus,
`docs/reference/divergences.md`.

<!-- Origin: 2026-07-07 divergence-ledger audit (verified vs tsc 6.0.3). -->
