---
id: 49
title: Possibly-undefined/null diagnostics + optional methods/accessors
---

# 49 — Possibly-`undefined`/`null` family + the optional-members tail

**Summary.** The dedicated strict-null access diagnostics are missing — TK2531/TK2532/
TK2533 (object is possibly null/undefined), TK2722 (cannot invoke a possibly-undefined
value), the TK18047/TK18048 strays — and the M21 deferrals still stand: optional
**methods**/accessors (`go?(): T`) and narrowing an optional through a member-access
guard (currently over-reports `T | undefined`).

## Problem

M21 made optional-property reads yield `T | undefined`, so misuse surfaces as a generic
TK2322/TK2345 at best — but tsc's dedicated codes fire at positions typokat doesn't check
at all: with `declare const o: { a: number } | undefined`, `o.a` must be TK2532. Optional
methods don't exist in the model (verify their current lowering at spec time). The
non-null assertion operator (`x!`) must be honored when these checks land, or every `!`
becomes a false positive.

## Approach / acceptance

Corpus first: nullable receivers for member access / calls / element access, optional
chaining (`?.`) as the sanctioned form, optional-method declarations + calls (TK2722),
`x!` suppression. The member-access-guard half overlaps backlog `51` (member-path
narrowing) — sequence `51` first or scope the guard work there. Cross-check tsc 6.0.3
--strict.

## Touch points

`src/check/checker/expr.rs` (receiver nullability), `src/types/repr.rs` (optional
methods/accessors), `src/check/flow.rs` (guard overlap with `51`), `src/diagnostics.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); M21 deferral list (README known limitations). -->
