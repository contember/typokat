---
id: 56
title: Instantiation cycles resolve silently to the error type (no TK2589/TK2456)
---

# 56 — Silent instantiation cycles

**Summary.** `type Loop<T> = T extends string ? Loop<T> : never; const x: Loop<string> =
42;` produces **zero diagnostics** (leader-verified 2026-07-07; tsc: TS2589) — and so does
the mutual form (`A<T>` → `B<T>` → `A<T>`). The in-flight re-entry short-circuits to the
error type with no flag and no diagnostic (`eval.rs:328-331` conditional, `:515-518`
instantiation), bypassing the budget entirely, and the error type then accepts anything.

## Problem

A genuine cycle is indistinguishable from a successful evaluation at the check surface:
`x` gets error, every use is silently accepted. TK2456 doesn't fire either (it is
check-surface-only — the self-reference here is in a branch). Secondary discipline gap
from the same trace: ancestors of the re-entered id ARE durably memoized with the baked-in
error (`SetMemo` guards only on `exhausted`, not on cycle-tainted values) — deterministic
today, but it contradicts the module header's stated invariant; fix or re-state it.

## Approach / acceptance

On in-flight re-entry, surface a diagnostic (TK2589 to match tsc's verdict on these
shapes) and/or taint the result so ancestors don't memoize it as final. Corpus:
direct-recursive and mutual-recursive conditional aliases, plus a
cycle-then-legitimate-reuse fixture proving no poisoning. Cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/checker/eval.rs` (in-flight re-entry paths, `SetMemo` taint discipline).

<!-- Origin: cross-cutting soundness review 2026-07-07 (evaluator reviewer #2), leader-verified. -->
