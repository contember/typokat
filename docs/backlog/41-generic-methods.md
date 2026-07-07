---
id: 41
title: Generic methods (method-level type parameters)
---

# 41 — Generic methods

**Summary.** Method-level type parameters are a documented deferral: instance methods with
their own `<U>` are skipped as out-of-subset; statics raise a spurious TK2304 (backlog
`23` is the minimal suppression). Full generic methods — declaration, call-site inference,
constraints, assignability — are required for real-world code and for `lib.d.ts` (`14`):
`Array.prototype.map<U>` alone makes them unavoidable.

## Problem

Class-member lowering has no per-method type-param frame, and the inference engine only
instantiates function-declaration type params. Method signatures with own type params must
lower to generic signature types, infer at call sites like free generic functions, and
relate per tsc. The same gap covers generic call signatures on objects/interfaces (dropped
by F1 WU2) and generic constructors.

## Approach / acceptance

Extend signature lowering with a method-scoped type-param frame (reuse the M9/M10 + M24
machinery); call-site inference through member calls; constraints (`TK2344`) apply.
Corpus first; cross-check tsc 6.0.3 --strict. Subsumes backlog `23` — close it with this
item if the warm-up hasn't shipped by then.

## Touch points

`src/check/checker/classes.rs` / `decls.rs` / `annotations.rs` (lowering frames),
`src/check/infer.rs` (member-call inference), `src/relate/relation.rs` (generic-signature
relation).

<!-- Origin: completion-roadmap review (2026-07-07); deferral noted at M11 and in backlog 23. -->
