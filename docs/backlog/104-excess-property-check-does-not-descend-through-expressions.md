---
id: 104
title: The excess-property check does not descend through a ternary or logical expression
blocked-by: []
---

# 104 — The excess-property check does not descend through a ternary or logical expression

**Summary.** `const x: Shape = flag ? { kind: "circle" } : { kind: "square", extra: 1 }` is silent;
`tsc` reports `TS2322` with the excess-property elaboration. `check_excess_properties` is a separate
syntax-directed walk that only recognises an object literal *directly* in the checked position, so an
arm of a ternary — or an operand of `&&`/`||`/`??` — is never reached. A dropped error, effort S.

## Problem

```ts
type Shape = { kind: string };
declare const flag: boolean;

const a: Shape = { kind: "square", extra: 1 };                      // TK2353 — correct
const b: Shape = flag ? { kind: "circle" } : { kind: "square", extra: 1 };  // SILENT
const c: Shape = flag && { kind: "square", extra: 1 };              // partially reported
```

`tsc 6.0.3 --strict` reports all three. typokat reports `a` correctly and misses `b` entirely. `c` is
the interesting one: typokat *does* report it, but only for the `boolean` constituent that `&&`
contributes — the excess property on the object operand is still invisible, so the diagnostic is
right by accident and would vanish if the left operand were non-falsy.

The freshness that makes an object literal subject to the check is a property of the literal, and it
survives being an arm — `tsc` treats each arm as a fresh literal against the contextual type. typokat
loses it at the first non-literal node.

## Why it only surfaced now

Until backlog `101` shipped, a ternary and a logical expression produced the **error type**, so nothing downstream of them was checked at all and this gap
was invisible behind a much larger one. Giving those forms real result types made the assignability
check live; the excess-property walk is the one consumer that did not come with it.

## Approach / acceptance

`context_can_shape_fresh_literal` (`src/check/checker/expr.rs`) already recurses into arms and
operands so the contextual re-walk reaches them — the excess-property walk needs the same descent,
against the same contextual target. Do not build a second freshness model; reuse whatever
`check_excess_properties` already treats as the checked position.

Corpus first per [`dev-method.md`](../reference/dev-method.md) §1: both ternary arms, each logical
operand, nested compositions, an arm that is *not* fresh (a variable reference, which must stay
silent), an arm whose excess property is legal because the target admits it, and the array/tuple
element positions. Cross-check every marker against `tsc 6.0.3 --strict`.

## Touch points

`src/check/checker/expr.rs`, `src/check/checker/assignment.rs`,
`tests/cases/b101_conditional_logical_values/`.

<!-- Origin: reported 2026-07-27 by the backlog-101 implementation work unit, which found it while
     writing that corpus and correctly declined to encode it as a divergence without an owner. -->
