---
id: 03
title: readonly / property not enforced through a union member access
blocked-by: []
---

# 03 — readonly / property not enforced through a union member access

**Summary.** False negative. Assigning to a member reached through a union of object types skips the
`readonly` (and union-property-existence) check.

## Problem

```ts
type A = { readonly value: number }; type B = { readonly value: number };
declare const u: A | B;
u.value = 12;             // typokat: silent   (tsc: TS2540 read-only)
```

## Approach / acceptance

When the assignment target is a member accessed on a union type, enforce `readonly` (and
property-existence) across the union members, as a non-union member access already does. Acceptance: a
fixture assigning to a `readonly` member of a union reports `TK2540`.

## Touch points

Member-assignment checking through union targets; the `readonly` enforcement path.

<!-- Origin: official-suite finding F5. -->
