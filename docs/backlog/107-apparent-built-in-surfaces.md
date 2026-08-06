---
id: 107
title: Apparent Function and Object surfaces
blocked-by: []
---

# 107 — Apparent `Function` and `Object` surfaces

**Summary.** Callable and ordinary object types do not consistently receive the built-in
`Function` or `Object` members that TypeScript uses as their apparent structural surface.

## Problem

The official TypeScript 6.0.3 suite has two independent safe-direction gaps:

- `objectTypeWithCallSignatureAppearsToBeFunctionType.ts` and
  `objectTypeWithConstructSignatureAppearsToBeFunctionType.ts` accept `.apply` on callable and
  constructable object types. typokat reports `TK2339` because it checks only explicitly declared
  object members instead of the apparent `Function` surface.
- `assignFromNumberInterface2.ts` accepts `a = b`, where the ordinary `NotNumber` source receives
  `Object.prototype` members such as `toLocaleString` before it is related to `Number`. typokat's
  stripped scoreboard line 19 reports the sole surplus `TK2741` because that apparent `Object`
  surface is absent. Primitive-to-boxed overlap itself is already represented and is not this gap.

## Approach / acceptance

Model built-in apparent surfaces in one query path without mutating stored object identity. Preserve
explicit members that hide built-ins and keep explicit incompatible overlaps rejected.

Acceptance is exact: the two callable-object witnesses lose only their `.apply` `TK2339` records;
the `assignFromNumberInterface2.ts` line-19 `TK2741` disappears; and
`b14_full_lib_loading/object_explicit_overlap.ts` remains a negative control. Cross-check all three
against TypeScript 6.0.3 with the pinned default library.

## Touch points

Structural apparent-type demand, member lookup, object relation, focused conformance fixtures, and
the official-suite ratchet.

<!-- Origin: backlog-14 closure divergence-owner audit (2026-08-06). -->
