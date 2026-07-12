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

## Approach / acceptance

Corpus first over the operator table (arithmetic, `+` concatenation, comparison overlap
including narrowed unions and literal types, unary and update forms); implement in expression typing,
reusing the relation engine for TK2367 comparability. Cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/checker/expr.rs` (binary/unary typing), `src/diagnostics.rs` (new codes),
`src/relate/` (comparability entry for TK2367).

<!-- Origin: completion-roadmap review (2026-07-07); probe: "x" - 1 and non-overlapping === both silent. -->
