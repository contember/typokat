---
id: 92
title: One error inside nested contextual arguments is reported 2^d times
blocked-by: []
---

# 92 — One error inside nested contextual arguments is reported 2^d times

**Summary.** A single error nested `d` levels deep inside contextually typed arguments is emitted
`2^d` times; `tsc` emits it once. At depth 10 — an ordinary callback or promise chain — one typo
produces 1,024 identical diagnostics. Severity: HIGH (quality, not soundness). Effort M.

## Problem

Per nesting level the checker walks an argument expression three times, and **two of those retain
effects**: the raw argument walk and the committed `check_call_arguments` walk
(`retain_contextual_arrow_checks = true`). The third — candidate inference — discards its records.
Both retaining walks emit, so emissions double per level while time triples (the `3^d` law pinned by
`src/check/checker/calls/contextual_rewalk_scaling_spec.rs`).

Measured at HEAD `543d635`, release binary, counting `error[TK2304]` records:

| depth | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|
| nested arrows | 2 | 4 | 8 | 16 | 32 |
| nested object literals | 2 | 4 | 8 | 16 | 32 |
| `tsc 6.0.3 --strict` | 1 | 1 | 1 | 1 | 1 |

```ts
declare function run<T>(step: (value: number) => T): T;
const nested = run(v0 => run(v1 => run(v2 => undeclaredThing)));   // 8x TK2304, tsc: 1x TS2304
```

The copies are byte-identical — same code, same message, same span — so this is pure duplication,
not several distinct errors that happen to share a line. It is not recorded in
[`divergences.md`](../reference/divergences.md).

The conformance corpus does not catch it: markers assert that an expected diagnostic *is present*
by substring, not how many times, and no fixture nests contextual arguments deeply enough for the
doubling to be visible.

## Approach / acceptance

Emit once. The natural fix is for exactly one walk per level to retain effects, which also unblocks
memoizing the rest — see the sprint's WU6, where the `3^d` walk cost could only be reduced to `2^d`
precisely because the two retaining walks cannot be collapsed without changing output.

Two constraints found while prototyping that fix:

- Serving a retaining walk from a memo requires replaying its **effects**, not just its type.
  `CheckerEffects` (`src/check/checker/context.rs:223`) is not `Clone`, and re-merging one batch
  twice reuses the same `UserRecordTicket`s — two records with an identical
  `(module_ordinal, source_start, event_ordinal, record_ordinal)` replay key, which
  [`invariants.md`](../reference/invariants.md) §1 does not allow.
- So this is a change to the event/ticket model, not a local edit. Deciding *which* walk retains is
  the design question: the committed walk sees instantiated contextual types, the raw walk does not.

Acceptance: the repro above reports exactly one `TK2304` at every depth from 1 to 12, cross-checked
against real `tsc --strict`; a fixture pins the count, which requires the conformance harness to
assert occurrence counts rather than mere presence (`tests/cases/README.md` — the same gap that hid
[`90`](./90-assignability-span-precision.md)); the corpus stays green and the official-suite ratchet
shows no regressions.

Deduplicating identical records at the reporting boundary would mask this more cheaply, but it
treats the symptom — the duplicate work is still done, and it would hide the next such bug. Prefer
the emission fix; if the cheap route is taken deliberately, record it in `divergences.md` with the
reason.

**The duplicate count and the residual time exponent are the same `2^d`.** One retaining walk per
level fixes both at once, so this item closes the performance problem too and no separate perf item
is needed after it.

## Which shapes are affected (measured, 2026-07-25)

The discriminator is whether the parameter type is a **bare type variable** or **structurally
contains one**:

| signature | shape | walks |
|---|---|---|
| `object<T>(shape: T)` — bare `T` | candidate inference never re-walks | already base 2 |
| `wrap<T>(value: { inner: T })`, `run<T>(step: (v: number) => T)` | structured | base 3 |
| `describe(fn: () => void)` — non-generic | no inference phase | already base 2 |

This matters for scoping the fix: real `zod` is `object<T extends ZodRawShape>(shape: T)` and real
`describe`/`it` are non-generic, so the shapes that actually hang in the wild are the **base-2**
ones — and base 2 is exactly what this item removes. Structurally-embedded generics (`map`, `filter`,
`then`, `pipe`, `reduce`) are the base-3 ones, and nested generic callbacks realistically reach depth
3–5, ceiling ~6, where the cost is 2.8–8 ms and imperceptible. Schema builders routinely reach depth
8–12: 40 zod-style schemas at depth 12 is 525 lines and **1.11 s**.

A verified prototype memoizing the effect-discarding walk (base 3 → base 2, output-neutral, 70–83×
at depth 14) was built and deliberately **not** landed: it fires zero times on every realistic shape
above, because those are already base 2. It is preserved at
`scratchpad/wu6/wu6-base2-memo.patch` (session `8c09d38b`) and may be useful once one walk per level
retains — at that point memoizing the other two is what takes this from base 2 to base 1.

## Touch points

`src/check/checker/calls.rs` (`check_call_arguments`, `retain_contextual_arrow_checks`),
`src/check/checker/expr.rs`, `src/check/checker/context.rs` (`CheckerEffects`, ticket allocation),
`tests/conformance.rs` if occurrence counts are asserted, `tests/cases/`.

<!-- Origin: WU6 of the checker-scaling sprint, 2026-07-25 — found while memoizing the contextual
     re-walk; the memo could not reach base 1 because collapsing the walks deletes these duplicates.
     Counts and the tsc cross-check independently reproduced by the leader. -->
