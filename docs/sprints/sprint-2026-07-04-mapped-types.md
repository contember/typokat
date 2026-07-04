<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — mapped types `{ [K in keyof T]: … }` / M26 (2026-07-04)

**Goal.** Ship backlog [`10`](../backlog/10-mapped-types.md): mapped types are lowered to an
interned node and **evaluated** over concrete sources (keyof-derived and literal-union key
sources) — iterating resolved keys, applying the value transformation, the modifier arithmetic
(`?`/`+?`/`-?`, `readonly`/`-readonly`), and homomorphic preservation — and stay **deferred**
on an unresolved source with the M25 conservative relation model.

**Theme.** Second milestone of the type-level evaluation phase. Today mapped types are silently
permissive (probe: every m26 fixture passes with exit 0 — a pure false-negative class). Builds
directly on M25's machinery: the evaluator's work-stack/memo/budget, the demand sites, and the
deferred-node relation rules all exist; M26 adds a node kind and its evaluation rule.

## Refs re-verified at HEAD (2026-07-04, 4abd6cb)

- ✔ **Mapped types are unchecked today** — probe: `const i2: Ident<P> = { a: 1, b: 2 }` → exit 0.
- ✔ **M25 evaluator machinery** — `src/check/checker/eval.rs`: work-stack, memo (provisional
  discipline), per-root budget, demand sites in `annotations.rs`/`decls.rs`/`calls.rs`; the lazy
  `InstantiationType` defers recursive alias bodies. Reuse all of it.
- ✔ **keyof + indexed access on CONCRETE objects** — M20 (`m20_keyof/`): `keyof T` resolves to a
  literal union; `T[K]` for literal K resolves. The mapped evaluation loop binds K per key.
- ✔ **Property metadata is identity-bearing** — optional (M21) + readonly (M14) are on
  `PropertyType` and fold into the structural hash; `substitute` carries all fields
  (invariants §1). Homomorphic preservation composes source flags with modifier arithmetic.
- ✔ **tsc 6.0.3 probes** (scratchpad `m26_p1`–`m26_p3`): homomorphic identity preserves `?` and
  `readonly` (TS2540 through `Ident<P>`); `-?` → TS2741 on omission; `-readonly` allows writes;
  literal-union sources build required members; mapped-of-mapped accumulates modifiers;
  `f<T>(t: T): Ident<T> { return t; }` is LEGAL in tsc (homomorphic-identity rule) — typokat
  stays conservative (documented divergence); `return 5` → TS2322 in both.

## Work units

### WU1 — Repr + lowering: the mapped node (effort M)

- **Scope.** Interned mapped-type node `{key_param (its own binder — de Bruijn-style within the
  node per ADR-0002), key_source, value_template, optional_modifier (+?/-?/none),
  readonly_modifier (+/-/none)}`; hash/eq/substitute (no capture of the node's own key binder);
  lowering from the oxc AST at every annotation site; display `{ [K in S]: V }` (code-only
  markers). References to `K` inside the value template (typically `T[K]`) lower against the
  node's binder.
- **Acceptance / witness.** Lowering round-trip unit tests; no behavior change (corpus stays
  registered `false` until WU2).
- **Touch points.** `src/types/repr.rs`, `hash.rs`, `intern.rs`, `substitute.rs`,
  `src/check/checker/annotations.rs`.

### WU2 — Evaluation + deferral (effort M/L)

- **Scope.** In `eval.rs`, a mapped node with a **concrete key source** evaluates: resolve the
  key source (`keyof T` for concrete T → literal-key union; a literal union directly; a single
  literal); for each key, bind the node's key binder and evaluate the value template (indexed
  access `T[K]` resolves per key through the existing M20 path); build the object type with
  modifier arithmetic — start from the source property's `?`/`readonly` when the key source is
  `keyof <concrete>` (homomorphic), else default absent, then apply `+`/`-` modifiers.
  Composes with the existing memo/budget/work-stack (a mapped body may demand conditional
  evaluation and vice versa). A mapped node whose key source contains a free declaration param
  stays **deferred**: identical-node assignable; nothing else in either direction (conservative;
  the tsc homomorphic-identity allowance is the documented over-report divergence). Demand
  sites: same as M25 (annotations, alias/generic-call instantiation).
- **Acceptance / witness.** All four `m26_mapped_types/` fixtures green (flip the conformance
  row to `true`); m20/m21/m14/m25 corpora unregressed; official-suite `run --check` audited.
- **Touch points.** `src/check/checker/eval.rs`, `annotations.rs`/`decls.rs`/`calls.rs` (demand
  wiring), `src/relate/relation.rs` (deferred-mapped rules — mirror the deferred-conditional
  dispatch).

### WU3 — Independent adversarial review + ratchet (effort M)

- **Scope.** Hunt false negatives, cross-check tsc: modifier arithmetic edge cases (`-?` on a
  non-optional source, double application via mapped-of-mapped), homomorphic preservation under
  substitution order, evaluation × conditional interplay (a conditional value template), memo
  soundness across instantiations, deferred-mapped conservatism (both directions), TK2540
  through composed maps, literal-union key duplicates, empty sources. Then ratchet — the
  official suite has a `mapped-type` gate to LIFT (like M25's conditional gate): audit every
  file entering scope.

## Out of scope (explicit)

- `as` key remapping + template-literal keys (backlog `11`).
- `K in string` / index-signature production from non-literal key sources.
- Generic/deferred `keyof T` relation semantics beyond the conservative deferral.
- tsc's homomorphic-identity assignability rule (documented over-report divergence).
- Utility-type aliases (`Partial`, `Readonly`, …) as built-ins — they fall out of mapped
  evaluation naturally when user-defined; the lib built-ins are backlog `12`/`14`.

## Decisions

- **Mapped node key binder follows ADR-0002**: a node-scoped binder like conditional infer
  binders — never a substitution target, never reaches the relation engine unbound.
- **Deferred-mapped relation = the M25 conservative model** (identical node or nothing).

## Run log

<!-- Append as you work. -->
