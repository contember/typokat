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

## Touch points

`src/check/checker/calls.rs` (`check_call_arguments`, `retain_contextual_arrow_checks`),
`src/check/checker/expr.rs`, `src/check/checker/context.rs` (`CheckerEffects`, ticket allocation),
`tests/conformance.rs` if occurrence counts are asserted, `tests/cases/`.

<!-- Origin: WU6 of the checker-scaling sprint, 2026-07-25 — found while memoizing the contextual
     re-walk; the memo could not reach base 1 because collapsing the walks deletes these duplicates.
     Counts and the tsc cross-check independently reproduced by the leader. -->
