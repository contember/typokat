---
id: 70
title: this-parameter typing + ThisType<T> (lib.d.ts prerequisite)
blocked-by: []
---

# 70 — `this`-parameter typing + `ThisType<T>`

**Summary.** Track A (model completeness). Explicit `this` parameters
(`fn(this: T, …)`) and the `ThisType<T>` marker are unmodeled, so any signature
that types `this` lowers to something silently permissive. Surfaced by the TS 6.0.3
`lib.d.ts` surface audit ([`lib-audit-6.0.3.md`](lib-audit-6.0.3.md)): a third
`lib.es5.d.ts` blocker for backlog `14`, alongside `43` (namespaces + declaration merging).

## Problem

`lib.es5.d.ts` uses `this`-parameter typing in the core function surface
(`(this:` appears 16×; `ThisType`/`ThisParameterType`/`OmitThisParameter` 7×):
`Function.prototype.apply/call/bind` (`apply<T, R>(this: (this: T) => R, …)`),
`CallableFunction`/`NewableFunction`, and `ObjectConstructor.defineProperty<T>(o, p,
attributes: PropertyDescriptor & ThisType<any>)`. typokat has no `this`-parameter
frame and no `ThisType` contextual-`this` rule, so these signatures would be loaded
permissively and poison every downstream check that goes through them — exactly the
"silently permissive construct" DoD condition 1 forbids. `ThisType<T>` is already
listed out-of-scope in the M28 utility-type divergences.

## Approach / acceptance

Model the `this` parameter as a distinct, non-positional signature slot (it does not
count toward arity and is checked contravariantly against the receiver), and give
`ThisType<T>` its contextual-`this` meaning inside object-literal methods. Reuse the
M32 signature-shape machinery for the slot; compose it with the shipped persistent generic
signature representation for `apply<T, R>`. Corpus first; cross-check `tsc 6.0.3 --strict`.

Acceptance: fixtures for `this`-typed methods/free functions, `Function.bind`-shaped
signatures, and a `ThisType`-annotated object literal check vs tsc; no regression on
the existing function/method corpora. Then the es5 `Function`/`Object` surface no
longer lowers permissively.

## Touch points

Signature lowering (`src/check/checker/annotations.rs` / `classes.rs` / `decls.rs`),
the relation engine for the `this` slot (`src/relate/relation.rs`), contextual `this`
in object literals (`src/check/checker/expr.rs`),
[`../reference/divergences.md`](../reference/divergences.md) (remove the `ThisType`
out-of-scope note when it lands).

<!-- Origin: TS 6.0.3 lib.d.ts surface audit, sprint-2026-07-10 WU7 (docs/backlog/lib-audit-6.0.3.md). -->
