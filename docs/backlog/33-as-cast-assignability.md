---
id: 33
title: as-cast RHS bypasses initializer assignability (silent FN family)
---

# 33 — `as`-cast RHS bypasses initializer assignability

**Summary.** `const x: number = "hello" as string` and
`const y: { a: number } = {} as object` are accepted silently — the asserted type is
never related to the declared annotation (tsc: TS2322/TS2741). Pre-existing, surfaced
by the M28 review (it initially masqueraded as an intrinsic false negative).

## Problem

An `as`-cast expression's type should be the asserted type, and that type must still
satisfy the annotation/contextual target like any other RHS. Today the initializer
assignability check is skipped entirely when the RHS is a cast, so every declaration
with a cast RHS checks nothing — a broad silent-FN family in real-world code, where
casts are common.

The M28 round-4 review widened the finding: `as`-expressions in CALL ARGUMENT
position break arity/assignment checking too (`g(1 as never)` → spurious TK2554 with
no generics involved), so the whole `as`-expression typing path is unfinished, not
just the initializer site.

tsc also validates the cast itself (TS2352 when the source and asserted types are
unrelated); that sub-check can ship with this item or be split when scoped.

## Approach / acceptance

Type the cast expression as the asserted type and let the normal assignability path
run against the declaration target unchanged. Corpus first: primitive/object/generic
asserted types vs mismatched annotations, `as const` untouched, legal upcasts stay
clean; cross-check tsc 6.0.3 --strict. Acceptance: the two probes above report
TK2322/TK2741; official-suite `run --check` audited.

## Touch points

`src/check/` expression typing for `TSAsExpression`/`TSTypeAssertion` (locate the
skip), the declaration-initializer check path.

<!-- Origin: M28 review round 1 (2026-07-05), incidental pre-existing finding. -->
