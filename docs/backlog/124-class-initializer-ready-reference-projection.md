---
id: 124
title: Project ready collection references inside class initializers
blocked-by: []
---

# 124 — Project ready collection references inside class initializers

**Summary.** A const-asserted class field that references an element of an already complete
tuple/array field (`({ value: this.source[0] }) as const`) records
`class/property-definition/initializer-inference` and loses the downstream `TK2540`/`TK2322` pair.
Effort S–M. This is a bounded ready-input producer bug, not the contextual class preparation gap of
[123](123-contextual-class-initializer-publication.md).

## Problem

`checker/classes/initializer.rs::infer_const_member_reference` handles only raw objects and object
indexing. It neither unwraps a readonly collection nor selects tuple/array elements, and
`checker/statements.rs::query_free_annotation_type` does not admit the fixtures' tuple/array/readonly
annotations, so lexical declarations have no ready value at class seeding time.

Witnesses (strict `tsc 6.0.3` oracles are pinned in the fixtures; `main` and the shipped `as const`
candidate both fail these rows the same way):

- `tests/cases/b44_as_const/class_ready_collection_references.ts` — incompletes at 12/21/30, missing
  `TK2540`/`TK2322` at 16/17, 25/26, 34/35.
- `tests/cases/b44_as_const/class_published_reference_surfaces.ts:11/18` and
  `class_reference_publication_controls.ts:9/10/22/23/25/26`.

The local-class member rows in the same fixtures (`class_published_reference_surfaces.ts:10/17`,
`class_reference_publication_controls.ts:8/21/24/27/34/35`, `class_member_selector_*.ts`) are a
different capability ([ADR-0042](../decisions/0042-qualify-deferred-class-member-results.md)) and stay
with [123](123-contextual-class-initializer-publication.md).

## Approach / acceptance

Select from an already complete raw fixed tuple/array, unwrap readonly only for lookup, and return
the selected child `TypeId` unchanged; the surrounding owned literal adds readonly to its own
property while the referenced child stays mutable. Extend only the existing query-free annotation
reservation for fixed dense tuples, arrays, readonly wrappers and already-ready children. No extra
declaration walk, lazy value inference, new projection engine or publication schedule.

Acceptance: the three fixtures above are fully green on their collection rows; direct tests assert
child-ID identity, readonly-wrapper lookup, valid nested writes, invalid nested/outer writes, and
that missing/out-of-range/unsupported selections stay non-clean. Existing query-free construction and
ADR-0037 receipt/event boundaries are unchanged.

## Touch points

`crates/typokat-check/src/check/checker/classes/initializer.rs`,
`crates/typokat-check/src/check/checker/statements.rs` (annotation reservation),
`crates/typokat-check/src/check/checker/classes/publication.rs` (seeding).

<!-- Origin: as-const sprint M3 preparation group 3 (2026-09-09); fixtures stay disabled rows of the mixed b44_as_const corpus. -->
