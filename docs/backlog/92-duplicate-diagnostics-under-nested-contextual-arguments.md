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

The conformance corpus does not catch it, but **not** because markers are presence-only:
`compare_fixture_output` step 1 (`tests/conformance.rs:681-693`) already compares the sorted
*multiset* of codes per line — `sorted_codes` does not dedup — so one `// error[TK2304]` marker on a
line emitting `2^d` copies fails today. Only the optional `: substring` is presence-like, and it is
subordinate to the code multiset. What hid this is simply that **no fixture nests contextual
arguments deeply enough** for the doubling to appear. No harness or marker-format change is needed.

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

Acceptance: the repro above reports exactly one `TK2304` at every depth from 1 to 12 on **all four**
shapes below, cross-checked against real `tsc --strict`; a fixture pins the count (the existing
marker format already suffices — see above); the corpus stays green and the official-suite ratchet
shows no regressions.

Deduplicating identical records at the reporting boundary would mask this more cheaply, but it
treats the symptom — the duplicate work is still done, and it would hide the next such bug. Prefer
the emission fix; if the cheap route is taken deliberately, record it in `divergences.md` with the
reason.

**Emissions and walks are two different problems that happen to share the exponent.** Making one walk
per level stop retaining fixes the duplication *everywhere*, but leaves walk counts — and therefore
time — unchanged. Collapsing base 2/3 → base 1 additionally needs the memo below. Both are required
before `contextual_rewalk_scaling_spec` goes green; do not expect the emission fix alone to do it.

## Which shapes are affected (measured, 2026-07-25/26)

**The duplication is universal — every shape doubles at `2^d`.** It is the two *retaining* walks, and
every shape has both. Verified at HEAD against `tsc 6.0.3 --strict`, which reports 1 in every cell:

| signature | duplicates at depth 1–6 | walks | debug time @ d=12 |
|---|---|---|---|
| `run<T>(step: (v: number) => T)` — structured | 2, 4, 8, 16, 32, 64 | base 3 | 30.2 s |
| `wrap<T>(value: { inner: T })` — structured | 2, 4, 8, 16, 32, 64 | base 3 | 21.7 s |
| `shapeOf<T>(shape: T)` — bare `T` | 2, 4, 8, 16, 32, 64 | base 2 | 0.64 s |
| `describe(fn: () => unknown)` — non-generic | 2, 4, 8, 16, 32, 64 | base 2 | 0.50 s |

The bare-`T` / non-generic rows are base 2 because candidate inference never re-walks them (no
inference phase at all, in the non-generic case) — that is a statement about **walks and time**, not
about emissions. An earlier revision of this item wrongly implied the two tracked together.

Why it matters for scoping: real `zod` is `object<T extends ZodRawShape>(shape: T)` and real
`describe`/`it` are non-generic, so the shapes that hang in the wild are the base-2 ones — and their
*time* problem is untouched by memoization. Structurally-embedded generics (`map`, `filter`, `then`,
`pipe`, `reduce`) are the base-3 ones, and nested generic callbacks realistically reach depth 3–5,
ceiling ~6, where the cost is 2.8–8 ms and imperceptible. Schema builders routinely reach depth 8–12:
40 zod-style schemas at depth 12 is 525 lines and **1.11 s**.

A verified prototype memoizing the effect-discarding walk (base 3 → base 2, output-neutral, 70–83×
at depth 14) was built and deliberately **not** landed: it fires zero times on every realistic shape
above, because those are already base 2. It is preserved at
`scratchpad/wu6/wu6-base2-memo.patch` (session `8c09d38b`) and may be useful once one walk per level
retains — at that point memoizing the other two is what takes this from base 2 to base 1.

## Touch points

`src/check/checker/calls.rs` (`check_call_arguments`, `retain_contextual_arrow_checks`),
`src/check/checker/expr.rs`, `src/check/checker/context.rs` (`CheckerEffects`, ticket allocation),
`tests/cases/` (the existing marker format already asserts counts — no harness change needed).

<!-- Origin: WU6 of the checker-scaling sprint, 2026-07-25 — found while memoizing the contextual
     re-walk; the memo could not reach base 1 because collapsing the walks deletes these duplicates.
     Counts and the tsc cross-check independently reproduced by the leader. -->
