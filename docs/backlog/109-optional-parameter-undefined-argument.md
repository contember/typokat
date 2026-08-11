---
id: 109
title: Optional parameter calls reject explicit undefined
---

# 109 — Optional parameter calls reject explicit `undefined`

**Summary.** Call target construction checks an optional parameter against its bare declared type.
It therefore rejects an explicit `undefined` argument even though omission and explicit
`undefined` are both valid in TypeScript's parameter model. The immutable `placetext` re-screen
exposes this as `TK2345` for a correctly published `number | undefined` binding passed to
`createRandom(seed?: number)`.

## Problem

`ParameterType` preserves the optional flag and call arity accepts omission, but
`call_argument_target_alternatives` returns `param.ty` for every fixed argument position. An
optional `p?: T` therefore supplies `T`, not `T | undefined`, to contextual checking and the final
argument relation. This is a safe false positive and the next general blocker in the pinned
`placetext` screen.

## Approach / acceptance

Spec first against pinned `tsc 6.0.3 --strict`: direct and forwarded `undefined`, a `T | undefined`
value, optional and defaulted parameters on functions/methods/constructors/overloads, and wrong
non-undefined values that must retain `TK2345`. Preserve optional/default arity,
rest-target selection, contextual literal checking, overload failure selection, and parameter
identity. The fixed target must admit `undefined` for optional and defaulted parameters; do not
widen the stored body-local parameter type or required parameters.

Acceptance removes `placetext`'s `src/core/generator.ts:36:48 TK2345` without changing its roots,
file accounting, resolutions, or other output identities.

## Touch points

`crates/typokat-check/src/check/checker/calls.rs` (fixed call argument targets), M32 signature-shape
fixtures, official-suite ratchet, and the immutable backlog-72 re-screen.

<!-- Origin: object-binding publication sprint WU3, 2026-08-10. -->
