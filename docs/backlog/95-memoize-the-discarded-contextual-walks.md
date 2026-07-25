---
id: 95
title: Memoize the two discarded contextual argument walks
blocked-by: []
---

# 95 — Memoize the two discarded contextual argument walks

**Summary.** An argument nested `d` levels deep is still walked `3^d` times (`2^d` for bare-type-variable
and non-generic shapes). Since `243a878` only **one** of the three walks per level retains effects, so
the other two can be memoized without any effect-replay problem — the precondition that blocked this
before. Target: base 1, i.e. one walk per nesting level. Effort M.

## Problem

`src/check/checker/calls/contextual_rewalk_scaling_spec.rs` is red and pins the law: walks are exactly
`(3^d - 1)/2` per phase with two phases firing, so `3^d - 1` total — 80 at depth 4, 6,560 at depth 8,
confirmed on both structurally-embedded-generic shapes. `raw_call_argument_walks` and
`generic_full_inference_runs` follow the same curve; they are downstream of the re-walk, not separate
costs.

Wall clock, release, on a 15-line file: depth 12 ≈ 1.3 s, depth 14 ≈ 12 s. `243a878` improved these
by a constant (depth-12 nested arrows 3153 → 2252 ms) purely by not materializing 4,095 duplicate
diagnostics; the exponent is untouched, and the guard's own runtime went 41.6 s red → 43.2 s green.

Which shapes are base 3 versus base 2 is measured in the table `243a878` left in
`tests/cases/README.md`: parameters that *structurally contain* the type variable
(`run<T>(step: (v: number) => T)`, `wrap<T>(value: { inner: T })`) re-walk during candidate inference
as well; a **bare** type variable (`shapeOf<T>(shape: T)`, real `zod`) and non-generic callbacks
(`describe`/`it`) do not, and are base 2. The base-2 shapes are the ones that hang in the wild —
40 zod-style schemas at depth 12 is 525 lines and ~1.1 s — so a fix that only helps base 3 helps the
benchmark and not the user.

## Why this is now unblocked

The earlier attempt could only memoize *one* walk, because two of three retained effects and serving a
retaining walk from a memo requires replaying them — `CheckerEffects` is not `Clone`, and re-merging a
batch reuses `UserRecordTicket`s, giving two records the same replay key, which
[`invariants.md`](../reference/invariants.md) §1 forbids. `243a878` made the committed walk the only
one that reports, so **two of the three now discard**. A memo over a discarding walk returns only
`(TypeId, Span)`, which is the entire observable output, so replay equivalence holds by construction.

A verified prototype covering *one* discarding walk is preserved at
`scratchpad/wu6/wu6-base2-memo.patch` (session `8c09d38b`): base 3 → base 2, output-neutral over 465
fixtures in two formats and 8 corpora, 70–83× at depth 14. It was not landed because at the time it
fired zero times on the shapes that hang. **Read it before designing, but do not assume it applies
unchanged** — its memo key was built for the old two-retaining-walk world, and its state lived in a
`thread_local!`, which is wrong for this architecture and must instead be a field on `Pass`.

## Approach / acceptance

Memoize both discarding walks. The key must be sound across the candidate-inference and
candidate-trial phases — the prototype found they carry different context `TypeId`s (uninstantiated
vs instantiated), so they could not collide, but re-verify that rather than inheriting it.

Acceptance: `contextual_rewalk_scaling_spec` **green**, which needs the ratio-of-ratios bound it now
carries (a fixed factor bound cannot separate a small-base exponential from a polynomial — that is
why it was retuned in `12b44e6`). Its non-vacuity assertion must still hold: the walks must still
reach every nesting level, so a "fix" that declines to walk fails. Diagnostics byte-identical over
`tests/cases/` and the bench corpora; official-suite ratchet at 0 regressions; the `b92_*` corpus
stays green.

Report the depth at which the shapes stop being flat, and whether the **base-2** shapes improve —
that is the acceptance question that matters, not the depth-14 headline.

## Touch points

`src/check/checker/calls.rs`, `src/check/checker/expr.rs`, `src/check/checker/mod.rs` (`Pass`),
`src/check/checker/calls/contextual_rewalk_scaling_spec.rs`.

<!-- Origin: split out of backlog 92 when its emission half landed as 243a878. 92 fixed which walk
     reports; this fixes how many walks happen. -->
