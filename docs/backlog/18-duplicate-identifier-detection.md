---
id: 18
title: Duplicate identifier detection for object and interface members
blocked-by: []
---

# 18 — Duplicate identifier detection for object and interface members

**Summary.** Missing duplicate-declaration diagnostics let conflicting members,
block-scoped bindings, and function implementations pass silently; duplicate
implementations can also hide independent call diagnostics.

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

The same smell exists one level up, at scope level: `let x = 1; let x = 2;` passes silently
(probe 2026-07-07; tsc: `TS2451 Cannot redeclare block-scoped variable`, on both declarations).
Scoped here as the sibling check — same "duplicate declaration goes silent" family, binder-side.

The backlog-74 WU3 review also proved that two function implementations plus a
same-name `var` can select the last implementation as the visible callable. Besides
the missing `TS2300`/`TS2393`, this can drop an independent `TS2345`; the disabled
`sr_deferred_ledger/b18_duplicate_function_implementations.ts` fixture pins that
soundness consequence.

## Approach / acceptance

Add duplicate-member detection for object type literals and interfaces, reporting
`TK2300` for names that occur more than once in the same member list when TypeScript
would reject them; add `TK2451` for `let`/`const` redeclaration and the corresponding
function-implementation/`var` conflict diagnostics (`TK2300`/`TK2393`/`TK2403`).
Preserve legal declaration merging. Acceptance includes property/property and
property/method duplicates, block-scoped redeclarations, duplicate function
implementations, and proof that rejected duplicates cannot suppress independent call
diagnostics, all checked against `tsc --strict`.

## Touch points

Binder and/or annotation lowering paths that collect object and interface members; binder scope
insertion for `TK2451`; diagnostic definition/rendering for `TK2300`/`TK2451`; conformance fixtures
documenting the duplicate cases.

<!-- Origin: WU1 adversarial review, recorded in ../archive/sprint-2026-06-28-object-interface-signatures.md. -->
