---
id: 18
title: Duplicate identifier detection for object and interface members
blocked-by: []
---

# 18 — Duplicate identifier detection for object and interface members

**Summary.** Missing `TK2300` duplicate-member detection lets conflicting declarations in one object
or interface type pass silently.

## Problem

typokat does not currently detect duplicate member names in object type literals or interfaces. A
case surfaced during the WU1 object/interface method-signature review is a property and a method
sharing the same name:

```ts
interface T {
  m: number;
  m(x: string): string;
}
```

`tsc --strict` reports `TS2300: Duplicate identifier 'm'`, while typokat reports nothing and later
member reads resolve according to existing member-lookup behavior. Method signatures did not create
the underlying gap; they only made it easier to observe.

## Approach / acceptance

Add duplicate-member detection for object type literals and interfaces, reporting `TK2300` for names
that occur more than once in the same member list when TypeScript would reject them. Acceptance:
fixture coverage for property/property and property/method duplicates in both object type literals
and interfaces, with behavior checked against `tsc --strict`.

## Touch points

Binder and/or annotation lowering paths that collect object and interface members; diagnostic
definition/rendering for `TK2300`; conformance fixtures documenting the duplicate-member cases.

<!-- Origin: WU1 adversarial review, recorded in ../sprints/sprint-2026-06-28-object-interface-signatures.md. -->
