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

### WU1 — b30 negative literals (DONE)

**Files.** `src/check/checker/annotations.rs` (`lower_literal_type`, imports),
`src/check/checker/expr.rs` (`infer_unary`), `tests/conformance.rs` (flip `true`).

**Decisions / findings.**
- Two lowering sites, not one. The sprint scoped WU1 as "TYPE position only", but the
  committed corpus line 12 (`const d: NU = -3`) requires the **value** side too: the
  expression `-3` previously fell through `infer_unary`'s `_ => wk.error` arm, so it was
  the error type and assignable to anything (silent pass). Fixed both: type position
  (`lower_literal_type`, oxc `TSLiteral::UnaryExpression` + `UnaryNegation`) and value
  position (`infer_unary`, `Expression::NumericLiteral` under `UnaryNegation`), each
  producing a fresh `LiteralValue::Number(-v)`. This mirrors tsc, where prefix-minus on a
  numeric literal yields a fresh negative literal type in both positions.
- `-0` normalization: both sites collapse `-0.0` → `0.0` (`if negated == 0.0 { 0.0 }`),
  matching tsc's SameValueZero literal interning (`-0` type ≡ `0` type). Without it the
  interner's `to_bits()` hashing (`hash.rs`) would key `-0.0` distinctly. Not corpus-pinned;
  done because cheap and tsc-exact (WU4 probes it).
- Scope kept tight: only `UnaryNegation` of a `NumericLiteral`. `+1`, `-1n` (bigint),
  `~1`, `- -1`, and other unary expressions still return `None` (type position) / the error
  type (value position) — out of subset. Direction corrected per the WU4 review: in **value**
  position the error type is permissive, so this is a **silent under-report** (e.g. `const
  d: 1 = - -1` passes unchecked), not an over-report; in type position the aborted annotation
  also under-checks. Accepted as the documented out-of-subset gap (no behavior change; the
  corpus pins none of these shapes). The ≥1e21 `number_to_string` dtoa gap stays documented
  in backlog 30.

**Divergence noted (pre-existing, NOT introduced here).** Assignment against a *union*
target renders the source as its widened base: `const d: -1 | -2 = -3` reports
`Type 'number' is not assignable to type '-1 | -2'` (tsc: `Type '-3'`). This predates WU1 —
`const d: 1 | 2 = 5` shows `number` too — which is exactly why the corpus omits a substring
on that line. Single-literal targets render the literal correctly (`Type '-3'`).

**Gates.** `cargo test` 194 unit + conformance green; `clippy -D warnings` clean. M27 review
probes `p27`/`p30`/`p31` (scratchpad `review_m27/`) all match tsc.

### WU2 — b28 interface extends (DONE — blocker resolved via read-based spec, eff3eb3)

**Files.** `src/check/checker/context.rs` (`TypeDecl::Interface.extends`, `Pass.interface_fill`),
`src/check/checker/decls.rs` (`reserve_type_decls` captures `&iface.extends`;
`ensure_interface_filled` + `interface_heritage_index` + `compose_interface_heritage` +
`resolve_heritage_type`; free fn `merge_object_members`), `src/check/checker/mod.rs`
(`interface_fill` init).

**Mechanism (correct, proven, regression-free).** Interface fill is now on-demand
(`ensure_interface_filled`, mirroring `ensure_class_filled`): fills interface bases first
(any declaration order), `Filling` guard breaks an `extends` cycle (TS2310 out of scope — no
diagnostic, terminates), then folds each base's members into the reserved object
(`merge_object_members`: bases left-to-right, own overrides by name; index + subset
call/construct signatures inherited when own is absent). Generic bases instantiate via the
existing `instantiate_type_reference`. `keyof` / mapped sources / member access / assignability /
TK2353 excess then read the composed object for free. Non-extending interfaces are byte-identical
(empty `extends` → own only). Proven: `declare const ov: Over; const bad: 6 = ov.x` → TK2322
`Type '5'` (own `x:5` overrides base `x:number`); official-suite progress on
`callSignaturesThatDifferOnlyByReturnType2.ts` (multi-base generic call sig now composed).

**Blocker (RESOLVED).** The original corpus override sub-test asserted object-literal
assignment against a literal-typed member (`const o1: Over = { x: 5 }`), which requires
**contextual object-literal typing** — a pre-existing, extends-independent gap
(`infer_object_literal` in `expr.rs` unconditionally widens members: `{ x: 5 }` →
`{ x: number }`; proven with `type Q = { x: 5 }; const q: Q = { x: 5 }` erroring identically).
Over-reporting (safe direction), but the tsc-exact corpus expected the line clean. Leader
decision (eff3eb3): the fixture's override block is now **read-based** (`declare const ov:
Over; const o1: 5 = ov.x; const o2: 6 = ov.x; // error[TK2322]`) — which the mechanism passes —
and the contextual-typing gap is filed as **backlog 31** (a loud false-positive family worth
its own item). b28 flipped `true`.

**Out of scope confirmed.** TS2430 (incompatible extension) NOT added — current behavior: an
incompatible override silently takes the own member's type (e.g. `interface Over extends Base
{ x: 5 }` composes `x: 5`), no diagnostic. Declaration merging untouched. Interface-extends-class
and generic-recursive-interface bases: mechanism resolves the base's object if filled/resolvable,
else contributes nothing (no crash) — not corpus-exercised.

**Gates.** b28 registered `true` at eff3eb3 (amended fixture): `cargo test` 194 unit +
conformance green (all b28 lines pass); clippy clean.

### WU3 — b29 alias cycles (DONE)

**Files.** `src/check/checker/context.rs` (`TypeDecl::Alias.object_template`;
`Pass.resolving_alias_stack` / `circular_aliases` / `alias_indirection_depth`),
`src/check/checker/decls.rs` (seed object-literal aliases in `reserve_type_decls`;
object-template fill step; `resolve_type_decl` surface-cycle detection + `report_surface_cycle`),
`src/check/checker/annotations.rs` (`with_indirection` + instrumentation of array / tuple /
object-member / function-param+return / construct-signature lowering), `mod.rs` (field init).

**Two mechanisms.**
1. *Legal member recursion resolves correctly (reads).* A **non-generic** alias whose top body
   is an object type literal is seeded with a reserved object id (`object_template`, mirroring
   the M25 `conditional_template`) and filled in a dedicated step via `lower_interface_members` —
   the m5 named-recursive representation. A member self-reference then resolves to the stable
   reserved id, never re-entering: `type X = { a: X | null }` types `x.a` as `X | null`
   (`const s: string = x.a` → TK2322; `x.q` → TK2339), mutual `Even`/`Odd` resolve through their
   reserved ids. `type Pair = { a; b }` (m5) becomes a reserved nominal object — still structural
   under the relation, m5 unregressed.
2. *Surface cycles report TK2456.* A re-entry into a still-resolving alias in `resolve_type_decl`
   (only reachable for non-seeded aliases — unions / direct / mutual) reports TK2456 for the
   **whole cycle** (via `resolving_alias_stack` slice from the re-entered alias to the top, so
   mutual `Mut1`/`Mut2` and deep `C1`/`C2`/`C3` chains each name every member once) and
   error-types them.

**Surface-vs-indirection discipline (`alias_indirection_depth`).** `with_indirection` brackets
the constructor boundaries where recursion is *legal* (array/tuple element, object member,
function/constructor param+return). A re-entry at the alias's start depth is a surface cycle
(TK2456); at a greater depth it came through a constructor → legal recursion, silently
error-typed, **no** TK2456. This kills false positives on `type Arr = Arr[]`, `type Fn = () =>
Fn`, `type W = { a: W } | null`, `type Tup = [number, Tup | null]` (all tsc-legal). Conservative
by design: a missed increment over-reports (safe), never under-reports.

**Invariant discipline (no provisional caching).** The only durable writes to `type_resolved`
are (a) the seeded reserved object id — the alias's *final* identity (like an interface), and
(b) error-typing a **confirmed** surface-cycle member (`circular_aliases`) — a settled verdict
once the cycle is detected, not a value contingent on an in-flight assumption. No provisional /
mid-cycle resolution is ever cached (the relation-cache / CFG-fixpoint trap shape is avoided:
the re-entry returns without writing, and the settled error is written only after detection).

**Orthogonality to M25/M26.** Conditional aliases (seeded `conditional_template`) and mapped
self-refs are caught by their own pre-lowering `check_surface_references` before any re-entry
reaches `resolve_type_decl`, so no double-report; m25/m26 corpora unregressed.

**Known gap (documented, out of corpus scope).** Reads through a *non-seeded* indirection
(`type Arr = Arr[]`, `type W = { a: W } | null`) still error-type (permissive; an FN on reads,
not a false positive) — only object-literal-top aliases get correct reads. Generic recursive
aliases (`type Tree<T> = { value: T; children: Tree<T>[] }`) are unhandled (not seeded — generic
aliases stay structural templates); no corpus coverage.

**Gates.** `cargo test` 194 + conformance (incl. m5 / m25 / m26 / b29) green; clippy clean.
Surface probes (Y/Z/Mut1/Mut2, C1/C2/C3) TK2456 per alias; member probes (X, Even/Odd, mixed
Node1) reads correct; edge probes (array/tuple/function/union-wrapped) no false TK2456.

### Official-suite audit (release binary, `run --check`)

**0 regressions, 1 progress, exit 0.** Full state-change list (nothing suppressed):
- **PROGRESS** `conformance/types/objectTypeLiteral/callSignatures/callSignaturesThatDifferOnlyByReturnType2.ts`
  matched 0→1, fp 2→0. Cause: **WU2** — `interface A extends I<number>, I<string>` now composes
  the inherited generic call signature, so `x.foo('')` correctly errors (TK2345) — the expected
  "b28 kills FPs on interface-heavy files".
- **WU3 (b29) / WU1 (b30): no scoreboard movement.** No tracked in-scope file exercises negative
  literal types or surface alias cycles in a state-changing way; `--check` (which flags any new FP
  as a regression) reports none. No new TK2456 / TK2322 false positives on lib-shaped files.

Re-run after the b28 flip at eff3eb3 (release rebuilt): result identical — 0 regressions, the
same 1 progress, exit 0. Expected: the corpus flag is harness-only; the binary already carried
the mechanism.

Not run: `--save` (no baseline change committed), no commit — per instructions.

### WU4 review fixes — F1 (b29 boundary set) + F2 (b28 base kinds) at 92d36cf

The adversarial review returned FAIL with two HIGH findings; both fixed against the amended
specs (92d36cf: b29 Json + MapRec block, b28 ObjAlias + CBase blocks).

**F1 — b29 false positive: incomplete `with_indirection` boundary set.**
`type Json = string | number | boolean | null | Json[] | { [k: string]: Json }` tripped a
spurious TK2456: the **index-signature value** (`lower_index_signature`) and the **mapped
value template** (`lower_mapped_type`) lowered at surface depth. Both now lower one
indirection deeper (`annotations.rs`; keys stay at surface depth — recursion through an
index/mapped KEY is never legal, and a self-referencing key set remains the TK2456 case).
The Json alias then resolves to a REAL union (inner self-references error-type through the
legal-recursion path, but the union members — `error[]`, `{ [k: string]: error }` — are
containers, not the bare error type, so no union-absorption): `const j2: Json = undefined`
→ TK2322 as the spec pins, and `const j1: Json = { a: 1 }` passes.

**F2 — b28 silent FN for alias and class bases (fill-ordering).**
- (a) *Object-literal alias base* (`interface IA extends ObjAlias`): interface fill ran before
  the B29 object-template fill step, so heritage composed the still-empty seeded object. Fixed
  by making the object-alias fill an idempotent on-demand `ensure_object_alias_filled` (shared
  `template_fill` state array — `interface_fill` renamed, covering both template kinds; each
  index only advanced by the ensure fn matching its decl kind) and having heritage resolution
  **force-fill its base first**: `ensure_heritage_base_filled` dispatches by decl kind
  (interface → recursive interface fill; class → `ensure_class_filled`; bare alias →
  `resolve_type_decl` (memoized) then `ensure_reserved_template_filled`, a reverse lookup from
  the resolved `TypeId` to whichever declaration reserved it — which also composes transparent
  alias chains, `type IfaceAlias = RealBase`, in any declaration order). A generic alias
  heritage needs no pre-fill (instantiation resolves its template on demand).
- (b) *Class base* (`interface IC extends CBase`): the class's reserved instance id was seeded
  but classes fill LAST — heritage read an empty instance. The class arm above force-fills via
  the existing idempotent, cycle-guarded `ensure_class_filled`, so the interface composes the
  real instance type (`{ cx: number }` inheritance as the spec pins). Public class members
  compose structurally (declaring-class metadata is carried; only private/protected members
  make a type nominal, per invariants).
- *Cycle safety*: heritage-triggered fills only follow reserved **ids** (members never inline),
  and every ensure fn is `Filling`-guarded. The degenerate `type A = I; interface I extends A {}`
  terminates (re-entry hits the guard, base contributes its members-so-far = empty) with **no
  diagnostic** — tsc reports TS2310 "recursively references itself as a base type"; consistent
  with the existing silent interface-extends-interface cycle handling (TS2310 out of scope).
- *Ordering caveat (noted, pre-existing class of hazard)*: a heritage-forced EARLY fill (class
  or alias) that contains an eager `keyof SomeIface` over a not-yet-filled interface would
  compute over the empty reserved object — same hazard as the pre-existing interface-member
  `keyof` forward-reference; narrow (only heritage-referenced decls), not corpus-exercised.

**Secondary:** WU1 run-log wording corrected — the `- -1`/unary fallback is a silent
**under-report** in value position (error type is permissive), not "over-report — sound".
No behavior change (out of subset, not corpus-pinned).

**Reviewer probes (scratchpad `review_wu/`), all re-run:**
- Must-match set — all match tsc: `b29_idxsig` (RecU/RecO/Jsonish/WU: zero diagnostics),
  `b29_boundaries` (MapRec/NestArr/NumIdx/PropOnly: zero diagnostics), `json_canonical`
  (TK2322 fires), `b28_hard` (alias base + inherited index sig + optional/readonly flags +
  widening override: exactly the 4 expected errors), `b28_aliasbase` (all 3 cases incl.
  alias-to-interface), `b28_class` (class base + deep keyof + mapped-over-deep), `b28_genalias`
  (generic + non-generic alias bases).
- Rest: `b28_diamond` (diamond/forward-ref/generic-passthrough) matches tsc. `b29_cycles`
  matches except `type Int = Int & { x: number }` — NO TK2456 (tsc: TS2456): intersections
  have no lowering arm at all (whole annotation degrades to error before any re-entry) —
  out-of-subset FN, noted. `b29_legal` matches incl. `type L<T> = L<T[]>` → TK2456 (tsc
  agrees) and generic-recursive `GTree` non-recursive-member reads checking correctly.
  `b29_gaps` shows exactly the documented non-seeded-indirection read gap (recursive-member
  reads error-type silently; container-level checks still fire). `b30_edges`/`narrow_neg`:
  the equality-narrowing lines fail identically for the POSITIVE analog (`1 | 2 | 3`) — a
  pre-existing literal-union narrowing limitation, nothing b30-specific. `cross_wu` (all three
  WU interplays) and `regress` (no TK2456 double-reports) match.

**Gates:** `cargo test` 194 unit + conformance green (both amended fixtures); clippy clean;
release rebuild + `run --check`: **0 regressions, 1 progress (the same
`callSignaturesThatDifferOnlyByReturnType2.ts`), exit 0** — aggregates byte-identical to the
pre-fix run (strict 20/64 files, 13/200 diagnostics; non-strict 170/436, 240/1459), i.e. the
class/alias-base composition and the two new indirection boundaries moved no other tracked
file. No `--save`, no commit.
