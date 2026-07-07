---
id: 55
title: Template evaluation memoizes results computed under an exhausted budget
---

# 55 — Template memo poisoning under budget exhaustion

**Summary.** The template-literal evaluation path is the one evaluator path that bypasses
the `SetMemo` discipline: `finish_template_with_holes` inserts into the memo directly
(`eval.rs:980-984`), so a hole that resolved to the error type *only because an unrelated
earlier evaluation exhausted the shared TK2589 budget* is committed durably. The memo is
pass-wide and the node hash-consed, so **every later annotation interning the same
template silently resolves to error for the rest of the module** — error suppresses all
downstream diagnostics. Probe-verified (2026-07-07, leader-reproduced): after a
budget-exhausting alias, `const later: Fine<string> = "no"` goes silent; the control file
without the exhausting line reports TK2322. **HIGH** — this is exactly the
provisional-result class invariants §1 bans (the relation engine's cache-poisoning bug,
transplanted to the evaluator).

## Problem

`eval_template` (`eval.rs:920-948`) has no `exhausted`/in-flight gate and schedules
`Task::FinishTemplate` without a guarding `Task::SetMemo`; the error-hole insert is the
leak. The conditional / instantiation / mapped / keyof paths were probe-verified NOT to
poison (their reuse after a TK2589 abort still errors correctly).

## Approach / acceptance

Gate `eval_template` like the other five node kinds and route its memoization through
`SetMemo` (which already refuses to commit when `exhausted`). Acceptance: the poison
probe reports both TK2589 *and* the downstream TK2322; a fixture pins
exhaustion-then-reuse for every node kind (regression net for the verified-clean four);
no change on the m27/m28 corpus.

## Touch points

`src/check/checker/eval.rs` (`eval_template`, `finish_template_with_holes`, `SetMemo`).

<!-- Origin: cross-cutting soundness review 2026-07-07 (evaluator reviewer #1), leader-verified. -->
