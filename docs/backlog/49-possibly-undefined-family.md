---
id: 49
title: Possibly-undefined/null diagnostics + optional methods/accessors
---

# 49 — Possibly-`undefined`/`null` family + the optional-members tail

**Summary.** The dedicated strict-null access diagnostics are missing — TK2531/TK2532/
TK2533 (object is possibly null/undefined), TK2721/TK2722 (cannot invoke a possibly-null/undefined
value), the TK18047/TK18048 strays — and the M21 deferrals still stand: optional
**methods**/accessors (`go?(): T`) and narrowing an optional through a member-access
guard (currently over-reports `T | undefined`). Optional-chain and non-null assertion expression
semantics are part of the same strict-null boundary.

## Problem

M21 made optional-property reads yield `T | undefined`, so misuse surfaces as a generic
TK2322/TK2345 at best — but tsc's dedicated codes fire at positions typokat doesn't check
at all: with `declare const o: { a: number } | undefined`, `o.a` must be TK2532. Optional
methods don't exist in the model (verify their current lowering at spec time). Optional chains
must preserve their nullish boundary, and the non-null assertion operator (`x!`) must be honored
when these checks land, or every `!` becomes a false positive.

The strict-tsc 6.0.3 controls in `classPropertyAsPrivate.ts`,
`classPropertyAsProtected.ts`, and `classPropertyIsPublicByDefault.ts` add an exact `TS2721`
witness: each static getter `b` returns `null`, so `C.b()` is a possibly-null call once backlog
`76` publishes the getter's inferred return instead of `void`. The current `TK2349` is therefore a
safe cascade owned by `76`; this item owns the final null-call diagnostic, not getter inference.
Official `partiallyNamedTuples2.ts` independently records
`expr-infer/non-null-assertion/self` at `null!`; named-tuple support exposed that existing wrapper
boundary without changing its ownership.

## Approach / acceptance

Corpus first: nullable receivers for member access / calls / element access, optional
chaining (`?.`) as the sanctioned form, optional-method declarations + calls (TK2722),
`x!` suppression. The member-access-guard half overlaps backlog `51` (member-path
narrowing) — sequence `51` first or scope the guard work there. Cross-check tsc 6.0.3
--strict.

Include the three official getter calls above in the strict corpus. After their inferred returns
are available, each must report exactly one `TK2721` at the call and no `TK2349`; the ordinary
optional-call controls must continue to use `TK2722`. The `partiallyNamedTuples2.ts` initializer
must publish the non-null `object` value without a wrapper incomplete once this item is implemented;
the object-keyword type itself is already shipped.

## Touch points

`crates/typokat-check/src/check/checker/expr.rs` (receiver nullability), `crates/typokat-types/src/types/repr.rs` (optional
methods/accessors), `crates/typokat-check/src/check/flow.rs` (guard overlap with `51`),
`crates/typokat-diagnostics/src/diagnostics/mod.rs`.

<!-- Origin: completion-roadmap review (2026-07-07); M21 deferral list (README known limitations). -->
