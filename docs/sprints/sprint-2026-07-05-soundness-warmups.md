<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — soundness warm-ups: backlog 28 + 29 + 30 (2026-07-05)

**Goal.** Kill the three silent-false-negative families the M26/M27 adversarial reviews
surfaced: [`28`](../backlog/28-interface-extends-composition.md) interface `extends`
composition, [`29`](../backlog/29-silent-alias-cycle-permissiveness.md) silent alias-cycle
permissiveness, [`30`](../backlog/30-numeric-literal-correctness.md) numeric-literal
correctness. Corpora committed (`978e743`, registered `false`): `b28_interface_extends/`,
`b29_alias_cycles/`, `b30_negative_literals/` — all tsc-exact, zero divergences.

**Theme.** The dev-method's between-milestones warm-up slot, spent where soundness > all:
each item is a family where VALID diagnoses are silently dropped today (and b28 also emits
false positives on valid code). All three de-risk M28's utility-type fixtures (which lean on
interfaces and numeric literals).

## Refs re-verified at HEAD (2026-07-05, 978e743)

- ✔ **b30** — `const b: -1 = 5` → exit 0; probe `p31` (review_m27): `-1` assignable to
  `{ a: 1 }` — the unary-minus type expression falls to the error type at lowering
  (`annotations.rs` literal lowering handles only plain literals). `number_to_string`
  (`repr.rs`) is decimal-only (the ≥1e21 gap is documented; fixing it is part of 30 but the
  corpus only pins simple negatives — do not attempt full JS dtoa here, record what's done).
- ✔ **b28** — `const bad: Derived = { y: "s" }` → exit 0 AND `const ok: Derived = { x: 1,
  y: "s" }` → spurious TK2353; `keyof Derived` = own keys only; `dv.x` → TK2339. Interface
  fill (`decls.rs`) ignores heritage clauses entirely. The class machinery walks heritage —
  mirror it (own member wins by name; multiple bases left-to-right per tsc).
- ✔ **b29** — `type Y = Y | null` → no diagnostic, downstream fully permissive; `x.a` on
  legal `type X = { a: X | null }` reads as the error type (silently — `const s: string =
  x.a` passes). M25/M26 added surface re-entry detection for conditional/mapped aliases
  (`resolving_alias` context) — plain aliases bypass it; the demand path re-enters and
  silently error-types. m5 handles the declaration-side representation of recursive types —
  the read path must resolve through it.

## Work units

### WU1 — b30 negative literals (effort S)

Lower `-<numeric literal>` in type position to a negative `LiteralValue::Number` (oxc:
TSLiteralType with a unary-minus expression). Everything downstream (relation, unions,
template holes via `number_to_string`, extends) already handles number literals — the fix is
the lowering arm. Witness: `b30_negative_literals/` green (flip `true`); the M27 review
probes `p27`/`p30`/`p31` match tsc.

### WU2 — b28 interface extends (effort M)

Fold heritage members into the interface's object type at fill time (`decls.rs`), mirroring
the class heritage walk: extends chain composed transitively, own member wins by name,
multiple bases merged left-to-right, deep chains and the `f1_object_interface_*` corpora
unregressed. `keyof`, mapped sources, member access, assignability, and TK2353 excess checks
then see the full key set for free (they all read the composed object type). Witness:
`b28_interface_extends/` green (flip `true`).

### WU3 — b29 alias cycles (effort M)

Generalize the M25/M26 `resolving_alias` surface-re-entry detection to ALL alias resolution:
a genuine surface cycle (direct, mutual, or through union members — the resolution of the
alias's own surface re-enters it) reports **TK2456 at the alias declaration** and
error-types (the M22 discipline now holds — a primary diagnostic exists). Legal recursion
through members (object indirection) must NOT trip it and must resolve reads through the m5
named-recursive representation instead of re-entering into the error type. Watch exactly the
trap invariants §1 names: never durably cache a provisional resolution. Witness:
`b29_alias_cycles/` green (flip `true`); m5 + m25/m26 corpora unregressed.

### WU4 — Independent adversarial review + ratchet (effort M)

Attack each fix's edges: b30 (negative zero, `-0` vs `0`, negative floats, unions with
negatives, template/extends interplay, `typeof x === "number"` narrowing on negative
literals); b28 (diamond inheritance, override widening — tsc TS2430 incompatible extension
is OUT of scope, note behavior; generic interfaces extending generic bases; interplay with
mapped/keyof/index signatures; class implements of derived interfaces); b29 (cycles through
conditional/mapped/template wrappers, self-reference in generic alias arguments, deep mutual
chains, no false TK2456 on m5-legal shapes — including through arrays/tuples/functions).
Cross-check tsc. Then ratchet — expect real scoreboard movement (b28 kills FPs on
interface-heavy files; b29/b30 may add matched diagnostics).

## Out of scope (explicit)

- TS2430 (incompatible interface extension) — a completeness item, not this FN family.
- Full JS `String(n)` dtoa for ≥1e21 (the b30 corpus pins simple negatives; the large-
  magnitude gap stays documented in backlog 30 if not closed here).
- Declaration merging (same-name interfaces) — untouched, whatever today's behavior is.
- TS2313 mapped-key circularity twin (documented M26 omission).

## Decisions

- Three WUs, one sprint, one implementation agent (sequenced WU1→WU2→WU3 — smallest first,
  each independently green before the next), one adversarial review over all three.

## Run log

<!-- Append as you work. -->
