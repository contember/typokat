# OUTCOME (closed 2026-07-05) — SHIPPED

**M27 shipped.** Template literal types: interned alternating-texts/holes node;
construction (literal collapse, budget-metered cartesian products, boolean/never/
number rules); the pattern relation arm (anchored non-greedy matching, correct
directionality, subsumption); `infer` extraction on literal anchors through the
non-widening candidate mode; adjacent-hole poison (M25 mechanism); deferred
conservatism + the → `string` allowance. Cross-milestone, spec-driven: call-site
inference preserves literals for primitive-constrained params (tsc
`hasPrimitiveConstraint`), and candidate collection is now raw-only with widening
in exactly one place.

**Commit map.** Spec `6752f56` · plan `e5a5214` · spec cleanup `778ed60` (c3
forward-reference accident + c3b) · implementation `87a02d4` (+1429/−134, incl.
backlog 30 filing + dead-collector deletion) · gate lift + ratchet `1622913`.

**Verification.** 194 unit + conformance green (m27: 4 files / 26 markers), clippy
clean. Adversarial review: **PASS** (41 probes; no M27-attributable false
negative; the inference change regression-swept). Gate lift: 5 files enter scope —
2 clean-kept, 1 with 2/2 matched (a direct pattern-relation win), 2 with one
audited over-report fp each. Scoreboard: in-scope 495→500, clean-kept 168/215,
**error-exact 21→22**, diag-recall 252/1659, 0 regressions.

**Deferred / byproducts.** Backlog `30` (negative numeric literals lower to `any` —
pre-existing HIGH found by the review's attribution work; `number_to_string` not
JS-exact — an UNDER-report at ≥1e21, doc direction corrected). Unevaluated holes
(nested template/conditional instantiations) stay symbolic — over-report,
documented; adjacent-hole resolution (tsc one-char rule) and `Uppercase`/…
intrinsics → backlog `12`.

---

# Sprint — template literal types `` `a${T}` `` / M27 (2026-07-05)

**Goal.** Ship backlog [`11`](../backlog/11-template-literal-types.md): template literal types
are lowered to an interned node, **constructed** when all holes are concrete (literal collapse,
cartesian union distribution, `boolean` expansion, `never` short-circuit, numeric
stringification), act as **patterns** in the relation engine (anchored segment matching for
`${string}`/`${number}` holes, correct directionality), support **`infer` extraction** on
literal anchors (non-greedy), and stay **deferred** on unresolved params (conservative model +
the deferred-template → `string` allowance).

**Theme.** Third milestone of the type-level phase; completes the trio the utility milestone
(`12`) builds on. Today template literal types are silently permissive (probe: every m27
fixture passes with exit 0). Builds on the shared evaluator (work-stack/memo/budget), the M25
conditional/infer machinery (template patterns in extends), and the M25/M26 deferred-node
relation model.

## Refs re-verified at HEAD (2026-07-05, 6752f56)

- ✔ **Template literal types unchecked today** — probe: `const g: \`hello ${string}\` =
  "goodbye world"` → exit 0. The official-suite harness still GATES `template-lit-type`
  (`tsofficial.py`) — the leader lifts it at ratchet time.
- ✔ **Evaluator machinery** — `src/check/checker/eval.rs` (work-stack, memo with provisional
  discipline, budget); M25 conditional nodes + the non-widening infer collection mode
  (`infer_from_types_for_conditional`, `src/check/infer.rs`); M26 union-distribution guard in
  `src/types/substitute.rs` (`apply_mapped` — the M27 template analog distributes holes).
- ✔ **Deferred-node relation dispatch** — `src/relate/relation.rs` carries the M25/M26
  conservative rules; template patterns add a new relation arm (literal ↔ pattern, pattern ↔
  pattern) that must go through the same dispatch.
- ✔ **String literals** — `LiteralValue::String` interned with precomputed hashes; unions
  canonicalize (cartesian products must canonicalize like any union).
- ✔ **tsc 6.0.3 probes** (scratchpad `m27_p1`/`m27_p2`): all corpus behaviors, incl. pattern
  subsumption (`\`hello ${string}\`` → `\`h${string}\`` yes, → `\`x${string}\`` no), `string`
  NOT assignable to any pattern with literal text, boolean → `"is:false" | "is:true"` (display
  order tsc-side), infer-in-template resolving, adjacent-hole tsc semantics (first hole = one
  char — out of scope, poisoned).

## Work units

### WU1 — Repr + lowering + construction (effort M)

- **Scope.** Interned template-literal node: alternating literal text segments and hole
  `TypeId`s (canonical form: no empty interior text between adjacent holes is representable —
  adjacent holes are a lowering-time fact used by WU3's poison rule; leading/trailing empties
  normalized). Hash/eq/substitute (holes are ordinary types; substitution may make holes
  concrete). **Construction** (evaluator): when every hole is a string/number/boolean literal
  or a union thereof — collapse to a literal or the cartesian-product union (canonicalized,
  through the shared budget so combinatorial blowups hit TK2589, not memory); `never` hole →
  `never`; `boolean` expands to the two literals. Non-literal holes (`string`, `number`,
  unresolved params) keep the node symbolic. Demand sites: the M25/M26 set.
- **Acceptance / witness.** `construction.ts` + the `deferred_generics.ts` instantiation lines.
- **Touch points.** `src/types/repr.rs`/`hash.rs`/`intern.rs`/`substitute.rs`,
  `src/check/checker/annotations.rs` (lowering), `eval.rs` (construction), `diagnostics.rs`
  (display: backtick form).

### WU2 — Pattern relation rules (effort M)

- **Scope.** In the relation engine (existing dispatch, reason chains): (1) **string literal →
  pattern**: anchored segment matching — literal prefix/suffix/separator anchors must appear in
  order; `${string}` holes match any (possibly empty) segment; `${number}` holes match a
  numeric segment (decimal forms; scientific/edge forms may conservatively fail — over-report,
  document). (2) **`string` → pattern**: NO (unless the pattern is a single bare `${string}`
  hole with no literal text). (3) **pattern → `string`**: yes. (4) **pattern → pattern**:
  subsumption via anchor containment (the probed `h${string}` case); conservative `No` where
  unclear (over-report, document). (5) Deferred templates (free params in holes): identical
  node; deferred → `string` allowed; nothing else in.
- **Acceptance / witness.** `pattern_assignability.ts`, `deferred_generics.ts`.
- **Touch points.** `src/relate/relation.rs` (one new arm + the deferred rules).

### WU3 — `infer` in template patterns (effort M)

- **Scope.** Extend the conditional-evaluation matcher: an extends type that is a template
  pattern with infer holes matches a string-literal check type by **non-greedy anchored
  scanning** (first-anchor split; prefix/suffix/middle shapes per the corpus); matched segments
  become string-literal candidates through the existing non-widening collection mode;
  distribution composes (per-member extraction). **Adjacent infer holes (no literal separator
  between them) POISON the conditional at lowering** (the M25 cross-binder mechanism — never
  evaluates, conservative relations; documented divergence, tsc resolves with one-char-first
  semantics). No-match → false branch.
- **Acceptance / witness.** `infer_extraction.ts` (incl. the poison line and the m5-style
  regression guards in m25 corpora staying green).
- **Touch points.** `eval.rs` (matcher), `annotations.rs` (adjacent-hole poison at lowering),
  `src/check/infer.rs` (candidate positions for template holes).

### WU4 — Independent adversarial review + ratchet (effort M)

- **Scope.** Hunt false negatives, cross-check tsc: pattern-matching edges (empty segments,
  repeated anchors, unicode/multibyte text, `${number}` numeric forms), construction ×
  distribution interplay (cartesian canonicalization, memo soundness), infer edges (anchor at
  string start/end, empty matches, same-name multi-hole unification, template infer × M24
  constraints), deferred conservatism, poison boundaries (a healthy separator-ed pattern next
  to an adjacent-hole pattern), display stability. Then ratchet — LIFT the harness
  `template-lit-type` gate and audit every file entering scope.

## Out of scope (explicit)

- `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` intrinsics — backlog `12` (arith/string
  intrinsics per architecture §7.3).
- Adjacent infer holes (poisoned — documented; a future item may add tsc's one-char rule).
- `infer X extends C` in template holes; mapped `as` key remapping with templates (backlog
  `11`'s original `as` note moves with utility work).
- Scientific-notation/bigint numeric hole forms beyond simple decimals (conservative,
  documented as they arise).

## Decisions

- **Adjacent-hole poison** reuses the M25 poison mechanism — one flag, one discipline.
- **Cartesian construction runs under the shared step budget** (TK2589 on blowup) — no
  separate cap.

## Run log

<!-- Append as you work. -->

### WU1–WU3 implementation (2026-07-05)

**Status.** WU1–WU3 implemented; `m27_template_literals` flipped `true`; `cargo test`
green (194 unit incl. 8 new + conformance); `cargo clippy --all-targets -D warnings`
clean; official-suite `run --check` exits 0 (**0 regressions / 0 progress / 0
missing**). WU4 (independent adversarial review + gate lift) is the leader's.

**Node representation (WU1).** `TypeTag::Template` + `TemplateType { texts: Vec<String>,
holes: Vec<TypeId> }`, alternating (`texts.len() == holes.len() + 1`). No extra
normalization — the oxc quasi structure already gives one text per gap; an empty
*interior* text records adjacency (the WU3 poison signal), leading/trailing empties mark
a template that starts/ends with a hole. Identity = ordered `(texts, holes)`, hash-consed
(holes folded into the hash; substitution rewrites holes only). Wired into
substitute / `is_concrete`+`child_types` / `contains_infer` / `substitute_infers` /
`replace_mapped_value` / display / flow `typeof` narrowing.

**Construction (WU1, `eval.rs::eval_template`).** All-literal (incl. union) holes →
cartesian product string set → literal or canonical union; `boolean` → `"false"|"true"`
BEFORE the product; `never` hole → `never`; a non-literal hole (string/number intrinsic,
free param, `infer`) keeps the node symbolic (un-memoized, like a deferred conditional);
number holes stringify via `repr::number_to_string` (JS `String(n)` for the common
finite cases; `-0` → `"0"`). Scientific/large-magnitude stringification is a **known
unsound gap — backlog 30** (review FN-2): Rust's shortest-form Display diverges from JS
`String(n)`, e.g. `` `${1e21}` `` constructs `"1000000000000000000000"` where tsc's
type is `"1e+21"` — typokat accepts a string tsc rejects (an UNDER-report, not a
conservative divergence). The
cartesian iteration meters each combination against the shared step budget → `TK2589` on
blow-up, not OOM. Demand sites are the existing M25/M26 set (`maybe_evaluate` +
`evaluate_type`), plus `evaluate_type`'s cheap-reject and `eval_union`'s needs-eval now
include `Template`.

**Matcher shape (WU2 relation / WU3 infer).** One anchored-scan shape shared in spirit by
the relation matcher (`relation.rs::match_literal_against_pattern`) and the infer capturer
(`infer.rs::infer_from_template`): prefix anchored at start, each interior text a
left-to-right **non-greedy** separator (first occurrence), the **last** hole spans the
remainder up to the trailing suffix. `${string}`/infer holes match any segment;
`${number}` a decimal; a literal hole its own value; a union hole any matching member.
Relation dispatch is a new arm placed AFTER `unknown`/`never`/`void`/literal-widening and
BEFORE the object rule (so `template <: unknown`, `never <: template`, union rules all
win first) and re-uses the existing `Relation::No(ReasonChain)` reason path — cache /
cycle-stack untouched. Template **source** → `string` (every template is a string) and →
subsuming pattern (`template_subsumes`: bare `${string}` accepts all; else a single
`${string}`-hole target by prefix/suffix anchor containment; multi-hole / `${number}`
target conservative `No`). Template **target**: string-literal by anchored match; the
`string` intrinsic only by the bare `${string}` hole; everything else `No`. Deferred
templates (free-param hole) relate only identically (fast path) + deferred → `string`.

**Numeric-segment rule (WU2).** `` `${number}` `` accepts a segment iff it is a decimal
(digits, ≤1 interior dot) AND `number_to_string(parse) == segment` — intended as tsc's
`String(Number(s)) === s`, which rejects redundant leading/trailing zeros. Signed /
scientific / `Infinity` / `NaN` forms are conservatively rejected (over-report, safe).
BUT the round-trip validates against `number_to_string`, which shares the backlog-30
stringification gap: at large magnitudes it accepts digit strings tsc rejects (e.g.
`"1000000000000000000000"` round-trips through Rust's Display where JS gives `"1e+21"`)
— a **known unsound gap — backlog 30**, not conservative there.

**Adjacent-infer poison (WU3).** At template lowering, an empty interior separator between
two holes where either is an `infer` node poisons the innermost active conditional frame
(the M25 `poisoned` flag) — one flag, one discipline. The poisoned conditional never
evaluates and relates conservatively (`infer_extraction.ts` p1: `Adj<T>`'s true branch is
a bare `infer` node, so `relate_conditional_source`'s `contains_infer` guard returns the
conservative `No`). Documented divergence — tsc resolves these one-char-first.

### Deviation / cross-milestone finding — primitive-constraint literal inference

`deferred_generics.ts` c4/c5 (`mk<T extends string>("x")` OK, `mk("y")` error) **require**
literal-preserving inference for a primitive-constrained type parameter — tsc's
`hasPrimitiveConstraint` rule. The pre-existing engine widened `mk("x")` → `string`
(probed), which would make c4 wrongly error (`` `tag:${string}` `` ≠ `"tag:x"`). This is a
generic-inference (M10/M24) change, strictly speaking outside the template-literal repr /
relation scope, but the committed corpus cannot be green without it (the fixture matches
tsc, so it is not "wrong"). Implemented minimally in `infer.rs`: call-site collection now
records **raw** (un-widened) candidates and `fix_candidates` widens **per parameter**,
skipping the widen when `has_primitive_constraint` holds (constraint is / contains a
primitive intrinsic, a literal, or a template pattern). **Soundness:** whenever this
preserves a literal, tsc also preserves (tsc's `widenLiteralTypes = !primitiveConstraint
&& …`), and `is_primitive_ish` is a *subset* of tsc's mask (missing bigint/enum/esSymbol
→ we widen where tsc might preserve → the safe over-report direction). **Flagged for the
leader's review** as a scope note.

### Deviation — error-typed hole degrades the template (M22)

`deferred_generics.ts` c3 (`mk(someStr)` with `someStr` used before its `declare const`)
tripped a pre-existing forward-reference gap: an ambient `declare const` used before its
textual declaration resolves to the **error type**, so `T` inferred the error type and the
return became `` `tag:${error}` ``. A template with an error-typed hole is not itself
error-like, so without handling it produced a cascade `TK2322`. Fixed in `eval_template`
with the M22 discipline `assemble_mapped` already uses: an error-typed hole degrades the
whole template to the error type (cascade suppression). The forward-reference resolution
itself is out of scope (unchanged).

### Official-suite audit

`run --check` → exit 0, **regressions 0 / progress 0 / missing 0** — the scoreboard's
per-file diagnostic counts are unchanged by every change here. Template-lit files are
still harness-gated (`syntax:template-lit-type`, leader lifts at ratchet), so no movement
there was expected; the string-literal-pattern relation and the primitive-constraint
inference change touched **no** ungated scoreboard file (the literal-vs-widened inference
difference is rarely diagnostic-observable, and it only ever aligns typokat *toward* tsc).

### Post-review final round (2026-07-05, per WU4 verdict)

- **Dead collectors removed** (review follow-up): `collect_call_candidates` and the
  always-widen public `infer_from_types` (+ `InferenceContext::new`) had no callers
  after the raw/widen refactor. Deleted; the constant `widen_literals` field went with
  them (both surviving modes record raw — call-site widening lives solely in
  `fix_candidates`, so a third collector mode cannot drift). Surviving entry points:
  `infer_type_arguments` (calls.rs) and `infer_from_types_for_conditional` (eval.rs);
  the union-descent test now pins the call-site mode via `infer_from_types_raw`.
- **FN-2 direction corrected** above and in `tests/cases/README.md`: large-magnitude
  numeric stringification is a known UNSOUND gap (backlog `30`), not "conservative".
- **Unevaluated-hole note** added to the README divergence list: a hole that itself
  needs evaluation (nested template, conditional/alias instantiation) stays symbolic —
  over-report, safe (review probes p10/p07j).
