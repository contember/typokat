---
id: 44
title: satisfies and as-const semantics
---

# 44 — `satisfies` + `as const`

**Summary.** Both assertion-adjacent operators are unchecked: `{ a: 1 } satisfies
{ a: string }` passes silently (tsc: TS2322) and `as const` produces no readonly literal
types (`o.a = 2` after `as const` passes; tsc: TS2540) — probes 2026-07-07. Silent-FN
families, and both are official-suite syntax gates.

## Problem

`satisfies` must check the expression against the target type (freshness/excess rules
apply) while keeping the expression's own, narrower type for later use. `as const` must
produce the const-asserted type: literals retained (no widening), members `readonly`,
array literals as readonly tuples. Sibling of backlog `33` (`as`-cast typing) — same
expression-typing path; keep the divergence ledger coherent across the two.

## Approach / acceptance

Corpus first: satisfies pass/fail shapes (excess property under `satisfies`, the narrow
type surviving afterwards), as-const object/array/nested shapes, write-through-readonly
TK2540, interaction with M30 contextual typing and literal widening. Cross-check tsc
6.0.3 --strict; open the `syntax:satisfies` and `syntax:as-const` official-suite gates.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs` (both operators), the freshness/excess path,
`crates/typokat-types/src/types/repr.rs` (readonly array/tuple forms — coordinate with backlog `24`'s tuple
work), official-suite `OUT_OF_SCOPE_SYNTAX`.

<!-- Origin: completion-roadmap review (2026-07-07); probes: satisfies mismatch + as-const write both silent. -->
