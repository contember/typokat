---
id: 100
title: A composed condition (`&&`, `||`) narrows nothing in the branch it guards
blocked-by: []
---

# 100 — A composed condition (`&&`, `||`) narrows nothing in the branch it guards

**Summary.** `if (x !== null && anything) { … }` does not narrow `x` inside the branch at all — the
branch reads the **declared** type. Probes that `tsc 6.0.3 --strict` accepts produce `TK2322` false
positives. The idiom is one of the most common in TypeScript. M23 shipped `&&` narrowing its
*right-hand side* and its corpus covers only that; `&&` as an `if` condition was never tested.
Effort M. **Until this is fixed the checker is unusable on ordinary code**, so it outranks the
remaining library families.

## Problem

```ts
declare const nn: number | null;
declare const flag: boolean;

if (nn !== null)                     { const a: number = nn; }  // clean, correct
if (nn !== null && flag)             { const b: number = nn; }  // TK2322: 'null' not assignable
if (nn !== null && nn !== 0)         { const c: number = nn; }  // TK2322
if (nn !== null && nn > 0)           { const d: number = nn; }  // TK2322
if (flag && nn !== null)             { const e: number = nn; }  // TK2322
if (nn !== null && nn !== 0 && flag) { const f: number = nn; }  // TK2322
```

`tsc 6.0.3 --strict --target es2025` reports **nothing**; typokat reports **five**. Reproduced on the
release binary at `12a15b5` and unchanged by the operator work that found it.

The branch type is the **un-narrowed union**, not an inverted narrow. An earlier revision of this
item claimed the type came out as `null`, the false-branch type; that was a misreading of the
diagnostic, which names the offending union *constituent* rather than the whole source type.
`if (nn !== null && flag) { const g: null = nn; }` reports `Type 'number' is not assignable to type
'null'`, which settles it.

It fires in either operand position (`a && x !== null` too) and for any right operand — a bare
boolean, an equality, a relational comparison. Nesting the same guards
(`if (nn !== null) { if (nn > 0) { … } }`) narrows correctly, so the defect is in how `&&` composes
into the branch, not in the individual guards.

The corpus (commit `429e3dc`) settles the shape of the defect: of 86 markers diffed against `tsc`,
**65 lines are RED and every one is an over-report**. There is no dropped-error mirror in the `else`
branch or in `||` — the composed condition narrows nothing anywhere, so the complementary branches
are accidentally correct. That makes them the regression net for the fix rather than a second
defect: a fix that pushes the then-branch fact into the wrong branch deletes 31 real diagnostics.

## Why the corpus did not catch it

`tests/cases/m23_unstructured_narrowing/logical_ops.ts` tests exactly one shape —
`x !== null && takesString(x)`, i.e. `&&` narrowing its own right-hand side — plus the `||` and
ternary equivalents. All pass. No fixture anywhere puts an `&&` in an `if` condition over a union.
This is a **corpus hole**, not a regression: the M23 milestone shipped the tested half.

That is the same failure mode as [`96`](./96-randomized-differential-corpus.md): a hand-written
corpus covers the cases someone thought of. A differential corpus would have found this on day one.

## Approach / acceptance

The corpus is written and committed (`429e3dc`, `tests/cases/b100_logical_condition_narrowing/`); it
is the acceptance spec and it is RED.

The defect is structural, not a missing case. `build_flow_if` builds the test expression, then asks
`analyze_guard` for **one** `GuardFact` — a single `(symbol, op, polarity)`. A composed condition
does not reduce to one fact, so the recursion has to carry the two continuations instead:
`build_flow_condition(test) -> (true_flow, false_flow)`, with `&&` chaining the right operand under
the left's true edge and joining the false edges, `||` mirrored, `!` swapping, and the leaf case
falling back to today's `analyze_guard` + `flow_condition` pair. Expression position stays as it is
by joining the two returned edges — that is what `build_flow_logical` already computes, so it
becomes a caller of the new builder rather than a separate path. The flow-node CFG is the single
narrowing model per [`invariants.md`](../reference/invariants.md) §1; fix it there rather than
special-casing `if` conditions.

Acceptance: 62 of the 65 RED lines go green; the 31 complementary-branch markers all survive;
`m23_unstructured_narrowing` stays green; official-suite ratchet shows no regression. The remaining
3 (`unmodeled_loop_flow_deferred.ts`) belong to [`51`](./51-narrowing-tail.md) and get re-homed as a
documented over-report in [`divergences.md`](../reference/divergences.md) when the fix lands.

## Touch points

`src/check/checker/flowgraph/exprs.rs`, `src/check/checker/flowgraph/mod.rs`,
`src/check/checker/flowgraph/nodes.rs`, `src/check/checker/narrowing.rs`,
`tests/cases/b100_logical_condition_narrowing/`.

<!-- Origin: found while fixing backlog 45's operator result typing, 2026-07-26 — the implementing
     agent hit it in an unrelated probe and flagged it rather than working around it. Confirmed
     pre-existing by rebuilding at 12a15b5. -->
