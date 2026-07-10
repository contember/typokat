---
id: 62
title: Index-signature relation parity (implicit-index rule, numeric names, optional-undefined)
---

# 62 — Index-signature relation parity

**Summary.** Three probe-verified gaps in the index-signature obligations of
`relate_objects` (2026-07-07 review):

1. **FN (MED):** interface/class sources are accepted against index-signature targets —
   `const a: { [x: string]: number } = someInterfaceValue` is silent; tsc reports
   TS2322 "Index signature for type 'string' is missing in type 'I'". tsc grants
   *implicit* index signatures only to anonymous object types; typokat has no
   "source must provide an index signature" rule and the relation never consults a
   nominal/declared bit (`relation.rs:672-741`). Hits the common
   `Record<string, T> = interfaceValue` shape.
2. **FN (LOW):** `is_numeric_property_name` (`relation.rs:1337-1339`) uses
   `parse::<f64>().is_finite()`; tsc's rule is `(+name).toString() === name`, so
   `"NaN"`/`"Infinity"` props escape a number index signature (silent accept), while
   `"01"`/`"1e21"` are wrongly treated as numeric-keyed (safe over-report).
3. **FP (MED-LOW, safe):** an optional source property relates its
   optionality-`undefined` against the target index value type —
   `const a: { [k: string]: number } = optObj` with `{ a?: number }` errors; tsc
   excludes optionality-derived `undefined` there (but still rejects explicit
   `number | undefined` — keep that). The snapshot at `relation.rs:685-689` drops the
   `optional` flag.

## Approach / acceptance

**Acceptance spec (ready, gap 1 only):** the disabled
[`tests/cases/sr_deferred_ledger/b62_index_signature.ts`](../../tests/cases/sr_deferred_ledger/b62_index_signature.ts)
fixture pins the implicit-index FN (nominal interface source rejected against an
index-signature target; anonymous source accepted). Gaps 2 (numeric-name classification) and
3 (optional-`undefined`) still need their own fixtures when this is scheduled.

Add the "source provides an index signature" requirement keyed on whether the source is
a declared (interface/class) vs anonymous object type; align numeric-name classification
with tsc's `isNumericLiteralName`; carry the `optional` flag into the index obligations
and exclude optionality-`undefined`. Corpus first (all three probe families, both
directions); cross-check tsc 6.0.3 --strict; m19 corpus extension.

## Touch points

`src/relate/relation.rs` (`relate_objects` index obligations,
`is_numeric_property_name`), possibly a declared-vs-anonymous bit on object types
(`src/types/`), m19 corpus.

<!-- Origin: cross-cutting soundness review 2026-07-07 (relate reviewer #1-#3), leader-verified. -->
