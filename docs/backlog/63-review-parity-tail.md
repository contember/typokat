---
id: 63
title: 2026-07-07 review parity tail (small FPs, wrong messages, depth guard)
---

# 63 — Review parity tail (2026-07-07)

**Summary.** The small, safe-direction or cosmetic findings of the cross-cutting
soundness review, batched: none drops errors, each is a real divergence worth killing
when its area is next touched. Repro details in the review transcripts; each was
probe-verified vs tsc 6.0.3.

## The list

- **Evaluator FPs:** (a) naked check parameter in the true branch —
  `type Up<T> = T extends string ? Uppercase<T> : never` raises spurious TK2344 (tsc
  narrows via substitution types; distinct from backlog `37`'s deferred *arguments*);
  (b) bare key binder as mapped value — `{ [K in "a" | "b"]: K }` raises TK2304 on `K`
  and degrades the alias to error (dropped member errors, loud); (c) `null`/`undefined`
  template holes stay symbolic instead of collapsing to `"null"`/`"undefined"`.
- **Relation FPs:** (d) protected member redeclared in a derived class breaks
  assignability to the base (tsc accepts legal protected redeclaration;
  `nominal_origin_ok` requires identical `declaring_class`); (e) `${number}` hole
  assignability demands canonical form — tsc's round-trip rule applies only to
  `infer N extends number` extraction, plain assignability is parse-only (`"01"`,
  `"1e3"` should be accepted; fix the wrong doc-comment too).
- **Checker FPs:** (f) assignment narrowing maps a literal RHS to its base primitive
  without intersecting the declared type — `x = "a"` on `x: "a" | "b"` then
  `const y: "a" | "b" = x` errors (tsc narrows to `"a"`); (g) out-of-subset call
  *argument expressions* (e.g. `use(x = "s")`) are dropped from `arg_types`, so arity
  reports "Expected 1, got 0" on a 1-arg call; (h) aliased guards
  (`const ok = typeof x === "string"; if (ok)`) narrow nothing — safe, but undocumented;
  (l, added 2026-07-10) assignment into an explicit `any` narrows it (`let s: any;
  s = 5; s.foo` → TK2339; tsc keeps `s` as `any` — narrowing belongs only to implicit
  evolving `any`; documented in divergences).
- **Docs/messages:** (i) TK2741 renders "missing in type ⟨TARGET⟩" — semantically
  inverted vs tsc's "missing in ⟨SRC⟩ but required in ⟨TGT⟩". *(Item (j), the
  multi-candidate inference union rule, graduated to backlog `65` — the b57 review
  confirmed it is a dropped-error family, not a doc-ledger gap.)*
- **Robustness (k) — PARTIALLY SHIPPED in sprint-2026-07-10 WU3.** The lowering/interning
  half landed: a shared annotation-depth budget (`MAX_ANNOTATION_DEPTH = 200`, reported as
  `TK2589`) paired with a 256 MiB scoped worker thread in the driver (rustc-style; exit
  codes, panic propagation, and semantics verified unchanged by WU4-C). **Residual (63k,
  still open):** input nested arbitrarily deeper than the budget still overflows **oxc's
  recursive-descent parser** before lowering runs, so the guard cannot see it — this needs
  an oxc-side parser nesting limit (or a pre-lowering token-depth reject). tsc 6.0.3 itself
  dies at ~3k. Pinned by the enabled `sr_wu3_types_recursion/deep_annotation.ts` fixture.

## Acceptance

Each entry: fixture pinning tsc behavior, fix, or an explicit divergence-ledger entry in
`docs/reference/divergences.md`. Close the item when the list is empty.

## Touch points

`src/check/checker/eval.rs` / `annotations.rs` (a-c), `src/relate/relation.rs` (d-e),
`src/check/checker/flowgraph.rs` / `calls.rs` (f-h), `src/diagnostics.rs` (i),
lowering depth guard (k).

<!-- Origin: cross-cutting soundness review 2026-07-07, low-severity batch. -->
