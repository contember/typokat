---
id: 22
title: new through a parenthesized callee or alias misses class-keyed checks
blocked-by: []
---

# 22 — `new (C)()` / `const A = C; new A()` miss class-keyed checks

**Summary.** The class-value path in `infer_new` keys on a **direct identifier callee**, so a
parenthesized callee (`new (Priv)()`) or a const alias (`const A = Priv; new A()`) bypasses
`TK2511` (abstract) and `TK2673`/`TK2674` (constructor accessibility). Arity via those forms is
still caught by the construct-signature path.

## Problem

`ClassInfo` facts (`is_abstract`, `ctor_visibility`) are looked up from the callee identifier's
value `DeclId`; any other callee expression falls to the object-construct-signature path, which
carries neither flag. Consistent pre-existing boundary (the abstract check has always had it),
documented in `docs/reference/divergences.md`; filed so it does not fossilize.

## Approach / acceptance

Either resolve through trivial parens/aliases to the class `DeclId` (cheap, syntactic), or carry
abstractness/ctor-visibility on the class's static-side type so the construct-signature path can
check them (structural, broader). Acceptance: `new (Priv)()` and aliased `new` match tsc's
`TS2511`/`TS2673`/`TS2674`; direct-path behavior unchanged.

## Touch points

`src/check/checker/calls.rs::infer_new`; possibly the static-side `ObjectType` representation.

<!-- Origin: b20 adversarial review (sprint 2026-07-04-class-completeness). -->
