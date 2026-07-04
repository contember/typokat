---
id: 23
title: Static method-level type parameters raise spurious TK2304
blocked-by: []
---

# 23 — `static of<U>(u: U)` raises spurious `TK2304 Cannot find name 'U'`

**Summary.** A `static` method with its own type parameters reports `TK2304` on the parameter
name — a false positive (safe direction, but noisy on idiomatic factory statics).

## Problem

Method-level type parameters are a documented deferral for *instance* methods (skipped without
error); on the `static` path the parameter annotation is lowered without the method's own
type-param frame, so `U` resolves to nothing and `TK2304` fires. Surfaced by the b20 adversarial
review (unrelated pre-existing bug).

## Approach / acceptance

Minimal: recognize method-level type params on statics enough to suppress the false `TK2304`
(lower `U` to an opaque parameter type or skip the member as out-of-subset, matching the
instance-method deferral). Full generic methods remain a later milestone. Acceptance: a static
generic factory produces no `TK2304`; instance-method behavior unchanged.

## Touch points

Class static-member lowering in `src/check/checker/classes.rs`.

<!-- Origin: b20 adversarial review (sprint 2026-07-04-class-completeness). -->
