---
id: 02
title: Accessibility not checked through a destructuring pattern
blocked-by: []
---

# 02 — Accessibility not checked through a destructuring pattern

**Summary.** False negative (under-reports, safe-ish direction). `private`/`protected` access checks
don't run when a member is reached through a destructuring pattern.

## Problem

`let { priv } = k` / `function f({ priv }: K)` doesn't run private/protected access checks. tsc
reports `TS2341`/`TS2445`; typokat is silent.

```ts
class K { private priv = 1; }
let { priv } = new K();   // typokat: silent   (tsc: TS2341)
```

## Approach / acceptance

Run the existing private/protected access check on each binding introduced by an object-destructuring
pattern, against the type being destructured. Acceptance: a fixture with destructured private /
protected members reports `TK2341` / `TK2445` matching tsc.

## Touch points

Destructuring-pattern checking; the access-control (private/protected) check.

<!-- Origin: official-suite finding F4. -->
