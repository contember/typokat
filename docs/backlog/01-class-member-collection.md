---
id: 01
title: Class member-collection drops parameter properties + inferred fields
blocked-by: []
---

# 01 — Class member-collection drops parameter properties + inferred fields

**Summary.** REAL BUG, highest-value fix. Class members declared without an explicit type annotation
aren't collected, so every access reports a spurious `TK2339` (and the member's access-control / type
checks never run). Unsound in the **false-positive** direction — typokat rejects valid, idiomatic
code.

## Problem

Two common forms trip it:

- **parameter properties** — `constructor(public x: number, private y: string)`
- **initializer-inferred fields** — `f = () => 1`, `g = 2` (no annotation)

Repro (faithful binary):
```ts
class C { constructor(public x: number, private y: string) {} }
const c = new C(1, "a");
const n: number = c.x;   // typokat: TK2339 'x' does not exist  (tsc: ok)
c.y;                     // typokat: TK2339 'y' does not exist  (tsc: TS2341 private)

class D { f = () => 1; g = 2; }
new D().g;               // typokat: TK2339 'g' does not exist  (tsc: ok)
```

Unlike the object/interface-signature gap (item 05), this is a hole in *implemented* class support.
It is the single largest source of over-reports: ~150 spurious `TK2339` across ~38 corpus files (6
parameter-property, ~32 initialized-field). The hand-written corpus missed it because its class
fixtures all use **annotated** fields (`private x: string`), which collect fine.

## Approach / acceptance

Collect class members in both un-annotated forms — parameter properties (infer the member from the
constructor parameter + its modifier) and initializer-inferred fields (infer the field type from the
initializer). Once collected, the existing access-control + type checks must run on them (so `c.y`
becomes `TS2341` private, not `TK2339`). Acceptance: a new `tests/cases/` fixture exercising both
forms, plus the repro above checking clean where tsc is clean and reporting the *right* code where it
isn't; no regression on the existing class corpus.

## Touch points

Class-phase member collection (the binder/checker path that gathers fields/methods); the
access-control and member-type checks that consume the collected members.

<!-- Origin: official-suite finding F3. -->
