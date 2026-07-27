---
id: 95
title: Memoize contextual argument walks
blocked-by: []
---

# 95 — Memoize contextual argument walks

**Summary.** An argument nested `d` levels deep is walked `3^d` times. A first attempt
(`412f321`) reached a quadratic law but was **unsound and reverted** (`40540a1`): its raw-walk memo
key omitted the ambient contextual parameter binding, dropping diagnostics and inventing others.
The *contextual* memo alone is proven sound and worth 73× on nested arrows. Effort M–L.

## Problem

`src/check/checker/calls/contextual_rewalk_scaling_spec.rs` is red and pins the law: `(3^d - 1)/2`
per phase, two phases firing, `3^d - 1` total — 80 at depth 4, 6,560 at depth 8.

The shapes that hang in the wild are **base 2**, not base 3: `shapeOf<T>(shape: T)` (real `zod`) and
non-generic `describe`/`it` never run a candidate-inference walk. 40 zod-style schemas at depth 12 is
521 lines and ~550 ms. A fix that only helps structurally-embedded generics helps the benchmark and
not the user — that is the acceptance question.

## What the reverted attempt established

Three walks per level, and **only one of them discards**:

| walk | retains? | memoizable |
|---|---|---|
| raw argument walk | **held**, discarded only if superseded | not naively — see below |
| candidate inference / trial | discards | yes, and proven sound |
| committed check | retains, reports | never |

So memoizing only the discarding walk gives base 3 → base 2 and **nothing else**, and does nothing at
all for the base-2 shapes. The remaining exponent is `raw × committed`, and the raw walk retains.

### Why the raw memo was unsound — the trap to avoid

`RawArgumentWalkKey` was `WalkSite { module, scope, span.start, span.end, contextual_this, class }`,
documented as "everything else a walk reads is lexically fixed by the node — except `this` and the
current class". **That premise is false**, and the same commit disproved it: `expr.rs` snapshots and
restores `decl_types` around the arrow walk *precisely because* the walk rebinds the contextual
parameter.

So the phase-1 walk, run with the parameter unbound (implicit `any`), was served to the phase-2
committed walk running with it bound. Three lines, no generics:

```ts
declare function plain(step: (value: number) => void): void;
declare function wantsStrFn(f: () => string): void;
plain(p0 => { wantsStrFn(<U,>() => p0); });
```

`412f321~1` reports `TK2345`; `tsc 6.0.3 --strict` reports `TS2322`; `412f321` reports **nothing** —
`() => any` is assignable where `() => number` is not. Recovering the *records* does not save it: the
returned `TypeId` is discarded, and it has already flowed into `arg_types`, overload selection and
inference. It also invents errors: `each(p0 => { over2({ a: p0 }); })` against two `over2` overloads
reports `TK2322` where base and `tsc` report nothing.

Blast radius from the review's fuzzers: **68 of 400** files differ at depth 1–3, **102 of 700** inside
a class method — dropped diagnostics, false positives, changed overload verdicts, wrong types in
messages.

## What survives, measured

- **The contextual memo alone is sound**: 0 of 1,100 fuzz files and 0 of 471 fixtures differ from
  base. Worth **73×** on nested arrows at depth 14 (11,996 → 163 ms) on its own. It does *not* help
  zod or `describe`.
- A probe with a sound raw key (contextual-binding generation counter added to the key) was
  output-identical to base on all 182 differing files and **kept the zod win**: 541 → 14.8 ms.
- The `describe` win in the reverted commit (360 → 10.5 ms) **did not survive** a sound key
  (375 ms). It was fast because it was wrong.

## Approach / acceptance

Two viable routes, in order of confidence:

1. **Land the contextual memo alone** — sound today, 73× on arrows, low risk. Does not reach the
   guard's bar and does not help the shapes that hang. A legitimate partial.
2. **Add a sound raw-walk memo.** The key must name *every* piece of ambient state a contextual
   re-walk can rebind — parameter bindings, type-parameter frames, receiver, `building_template` —
   not just the one a probe happened to need. A generation counter bumped whenever a contextual walk
   rebinds anything is the shape that worked in the probe. Do not re-derive the key from first
   principles and trust the derivation; the last one was wrong in its stated reasoning.

Acceptance: `contextual_rewalk_scaling_spec` green with its ratio-of-ratios bound and non-vacuity
assertion intact; the zod and `describe` shapes both improve; and — non-negotiable — **the randomized
differential corpus below shows zero divergence**, because `tests/cases` demonstrably cannot see this
bug class.

## The corpus hole this exposed

`tests/cases` (471 fixtures), eight bench corpora, the official suite, and a 2,193-binding
inferred-type probe **all showed zero diff** on a change that alters output on ~15 % of randomly
generated nested-contextual programs. The trigger shape — an argument a contextual re-walk can
supersede, nested inside a contextually typed callback, whose value depends on that callback's
parameter — simply does not occur in the corpus. The randomized differential harness that closes
this hole shipped as [`tooling/differential/`](../../tooling/differential/README.md); running it in
reference-binary mode against the pre-change build is a **required** gate for this item, per
[`dev-method.md`](../reference/dev-method.md) §1.

## Touch points

`src/check/checker/calls.rs`, `src/check/checker/expr.rs`, `src/check/checker/mod.rs` (`Pass`),
`src/check/checker/context.rs`, `src/check/checker/calls/contextual_rewalk_scaling_spec.rs`.

<!-- Origin: split from backlog 92 when its emission half landed as 243a878. First attempt 412f321
     reverted by 40540a1 after independent adversarial review; findings folded in here. -->
