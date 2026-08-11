---
id: 48
title: noImplicitAny family (TK7005/7006/7008/7017/7031/7053)
---

# 48 — `noImplicitAny` family

**Summary.** Unannotated, uninferrable positions lower silently instead of reporting the
strict implicit-any diagnostics: TK7006 (parameter), TK7005 (variable), TK7008 (member),
TK7017 (`globalThis` property), TK7031 (binding element), and TK7053 (element access).
Flat object variable patterns over a ready modeled source now publish distinct typed leaves,
including static keys and defaults. This shipped declaration path does not implement TK7031 or the
remaining binding-pattern shapes.
Tier A — "part of the strict type
model" (scope.md); typokat is always-strict, so these are in scope without a flag surface.

## Problem

Audit first what an unannotated parameter lowers to today: if it lowers to a permissive
type, this family is also a FN source (calls with wrong arguments pass), not just a
missing lint. Contextual typing (M30) supplies types in many positions — the diagnostic
fires only where no contextual or inferred type exists.

## Approach / acceptance

Corpus first: unannotated params (free functions, methods; callbacks WITH a contextual
type must stay clean), implicit-any variables (`let x;` used before any assignment fixes
a type), and implicit-any binding elements. Keep the shipped flat object-variable publication
controls clean while covering array, nested, rest, computed, parameter, catch, and `for-in` /
`for-of` binding shapes plus unready-source TK7031 cases. Also cover element access with an
uncheckable key (the TK7053 vs TK2536 split per tsc) and an absent property on `typeof globalThis`.
The existing
cross-project isolation witness must keep rejecting leaked properties while changing its ordinary
`TK2339` to tsc-compatible `TK7017`. Cross-check tsc 6.0.3 --strict.

Until `TK7008` ships, a class property with neither an annotation nor an initializer records
`incomplete[class/property-definition/implicit-any]` and poisons the class surface. It must not be
omitted and leave an empty, permissive relation target.

## Touch points

`crates/typokat-check/src/check/checker/decls/` (param/var lowering),
`crates/typokat-check/src/check/checker/statements.rs` (remaining binding-pattern boundaries),
`crates/typokat-check/src/check/checker/expr.rs` (element
access), `crates/typokat-diagnostics/src/diagnostics/mod.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); scope.md Tier A. -->
