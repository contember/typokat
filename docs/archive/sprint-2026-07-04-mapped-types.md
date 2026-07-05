# OUTCOME (closed 2026-07-05) — SHIPPED

**M26 shipped.** Mapped types are an interned node (template-mapper model: a
node-scoped `MappedValue` placeholder for `T[K]`; M20 untouched), evaluated over
concrete sources with homomorphic preservation and full modifier arithmetic
(`-?` strips `undefined`, exactly-`undefined` → `never`), **distributing over union
type arguments** (`Ident<A|B>` = `Ident<A> | Ident<B>`); no permissive fallback —
error sources error-type, non-iterable sources defer conservatively; self-
referential mapped aliases report `TK2456`.

**Commit map.** Spec `4abd6cb` · plan `4509dc3` · spec amendments `db0c8b8` (`-?`
undefined strip), `5b700ec` (union distribution + `-?`→`never` + no-`{}` fallback +
self-ref TK2456 + index-sig deferral) · backlog byproducts `2d5c3a1` (28),
`b037a7b` includes 29 · implementation `b037a7b` (+1540/−26) · gate lift + ratchet
`e24907c`.

**Verification.** 186 unit + conformance green (m26: 5 files / 23 markers), clippy
clean. Official suite: mapped-type gate lifted — 4 files enter scope (2 fully
clean-kept), scoreboard in-scope 491→495, clean-kept 166/211, diag-recall
250/1657, 0 regressions. Two fix rounds: leader verification (`-?` strip — where
the round-1 directive itself mis-encoded tsc's exactly-`undefined` behavior and
leader arbitration corrected the disputed probes), adversarial review FAIL (union
distribution — the permissive `{}` fallback; `never` edge) → fixes → re-review
**PASS** (the 13h re-verification run also independently confirmed).

**Deferred / byproducts.** Backlog `28` (interface `extends` composition —
pre-existing silent FN family) and `29` (silent alias-cycle permissiveness —
error-typing with no primary diagnostic; the review's sharpest cross-cutting
find). Index-signature sources defer (tsc resolves — documented over-report);
nested-mapped-in-composite templates and instance-reached class fields stay
conservative (backlog `27` analog); `as` remapping + template-literal keys →
`11`; numeric-literal keys defer.

---

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

### WU1 + WU2 implementation (leader-supervised subagent)

**Shipped.** `m26_mapped_types` flipped `true`; `cargo test` green (184 unit — incl. 4 new —
+ conformance); `cargo clippy --all-targets -D warnings` clean; official-suite `run --check`
exit 0, scoreboard unchanged.

**Representation decisions (WU1).**
- Two new node kinds: `TypeTag::Mapped` (side-table `MappedType`) and `TypeTag::MappedValue`
  (the node-scoped `T[K]` placeholder, payload-free singleton like `Infer`). Both folded into
  hash/eq/`intern`/`store`.
- `MappedType { homomorphic, key_source, value_template, optional_modifier, readonly_modifier }`.
  `ModifierOp { Keep, Add, Remove }` (new repr enum, identity-bearing). `homomorphic` records
  whether the `in` clause was `keyof <X>`; if so `key_source` is `<X>` (the object source,
  substituted at instantiation), else the constraint type directly (literal-union key set).
- **Deferral without a deferred-`keyof`/deferred-indexed-access node.** The key challenge was
  keeping `T[K]` symbolic through lowering (M20's `indexed_access_type` is eager → `error` on a
  non-concrete operand). Chosen design: represent `T[K]` in the value template as the
  `MappedValue` placeholder (recognized at lowering: an indexed access whose index names the
  active mapped key → placeholder), and for homomorphic maps preserve the source's `keyof <X>`
  operand as `key_source` rather than an eager `keyof`. This is the tsc "template mapper" model
  and covers every fixture, and crucially leaves M20's `keyof_type`/`indexed_access_type`
  **untouched** — non-mapped behavior is unchanged (hence the flat scoreboard).
- `substitute` maps over the mapped node (`key_source` + `value_template`) and leaves the
  `MappedValue` placeholder alone (no-capture, ADR-0002 analog). Unit test added.

**Evaluation decisions (WU2).**
- `evaluate_type` gate extended with `Mapped`. `eval_mapped` mirrors `eval_conditional`:
  deferred (free `key_source`) → returned unchanged, not memoized; concrete → in-flight +
  budget guarded, evaluates the key source (a tail step, so mapped-of-mapped is a loop — RORO
  witnessed), then `AssembleMapped` derives properties and `BuildMappedObject` interns the
  result object. All on the existing work-stack/memo/budget (no second engine).
- Modifier arithmetic: homomorphic starts each property's `?`/`readonly` from the SOURCE
  property (preservation), non-homomorphic starts absent; then `ModifierOp::apply`. Optional
  result props bake `| undefined` into their stored type to match the M21 convention the
  relation engine reads back.
- Deferred-mapped relation mirrors the deferred-conditional dispatch: identical node assignable
  (the `src == tgt` fast path), nothing else in either direction (`src is Mapped → No`;
  `tgt is Conditional|Instantiation|Mapped → No`). `deferred_generics.ts` ret1 pins the
  documented tsc homomorphic-identity over-report (`T` → `Ident<T>` → TK2322).

**Deviations / documented limitations (all safe/over-report, none tested by the corpus).**
- ~~**`-?` does not strip `undefined`**~~ — flagged here, leader confirmed it as a REAL dropped
  error (in scope, not a limitation), spec amended (db0c8b8, `modifiers.ts` q3/q4). Fixed — see
  the follow-up entry below.
- **Index-signature sources** (`{ [K in keyof T]: … }` where `T` has an index signature) map
  only the named properties; index-signature production (`K in string`) is explicitly out of
  scope. A source with only index signatures → empty result.
- **Cross-binder nested mapped** (an inner value template referencing an outer key) is left
  opaque (`replace_mapped_value` does not descend into a nested `Mapped`; the placeholder is a
  singleton), analogous to the poisoned nested-infer stopgap. No fixture.
- **`as` key remapping** and a **missing value template** abort the annotation (→ error type),
  out of subset.

**Audit — official-suite `run --check` (release binary).** `regressions: 0, progress: 0,
missing-from-corpus: 0`; exit 0. The scoreboard is unchanged because (a) the harness's
`mapped-type` gate (`tsofficial.py:151`, `[\s*\w+\s+in\s+`) buckets every mapped-type file as
out-of-scope-syntax, so no mapped file reaches typokat (gate NOT touched — leader lifts it at
ratchet), and (b) the mapped work is isolated to new node kinds + new match arms produced only
by mapped lowering, so no non-mapped code path changed behavior.

**Files changed (source):** `types/repr.rs`, `types/store.rs`, `types/hash.rs`, `types/intern.rs`,
`types/substitute.rs` (+no-capture test), `relate/relation.rs`, `check/flow.rs`, `diagnostics.rs`,
`check/checker/{context.rs, mod.rs, annotations.rs, eval.rs}` (+3 eval tests), `tests/conformance.rs`
(flag flip).

### Follow-up: `-?` strips `undefined` from the value type (leader-confirmed dropped error)

The WU2 flag above was verified by the leader as a two-sided in-scope bug (silent pass on
writing `undefined` into a `Req<P>` member + over-report on reading it) and the spec was
amended at db0c8b8 (`modifiers.ts` q3/q4).

**tsc 6.0.3 probes** (scratchpad `m26_p4_required_undef.ts`, `m26_p5_strip_site.ts` —
recording per the coordinator's ask):
- `Req<{b?: string}>` rejects `b: undefined` (TS2322) and reads `b` as exact `string` — the
  pinned q3/q4 pair.
- ~~**Exactly-`undefined` optional source**: `Req<{b?: undefined}>.b` is **`undefined`**~~ —
  **PROBE MISREAD, corrected in the round-2 entry below**: the `(15,22)` TS2322 in the probe
  output was line 15 (`u3 = { b: und }`), not line 14 (`u2: never = ru.b`, which was CLEAN).
  tsc's actual behavior (leader-arbitrated, `m26_arb.ts`): `-?` maps an exactly-`undefined`
  optional member to **`never`**.
- The strip applies to the **evaluated result**, not `T[K]` pre-substitution: with a template
  that re-adds it (`{ [K in keyof T]-?: T[K] | undefined }`), the optional member still reads
  as bare `string` (template-added `undefined` removed too).
- **Non-optional** source members never strip: `Req<{b: string | undefined}>.b` keeps
  `string | undefined` (assigning it to `string` is TS2322), and the re-adding template keeps
  `number | undefined` on the required member `a`.

**Fix** (`src/check/checker/eval.rs` only): `MappedProp` carries `strip_undefined`
(= `optional_modifier == Remove && source prop.optional`, homomorphic path only — a
non-homomorphic map has no optional source, flag always false); `build_mapped_object` filters
`undefined` out of the **evaluated** value's union members before the M21 `| undefined`
optional-baking step (which is off anyway — `-?` clears `optional`). `strip_undefined()` leaves
any non-union untouched, which is exactly the probed exactly-`undefined` behavior (a canonical
union is never all-`undefined`, so the result is never `never`). `+?`/`Keep` and non-optional
members unchanged. New unit test `required_strips_undefined_from_optional_source_values` pins
all three probe facts (result-level strip incl. template-re-added `undefined`; exactly-undefined
kept; non-optional not stripped).

**Gates re-run:** `cargo test` green (185 unit + conformance incl. amended q3/q4);
`clippy -D warnings` clean; release rebuilt; official-suite `run --check` exit 0 —
`regressions: 0, progress: 0, missing-from-corpus: 0` (mapped files still behind the harness
gate; no `--save`). No commit (leader verifies + commits).

### Round 2 — adversarial-review FAIL fixes (F1 distribution + no-permissive-`{}` + F2 `never` + self-ref TK2456)

Review returned two HIGH findings sharing one root cause — `assemble_mapped`'s
`object_type(key_source).unwrap_or_default()` produced a **permissive `{}`** for every
non-object evaluated key source (a union, an index-signature source, an error type) — plus an
arbitration correcting the round-1 `-?`/`undefined` probe. Spec amended at 5b700ec
(`union_distribution.ts` new; `modifiers.ts` q5/q6; `evaluation_sites.ts` self-ref + index-sig).

**F2 probe-record correction (own error, worth remembering):** the round-1 probe file's
output was **misread by one line** — `(15,22) TS2322 'undefined' is not assignable to 'never'`
belonged to `u3 = { b: und }` (line 15), not `u2: never = ru.b` (line 14, which was clean).
So tsc maps `-?` over an exactly-`undefined` optional member to **`never`** (both `u1:
undefined = r.b` and `u2: never = r.b` clean because `r.b` IS `never`; writing `{ b: und }`
errors 'not assignable to never'). Leader-arbitrated (`m26_arb.ts`), re-confirmed locally.
Fix: `strip_undefined()` maps a bare `undefined` value to `never` when the strip fires (union
filtering unchanged); unit test assertion corrected.

**F1 — homomorphic union distribution.** `apply_mapped` (substitute.rs) gets the M25-style
distribution guard: a homomorphic node whose `keyof` operand is a **naked declaration type
parameter** mapped to a **union** distributes — a union of plain per-member substitutions of
the whole node (`Ident<A | B>` = `Ident<A> | Ident<B>`); `never` → `never`. Unlike M25 the
union is built **eagerly** (a mapped node is inherently lazy — interned, expanded only at
evaluation demand — so no `InstantiationType` wrap is needed; a union member is never a union,
so per-member recursion cannot re-distribute). Covers alias instantiation AND generic-call
instantiation (both route through `substitute`). The DIRECT `{ [K in keyof (A | B)]: … }`
form never fires the guard (its key source is a union at lowering, not a parameter) and gets
**evaluation-time common-key semantics** instead: `homomorphic_source_props` intersects the
members' keys (tsc: `keyof (A|B)` = `keyof A & keyof B`), each common property's type the
union of member types, `?`/`readonly` OR-ed — `d7`/`AB_direct_union` pin the `{}` contrast.
Unit test `homomorphic_mapped_distributes_over_naked_param_union_only` pins guard + non-guard.

**No permissive fallback.** `assemble_mapped` restructured: error/`any` key source → the
**error type** (M22 cascade suppression — e.g. `keyof Bogus` after TK2304); an object with
index signatures, a primitive, a union with a non-object member → the node stays **DEFERRED**
(its own value, memoized `ty → ty`, conservative relations — s1 pins `Ident<{[k: string]:
number}>` rejecting both directions); non-homomorphic key sets became strict
(`literal_string_keys` → `Option`, any non-string-literal member defers the node — silently
dropping a key would shrink the target, a missed-member false negative).

**Self-referential mapped alias → TK2456.** New `Pass::resolving_alias` context
(decl id + name span + name, save/restored per nested `resolve_type_decl` — kept SEPARATE
from `resolving_conditional_alias` so nested conditionals inside plain alias bodies keep M25
behavior). `lower_mapped_type` surface-checks the key source (the `keyof` operand, or the
bare constraint) against it via the existing `check_surface_references` → `TK2456` at the
alias declaration, alias degrades to the error type (downstream silent — no spurious member
errors; `D_selfref` confirms). Documented divergence: tsc also emits TS2313 for `K`'s
circular constraint — secondary code omitted.

**Bonus fix (review probe X):** `replace_mapped_value` now descends into a nested mapped
node's **key source** (which is outer-scoped — `Outer<T> = { [K in keyof T]: Ident<T[K]> }`
injects the outer placeholder there at instantiation) while still leaving the nested node's
value template opaque (it rebinds its own placeholder). `X_nested_mapped` now matches tsc
exactly.

**Reviewer-probe audit (all 36 re-run, tsc 6.0.3 vs release binary).** Exact code-multiset
match on 27, including every mandatory probe: `A_distribute`, `AA_ret_union`, `N_part_union`,
`Q_call_inst`, `V1`–`V5`/`V_exact_undef`, `F_index_sig`, `AB_direct_union`, `X_nested_mapped`,
plus `AC/AD/B/C/G/I/J/M/O/P/R/S/T/U*/W`. Non-exact, each attributed:
- `D_selfref` — TK2456 only vs tsc TS2456+TS2313: the documented omitted-secondary divergence.
- `Fattr_nonliteral` — 1×TK2322+1×TK2339 vs 2×TS2322: member access on the deferred
  index-signature-source node is TK2339 where tsc resolves it to the index value type —
  the s1-pinned deferral divergence (over-report, safe).
- `Z_class_field`/`Z2_classfield_dir` — a mapped node in a **class field** is not evaluated
  at member-position demand: the mapped analog of the documented M25 backlog-27 divergence
  (conditionals buried in named alias/interface/class bodies stay deferred; over-report, safe).
- `Y_demand` — the `par(...)` line is TK2345 vs tsc's contextual TS2322: the documented
  argument-code mapping; same line errors in both.
- `Eattr_plain`/`Eattr_keyof`/`E_iface_inherit` — PRE-EXISTING interface-`extends` member
  composition gap (`Derived extends Base` does not compose `Base`'s members): `Eattr_plain`
  contains **no mapped types** and fails identically, so the `Ident<Derived>` divergence is
  fully attributed to the existing gap, not M26. Flagged for the leader (candidate backlog:
  interface heritage composition).
- `U_memo` — typokat reports excess (TK2353) AND missing (TK2741) on one literal where tsc
  stops at TS2353: pre-existing plain-object behavior (attribution probe: identical without
  mapped types).

**Gates:** `cargo test` green — 186 unit + conformance (all five m26 fixtures incl.
`union_distribution.ts` and the amended `modifiers.ts`/`evaluation_sites.ts`); `clippy
--all-targets -D warnings` clean; release rebuilt; official-suite `run --check` exit 0 —
`regressions: 0, progress: 0, missing-from-corpus: 0` (mapped gate untouched; no `--save`).
No commit.

**Round-2 files:** `types/substitute.rs` (distribution guard + test),
`check/checker/eval.rs` (assemble restructure, `homomorphic_source_props`, strict
`literal_string_keys`, `strip_undefined` → `never`, nested-key-source descent, test fixes),
`check/checker/{context.rs, mod.rs}` (`resolving_alias`), `check/checker/decls.rs`
(save/restore wiring), `check/checker/annotations.rs` (TK2456 surface check).
