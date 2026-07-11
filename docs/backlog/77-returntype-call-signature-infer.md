---
id: 77
title: ReturnType inference from object call signatures
---

# 77 — `ReturnType` inference from object call signatures

**Summary.** Track C silent-FN. Conditional `infer R` extracts returns from direct
function types, but callable object types — including overload sets — degrade to the
error type, so wrong assignments to `ReturnType<CallableObject>` pass silently.

## Problem

Verified against `tsc 6.0.3 --strict` during the backlog `67` adversarial review:

```ts
type Over = {
  (x: string): number;
  (x: number): string;
};
type R = ReturnType<Over>;
const bad: R = 1;
```

tsc infers the last overload return (`string`) and reports `TS2322`; typokat is clean.
A single call-signature object with an ordinary property has the same failure, so the
family is broader than overload selection. The new `ReturnType` constraint correctly
accepts both callables; the pre-existing failure is in conditional infer extraction.

## Approach / acceptance

**Acceptance spec (ready):** the disabled
[`tests/cases/sr_deferred_ledger/b77_returntype_call_signatures.ts`](../../tests/cases/sr_deferred_ledger/b77_returntype_call_signatures.ts)
fixture pins single-signature callable-object return extraction and tsc's last-overload
return rule.

Teach conditional inference to consume represented `ObjectType.call_signatures` without
weakening direct-function inference. Preserve signature order for overload return
selection, keep non-callable constraint rejection owned by shipped backlog `67`, and
cross-check mixed properties, optional/rest signatures, and nested conditional use.

## Touch points

Conditional infer candidate collection/evaluation under `src/check/`, object call-signature
representation in `src/types/`, focused M25/M28 fixtures, and backlog `68` only if code
tracing proves the same variance-aware candidate reducer is shared.

