---
id: 45
title: Operator and comparison typing (TK2362/2363/2365, TK2367)
---

# 45 — Operator & comparison typing

**Summary.** Arithmetic and comparison operands are unchecked: `"x" - 1` and `1 === "x"`
both pass silently (probe 2026-07-07; tsc: TS2362 + TS2367). A silent-FN family in Tier A
of the scope map.

## Problem

Binary/unary expression typing doesn't validate operand types: arithmetic operators need
number/bigint/enum operands (TK2362/TK2363 per side, TK2365 for the general mismatched
case), equality comparison needs overlapping types (TK2367 — the relation engine's
comparability), `+` has its own string/number union rules, and unary `-`/`+`/`~` have
numeric-operand rules. Prefix/postfix `++` and `--` compatibility and result typing belong here
too. `in`/`instanceof` operand rules belong here (pin the exact code
set against tsc in the spec).

## The sharper symptom: the *result* is the error type

**Measured 2026-07-26**, and worse than "operands are unchecked". `infer_binary`
(`crates/typokat-check/src/check/checker/expr.rs:689`) models only `+`; every other binary operator returns `wk.error`.
The error type then absorbs downstream assignments silently, so this family also eats ordinary
`TK2322`s that have nothing to do with operator rules:

```ts
declare const n: number;
const a: string = n + 2;   // TK2322 — reported
const b: string = n * 2;   // silent
const c: string = n - 2;   // silent
const d: string = n / 2;   // silent   (same for % & | ^ << >>)
```

`tsc 6.0.3 --strict` reports **seven** `TS2322`; typokat reports **one**. Confirmed on the release
binary against the pinned `tsc`. It also poisons inference: in
`numbers.map((value) => value * 2)` the callback returns the error type, so `U` infers `any` and
assigning the result to `string[]` passes — which is why
`tests/cases/b14_full_lib_loading/arrays_tuples_readonly.ts` fails for a reason that has nothing to
do with the library.

This is the same defect as the missing operand diagnostics and belongs to the same fix, but it is
the half that costs real diagnostics on ordinary code. Whoever implements this item should pin the
result type of every operator, not only the operand rules.

## Approach / acceptance

Corpus first over the operator table (arithmetic, `+` concatenation, comparison overlap
including narrowed unions and literal types, unary and update forms); implement in expression typing,
reusing the relation engine for TK2367 comparability. Cross-check tsc 6.0.3 --strict.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs` (binary/unary typing),
`crates/typokat-diagnostics/src/diagnostics/mod.rs` (new codes),
`crates/typokat-relate/src/relate/` (comparability entry for TK2367).

<!-- Origin: completion-roadmap review (2026-07-07); probe: "x" - 1 and non-overlapping === both silent. -->
