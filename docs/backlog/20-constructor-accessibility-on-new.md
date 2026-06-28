---
id: 20
title: Constructor accessibility on direct new expressions
blocked-by: []
---

# 20 — Constructor accessibility on direct new expressions

**Summary.** Pre-existing class-checking gap: direct `new C()` does not enforce private/protected
constructor accessibility, so typokat can miss `TS2673`/`TS2674`.

## Problem

Typokat records class constructor signatures in `class_ctors` and uses them for direct construction,
but it does not currently carry or check the constructor's `private`/`protected` accessibility on a
`new C()` expression. As a result, these direct constructions are accepted today:

```ts
class PrivateCtor { private constructor() {} }
class ProtectedCtor { protected constructor() {} }

new PrivateCtor();   // tsc TS2673
new ProtectedCtor(); // tsc TS2674
```

This is a pre-existing gap, not introduced by F1/WU3. It was surfaced by the WU3 review that fixed
static-side assignability of classes with private/protected constructors to public construct
signature targets.

## Approach / acceptance

Carry constructor visibility alongside the existing `class_ctors` entry and check it in the direct
class `new` path. A `private` constructor should be constructable only within its declaring class; a
`protected` constructor should be constructable only within the declaring class or subclasses.
Acceptance: fixtures matching tsc's `TS2673`/`TS2674` behavior for direct `new C()`, while existing
public-constructor and WU3 construct-signature behavior stays unchanged.

## Touch points

Class lowering in `src/check/checker/classes.rs`, `ClassInfo` in
`src/check/checker/context.rs`, and direct class construction in `src/check/checker/calls.rs`.

<!-- Origin: WU3 review. -->
