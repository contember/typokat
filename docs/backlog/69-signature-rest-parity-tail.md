---
id: 69
title: Signature rest parity tail
---

# 69 — Signature rest parity tail

**Summary.** Small M32 safe-direction tail: represented rest shapes work for calls,
relation, and fixed rest `infer`, but higher-order inference and aggregate variadic-call
diagnostics still over-report compared with `tsc`.

## Problem

The M32 adversarial review found no remaining dropped-error blocker after
`dc30516`, but two parity gaps remain:

- Generic call inference for embedded tuple rest params like `...args: [...T, boolean]`
  does not infer `T`; candidates are collected only when the whole rest parameter is a
  bare type parameter.
- Conditional tuple infer ignores a variadic rest segment on the **source** tuple, so
  `Tail<[string, ...number[]]>` resolves too narrowly / to `never` instead of `number[]`.
- In official `variadicTuples2.ts`, aggregate tuple-rest call failure selection is correct in
  verdict but wrong in identity/cardinality: harness line 66 reports `TK2555` where strict tsc
  reports one `TS2345`, and line 71 reports the expected `TK2345` plus a duplicate `TK2345`.
  The other newly matched identities are label-lowering progress; these two residuals are
  call-shape diagnostics, not a relation or tuple-label failure.

Both are safe-direction over-reports, but they are close enough to the M32 surface that
they should be fixed before broader `lib.d.ts` work depends on variadic tuple inference.

## Approach / acceptance

Extend rest-shape inference to flatten embedded tuple rest patterns in both call-site
and conditional modes. Add fixtures cross-checked with `tsc 6.0.3 --strict` for:

- `function f<T extends unknown[]>(...args: [...T, boolean]): T` called with fixed
  prefix arguments.
- `type Tail<T> = T extends [unknown, ...infer R] ? R : never` over
  `[string, ...number[]]`.
- The two exact `variadicTuples2.ts` identities above: line 66 becomes one aggregate `TK2345`,
  line 71 retains one `TK2345`, and neither line emits a scalar arity/per-argument duplicate.

Acceptance: the new probes match tsc verdicts without weakening the M32 dropped-error
guards around tuple-rest call arity.

## Touch points

`crates/typokat-check/src/check/infer/mod.rs`, `crates/typokat-check/src/check/infer/context.rs`, `crates/typokat-check/src/check/checker/calls.rs`,
`tests/cases/m32_signature_shape/`.

<!-- Origin: M32 signature-shape adversarial review follow-up (2026-07-09). -->
