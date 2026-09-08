---
id: 69
title: Signature rest parity tail
---

# 69 — Signature rest parity tail

**Summary.** Small M32 safe-direction tail: represented rest shapes work for calls,
relation, and fixed rest `infer`, but higher-order inference, callable-union synthesis,
and aggregate variadic-call diagnostics still over-report compared with `tsc`.

## Problem

The M32 adversarial review found no remaining dropped-error blocker after
`dc30516`, but two parity gaps remain:

- Generic call inference for embedded tuple rest params like `...args: [...T, boolean]`
  does not infer `T`; candidates are collected only when the whole rest parameter is a
  bare type parameter.
- Conditional tuple infer ignores a variadic rest segment on the **source** tuple, so
  `Tail<[string, ...number[]]>` resolves too narrowly / to `never` instead of `number[]`.
- Callable-union synthesis preserves a structured tuple rest when every constituent has
  equal fixed-prefix, variadic-middle, and required-suffix cardinalities. Constituents
  with unequal cardinalities, or with an optional prefix before the variadic segment, can
  conservatively reject a call that `tsc` accepts. The synthesis does not erase represented
  diagnostics; this is a parity-only false positive, not a silent channel.
- In official `variadicTuples2.ts`, aggregate tuple-rest call failure selection is correct in
  verdict but wrong in identity/cardinality: harness line 66 reports `TK2555` where strict tsc
  reports one `TS2345`, and line 71 reports the expected `TK2345` plus a duplicate `TK2345`.
  The other newly matched identities are label-lowering progress; these two residuals are
  call-shape diagnostics, not a relation or tuple-label failure.

All are safe-direction over-reports, but they are close enough to the M32 surface that
they should be fixed before broader `lib.d.ts` work depends on variadic tuple inference.

## Fixed source versus union-rest target

This item owns the eight pre-existing outer TK2322 identities in
`tests/cases/b44_as_const/contextual_rest_write_boundaries.ts`, lines 5/9/13/17/21/26/30/33,
column 7. Fixed positional callbacks are rejected against
`(...args: ["A", number] | ["B", string]) => void`; one source has an optional first parameter.
Strict TypeScript 6.0.3 accepts each outer relation. Pre-feature, R1a and both frozen publication/role
candidates each report one error at every site, even though contextual parameter/body checking
improves the source types in the feature candidates.

Leader disposition: defer these exact old relation identities from bounded as-const delivery to
this item, coordinated with R2b. The introduced body-property false positives at 24:29 and 28:29
remain required R2b repairs. Preserve the six correct recovered body diagnostics at 6:29, 11:29,
14:29, 18:29, 31:29 and 35:11, plus the clean direct-narrowing control at line 36. Do not remove
contextual checking to clear outer errors, or disable the mixed fixture wholesale.

The newly exposed `variadicTuples1.ts:363:13 TK2345` is owned primarily by
[71's spread-evidence repair](71-expression-inference-fn-tail.md#spread-evidence-and-contextual-callbacks).
Its identical embedded-rest header works with explicit scalar inputs; coordinate future rest inference
with that operand owner rather than treating every generic callback as this item's gap.

## Approach / acceptance

Extend rest-shape inference to flatten embedded tuple rest patterns in both call-site
and conditional modes, and preserve exact variadic layout while combining callable-union
signatures. Add fixtures cross-checked with `tsc 6.0.3 --strict` for:

- `function f<T extends unknown[]>(...args: [...T, boolean]): T` called with fixed
  prefix arguments.
- `type Tail<T> = T extends [unknown, ...infer R] ? R : never` over
  `[string, ...number[]]`.
- Callable unions whose structured tuple-rest constituents have unequal prefix or suffix
  cardinalities, plus optional-prefix forms before a variadic middle.
- The two exact `variadicTuples2.ts` identities above: line 66 becomes one aggregate `TK2345`,
  line 71 retains one `TK2345`, and neither line emits a scalar arity/per-argument duplicate.

Acceptance: the new probes match tsc verdicts without weakening the M32 dropped-error
guards around tuple-rest call arity or suppressing diagnostics for a represented signature.

## Touch points

`crates/typokat-check/src/check/infer/mod.rs`, `crates/typokat-check/src/check/infer/context.rs`, `crates/typokat-check/src/check/checker/calls.rs`,
`tests/cases/m32_signature_shape/`.

<!-- Origin: M32 signature-shape adversarial review follow-up (2026-07-09). -->
