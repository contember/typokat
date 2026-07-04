# OUTCOME (closed 2026-07-04) — SHIPPED

**M25 shipped** — the type-level evaluation phase is open. Conditional types are an
interned node (infer binders = de Bruijn indices scoped to the node, ADR-0002),
evaluated on demand through an explicit work-stack with memoization (provisional
discipline), a ~1000-step budget (`TK2589`), and alias-cycle detection (`TK2456`);
distribution over naked-param unions (incl. `boolean` expansion) at alias AND
generic-call instantiation; infer extraction through a dedicated non-widening
inference mode with union-target descent and tsc's naked-vs-structural candidate
priority; deferred conditionals relate conservatively; cross-binder nested infer is
poisoned (sound stopgap, backlog `26`).

**Commit map.** Spec `8169767` · plan `ff4f89b` (with ADR-0002 `85786ec` pointer) ·
spec amendments `c0f1807` (call-site distribution), `8a758d9` (poisoned nested
infer + backlog 26), `390af62` (infer literal widening + param-position evaluation +
backlog 27), `1b0c114` (candidate priority) · implementation `8255669` (+2519/−26,
new `eval.rs`) · harness ungate + relabel `a69de61`.

**Verification.** 180 unit + conformance green (m25: 6 files / 47 markers; 10k-deep
work-stack stress test, no-capture and priority unit tests), clippy clean. Official
suite: measurement-neutral (conditional-heavy files still gated by mapped-type/
template-literal/variadic buckets — signal arrives with `10`–`12`/`24`). Four fix
rounds: two from leader verification (call-site distribution — the implementation's
own flag confirmed as a dropped error; nested-infer poisoning after probes showed
verdict inversions), one adversarial-review FAIL (infer literal widening corrupting
results AND branch selection, union-member collection, param-position over-report),
one re-review residual (naked-member candidate priority). Final verdict: PASS.

**Deferred.** Cross-binder nested infer resolution (backlog `26`); template-buried
conditional evaluation (backlog `27`); `new`/`super` construct-path param-position
evaluation (over-report today — review note); contravariant same-name intersection
(needs intersections, backlog `25`); `infer X extends C`; rest-element infer shapes
(backlog `24`); `type A = A` circularity (out of scope).

---

# Sprint — conditional types `T extends U ? X : Y` / M25 (2026-07-04)

**Goal.** Ship backlog [`09`](../backlog/09-conditional-types.md): conditional types are lowered
to an interned node, **evaluated** when the check type is concrete (extends test through the
relation engine, `infer` extraction, distribution over unions), stay **deferred** on an unresolved
type-parameter check with conservative assignability, and the evaluator carries its required
machinery — memoization keyed on interned `TypeId`s, an explicit work-stack (no host recursion),
a depth limit (`TK2589`), and alias-cycle detection (`TK2456`).

**Theme.** Opens the type-level evaluation phase (`09`–`12`, architecture §7). Today conditional
types are silently permissive (probe: `IsString<string> = false` passes — a pure false-negative
class). The de Bruijn question that gated this milestone is resolved by
[ADR-0002](../decisions/0002-de-bruijn-scoped-to-infer-binders.md): indices for `infer` binders
scoped to their conditional node; declaration params stay named `TypeParamId`s.

## Refs re-verified at HEAD (2026-07-04)

- ✔ **Conditional types are unchecked today** — probe: `type IsString<T> = T extends string ?
  true : false; const c: IsString<string> = false` → exit 0, no diagnostics.
- ✔ **Type params** — `src/types/repr.rs:362-383`: named unique `TypeParamId`, identity = id
  alone; the repr comment flags the §3.1 de Bruijn deviation (now governed by ADR-0002). M24
  (in flight) adds a constraint column keyed by `TypeParamId` and the apparent-type rule in
  `src/relate/relation.rs` (~440-451).
- ✔ **Inference engine** — `infer_type_arguments(&mut interner, &[TypeParamId], targets,
  sources)` (`src/check/infer.rs:144`), candidates keyed by `TypeParamId`; handles function
  param/return positions (m10).
- ✔ **Rest elements are NOT in the type model** — `src/types/repr.rs:330` (optional/rest params
  out of the M3 subset); probes show `[string, ...number[]]` and `(...args: never[]) => number`
  are silently permissive. M25 fixtures avoid rest entirely; gap filed as backlog `24`.
- ✔ **Boolean** — an intrinsic (`repr.rs:64`) plus `LiteralValue::Boolean(bool)` literals
  (`repr.rs:133`); not a `true | false` union. Distribution must expand it explicitly.
- ✔ **tsc 6.0.3 probes** (scratchpad `m25_p1`–`m25_p7`): distribution over naked params only
  (`[T] extends [U]` and `(T | undefined)` do not distribute); `never` → `never`; `boolean` →
  `false[] | true[]`; same-name covariant infers union; infer name in false branch → TS2304;
  deferred conditional assignable to itself / union of branches / supertypes thereof, literal
  NOT assignable into it; call sites resolve; genuinely-infinite growth → TS2589 (tail-recursive
  aliases iterate ~1000 steps first); direct self-reference → TS2456 at the alias declaration.

## Work units

### WU1 — Repr + lowering: the conditional node, infer binders (effort M)

- **Scope.** (1) An interned **conditional-type node** `{check, extends, true_branch,
  false_branch, infer_binder_count}` (hash-consed like every type; canonicalization per §3.3
  does not reorder its fields — position is meaning). (2) Lowering from the oxc AST at every
  type-annotation site: `infer U` names bind to **de Bruijn indices within the node**
  (ADR-0002); the same name in multiple positions binds the same index; an infer name
  referenced in the false branch is out of scope → **`TK2304`** (tsc behavior). (3) `substitute`
  maps over all four fields and **must not capture**: the node's own bound indices are never
  substitution targets. (4) Display: render `C extends E ? T : F` (code-only markers in
  fixtures — display is unstable).
- **Acceptance / witness.** Lowering round-trips (unit tests); `infer_extraction.ts`'s TK2304
  line; no behavior change elsewhere (conditionals still unevaluated until WU2 — full corpus
  stays red until then, dir registered `false`).
- **Touch points.** `src/types/repr.rs`, `intern.rs`, `store.rs` (node + display),
  `src/check/checker/decls.rs` (lowering), `src/diagnostics.rs` (nothing new — TK2304 exists).

### WU2 — The evaluator (effort L)

- **Scope.** Demand-driven evaluation when a lowered/instantiated conditional has a **concrete
  check type** (no free declaration params):
  1. **Distribution**: a check type that was a *naked* declaration param distributes over union
     members after substitution; `never` → `never`; the `boolean` intrinsic expands to
     `true | false` members first. Non-naked checks (tuple-wrapped, widened) evaluate once.
  2. **The extends test** runs through the relation engine (existing entry points, cycle stack
     and cache discipline untouched). With infer binders present, candidate collection runs
     through the **inference machinery**, not ad-hoc matching: prefer generalizing candidate
     keying over binder ordinals; freshening node binders to transient `TypeParamId`s is an
     acceptable fallback if the generalization balloons — decide and record in the run log.
     Same-name candidates union (covariant fixture behavior; multi-occurrence contravariant
     intersection is out of scope, documented).
  3. **Branch selection + substitution** of matched candidates into the taken branch; an
     unmatched infer in a taken true branch cannot happen (match implies candidates); a
     no-match takes the false branch unchanged.
  4. **Machinery (required, architecture §7.2)**: memoization `(conditional TypeId after outer
     substitution) → result TypeId` — sound because hash-consing makes the key total; an
     explicit **work-stack** — no host recursion per evaluation step (witness: a Rust unit test
     builds a ~10k-deep nested type programmatically and evaluates an `Unwrap`-style descent
     without stack overflow — a fixture would stress the parser instead, so this witness is a
     unit test); a **per-root step budget (~1000, matching tsc's tail-iteration order)** →
     **`TK2589`** at the annotation that demanded evaluation (span divergence vs tsc documented
     in `tests/cases/README.md`); **alias re-entry detection** during check-type resolution →
     **`TK2456`** at the alias declaration, alias becomes the error type (silent downstream,
     m22 discipline) — scoped to surface-resolution re-entry so m5 recursion-through-members
     stays legal.
- **Acceptance / witness.** `basic_resolution.ts`, `distribution.ts`, `infer_extraction.ts`,
  `recursion_depth.ts` green; the 10k-deep unit stress test; m5/m9/m10/m16 corpora unchanged.
- **Touch points.** New `src/check/eval.rs` (or `checker/eval.rs` — evaluator + work-stack +
  memo), `decls.rs` (demand sites: annotations, alias instantiation, type references),
  `src/check/infer.rs` (candidate keying), `src/diagnostics.rs` (TK2589, TK2456).

### WU3 — Deferred conditionals in the relation engine (effort M)

- **Scope.** A conditional whose check type still contains a free declaration param does not
  evaluate; it is an ordinary interned type. Relation rules: (1) identical node ⇒ assignable
  (integer compare, free today); (2) **conditional as source**: assignable to `X` if **both
  branches** are assignable to `X`, *only when both branches are closed w.r.t. the node's own
  infer binders* — otherwise conservative `No` (sound over-report, documented); (3) nothing is
  assignable *into* a deferred conditional except an identical one (conservative, matches
  probed tsc). Reason chains flow as usual — never a bare bool. Cache stays sound: deferred
  nodes are context-free interned types.
- **Acceptance / witness.** `deferred_generics.ts` green (incl. call-site resolution through
  generic instantiation); no relation-cache regressions (order-independence spot checks).
- **Touch points.** `src/relate/relation.rs` (two rules, through existing dispatch),
  `src/check/checker/calls.rs` (return-type instantiation already substitutes — evaluation
  then triggers on the now-concrete check).

### WU4 — Independent adversarial review + ratchet (effort M)

- **Scope.** Hunt false negatives, cross-check vs tsc 6.0.3 --strict: nested conditionals
  (branch of a conditional is a conditional); distribution × infer combined; distribution over
  aliased/nested unions; memoization poisoning (same conditional, different args — a memo hit
  must key on the *substituted* node); order-independence (evaluate-then-relate vs
  relate-then-evaluate); TK2589/TK2456 boundaries (16-deep must pass, runaway must trip; legal
  m5 recursive object aliases must NOT trip TK2456); deferred-conditional soundness (nothing
  leaks assignable-into); interaction with M24 constraints (`<T extends string>` + conditional
  on `T` stays deferred). Then ratchet the official-suite scoreboard.

## Out of scope (explicit)

- **Rest elements / rest params in the type model** — filed as backlog
  [`24`](../backlog/24-rest-elements-in-type-model.md); fixtures avoid them.
- `infer X extends C` constraint syntax (TS 4.7) and contravariant multi-occurrence
  intersection inference — documented divergences.
- Mapped types, template literals, `keyof`-dependent conditionals (`10`–`12`).
- Tail-position analysis / accumulator reuse (architecture §7.2 items 2 & 4) — they belong to
  the tuple-building milestones (`10`+); M25's step budget covers termination.
- General circular-alias detection (`type A = A` without a conditional) — may fall out of the
  WU2 alias re-entry machinery, but is not specced here (no fixture; if it lands, document).

## Decisions

- **ADR-0002**: de Bruijn scoped to infer binders within conditional nodes; declaration params
  stay named ids; full §3.1 migration demoted to a measured optimization.
- **Marker/span**: TK2589 at the annotation that demanded evaluation (tsc attributes it inside
  the alias body) — documented divergence; TK2456 at the alias declaration (matches tsc).
- **Step budget ~1000 per evaluation root** (tsc tail-iteration order of magnitude, §7.4) — one
  budget, no separate tail/non-tail limits in M25.

## Run log

### Implementation (WU1–WU3, 2026-07-04)

**Representation (WU1).** Three new interned nodes:
- `TypeTag::Conditional` → `ConditionalType { check, extends_ty, true_branch, false_branch,
  infer_count, distributive }` — hash-consed by all fields **in field order** (position is
  meaning; branches are not a canonical set). Reserve/fill (`reserve_conditional`/
  `fill_conditional`) mirrors nominal objects, for recursive templates.
- `TypeTag::Instantiation` → `InstantiationType { base, args: sorted Vec<(TypeParamId, TypeId)> }`
  — a **lazy** `substitute(base, args)`. Needed so a self-recursive conditional alias
  (`type Unwrap<T> = … Unwrap<U> …`) does not expand at lowering (which loops through the M5
  `resolving` guard → error type) but at evaluation. A generic **alias** reference to a
  conditional template lowers to this (`instantiate_type_reference`); a generic **function**
  return uses plain `substitute` (single concrete check, no distribution needed for the corpus).
- `TypeTag::Infer` → de Bruijn index in `payload` (ADR-0002). `substitute` leaves it untouched
  (no-capture) — witnessed by `substitute_does_not_capture_infer_binders_under_nesting`.

**Nakedness tracking (decided).** `distributive: bool` is a **flag on the node**, set at
lowering iff the check type lowered to a bare `TypeTag::TypeParam` (`T extends …`, not
`[T] extends …` / `(T | undefined) extends …`). Recorded on the node because substitution
erases the naked parameter. Distribution happens **only** at `Instantiation` evaluation
(deriving per-member branches by plain-substituting the template with `check-param → member`),
never in `substitute` — keeping `substitute` a pure structural rewrite.

**Candidate keying (decided — the freshening fallback).** Per ADR-0002's allowed fallback:
each conditional evaluation with `infer_count > 0` allocates `infer_count` transient
`TypeParamId`s from the module counter, substitutes `Infer(i) → TypeParam(fresh_i)` into the
extends type + true branch, and reuses the **existing** `infer.rs` machinery
(`infer_from_types`) to collect candidates keyed on the fresh ids. Same-name occurrences union
(covariant rule); no-candidate → `unknown`. Added a `(Tuple, Tuple)` positional arm to
`infer.rs` (needed for `[infer H, infer R]`). Chosen over generalizing candidate keying to
node-scoped binders because freshening is a ~40-line localized addition vs. touching every
`infer.rs` recursion arm.

**Memo-soundness argument.** The evaluator memo is keyed on the interned conditional /
instantiation `TypeId` → result. Sound because hash-consing makes the key **total** (one id ⇒
one type ⇒ one deterministic result over the append-only store). Provisional discipline
(mirrors the relation cache, invariants §1): a result reached (a) while the per-root step
budget is **exhausted**, or (b) by re-entering an **in-flight** id (a genuine self-cycle), is
**never** written to the durable memo — `SetMemo` checks `!exhausted`, and an in-flight
re-entry returns the error type without scheduling a memo write. Work-stack is explicit
(`Task::{Eval, SetMemo, BuildUnion}`), so the deep-recursive descent is heap iteration, not
native recursion (10 000-deep witness `deep_recursive_unwrap_does_not_overflow_the_native_stack`).

**Deferred-conditional relations (WU3).** In `relate_uncached`, placed **before** the union
rules (like the M24 constraint rule) so a conditional source relates to a whole union target:
(1) identical ⇒ `src == tgt` fast path; (2) conditional **source** ⇒ both branches must relate,
gated on both branches being **closed** w.r.t. the node's infer binders (`contains_infer`) else
conservative `No`; (3) nothing into a `Conditional`/`Instantiation` target (or from an
`Instantiation` source) except identical.

### Deviations / limitations (documented, all in the over-report/safe direction unless noted)

- **Message rendering**: the assignment/argument headline now keeps the source **literal** when
  the message target is itself a literal type (`false` → `true`, `2` → `1`), widening only
  against a non-literal target (`"hello"` → `string`) — tsc's behaviour. Localized to
  `emit_obligation_failure` (`is_literal_target`); no existing corpus asserted a literal-target
  message, so no earlier milestone changed.
- **Reserve for all top-conditional aliases**: any alias whose top body is a `TSConditionalType`
  reserves a conditional template id (not just recursive ones) — detecting recursion at reserve
  time needs analysis; uniform reserve is simpler and correct (a reserved non-recursive template
  is just not deduped).
- **Nested `infer`** (an `infer` in a nested conditional referencing an outer binder) is not
  modelled — probed on request: **can drop errors**, → see "Nested-`infer` … probe verdict"
  below (stopped for the coordinator's call).
- ~~**Generic-function-call distribution**~~: was a confirmed in-scope false negative, → fixed;
  see "Amendment — call-site distribution" below.
- **`TK2589` span**: at the annotation that demanded evaluation (already a documented divergence
  in the plan/`tests/cases/README.md`).
- **Witness budget**: the 10k-deep unit test raises the budget above the depth; the default
  `~1000` (`DEFAULT_STEP_BUDGET`) is what real checking uses (trips `Inf`, passes `Deep16`).

### Gates

- `cargo test`: **176** unit tests + conformance all green (was 172+; adds the no-capture,
  3 evaluator, and tuple-infer coverage). m5/m9/m10/m16/m24 corpora unregressed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo build --release` + `python3 tooling/official-suite/tsofficial.py run --check`: exit 0.

### Official-suite audit (vs committed scoreboard)

| Metric | Result | Cause |
|---|---|---|
| regressions | **0** | — |
| progress (new matches) | **0** | — |
| missing-from-corpus | **0** | — |

**No scoreboard state change.** Conditional/`infer` files are gated **out** of the in-scope diff
by the harness's own `syntax:` detection (`syntax:conditional-type` 8, `syntax:infer` 2), so
typokat now evaluating them is never diffed. Non-conditional files are unaffected: the new
relation rules only fire for `Conditional`/`Instantiation`/`Infer`-tagged types (absent from
gated-in files), and the literal-target rendering change alters message text only, never a
diagnostic's line/code (which the scoreboard keys on). No diagnostics were suppressed to protect
the scoreboard (`--check` only, no `--save`, no commit).

### Amendment — call-site distribution (leader-verified FN, spec c0f1807)

**The bug (confirmed in-scope, not a limitation).** A union type argument reaching a
naked-param conditional through a generic **call** (inferred `m(sn)` or explicit
`m<string | number>(sn)`) evaluated ONCE (`string | number extends string` → false branch)
instead of distributing — `const d2: "no" = m(sn)` passed silently where tsc distributes to
`"yes" | "no"` and reports TS2322. Root cause: the call paths instantiate via plain
`substitute`, which baked the union into the check, erasing the distribution decision before
`evaluate_type` ran.

**The fix (decided): a distribution guard in `Substitution::apply_conditional`.** A
*distributive* conditional whose naked check parameter is mapped to a **distributing** type
(union / `never` / `boolean` — `distributes_over`) is not plainly rewritten; it defers as a
lazy `InstantiationType` carrying the substitution map, so the evaluator distributes — the
**same** path alias instantiation takes (per the amendment's directive). One rule covers both
call paths (explicit `instantiate_generic_callee` and inferred `infer_generic_callee` both go
through `substitute`) and **composes**: a recursive `Unwrap<U>` whose matched `U` candidate is
a union re-distributes at the next evaluation step. No-cycle argument: the evaluator's own
per-member substitutions map the check parameter to a **single** non-distributing member
(`distribute_members` returns flattened non-union members; `boolean` expands to the two
literals; `never` members are dropped), so the guard never re-wraps and single-argument
behavior is identical to before (pinned by
`distributive_conditional_defers_on_union_plain_on_single`, incl. the `never`-wraps case).

**Gates re-run**: `cargo test` **177** green (conformance incl. the amended `d1`/`d2`/`d3`
lines); clippy clean; release rebuild + official-suite `run --check` exit 0 — regressions
**0**, progress **0**, missing **0** (conditional/`infer` files remain harness-gated; nothing
else moved). No `--save`, no commit.

### Nested-`infer`-referencing-an-outer-binder — probe verdict: CAN DROP ERRORS (stopped)

Coordinator-requested re-examination of the documented limitation. Three probes vs
`tsc 6.0.3 --strict` (scratchpad `m25_nested_infer/p1`–`p3`):

| Probe | Shape | tsc | typokat | Class |
|---|---|---|---|---|
| p1 | outer `infer U` as the **check** of a nested conditional (`U extends string ? "s" : "n"` in the true branch) | `Inner<{a:"x"}>` = `"s"` → errors on `const c1: "n"` | evaluates to `"n"` → `c1` passes silently, `c2: "s"` errors instead | **verdict inversion: 1 dropped error + 1 false positive** |
| p2 | outer `infer U` in the nested conditional's **true branch** | `P2<…>` = `string` → errors on `const c4: number` | `TK2304 "Cannot find name 'U'"` at the alias (infer lookup checks only the innermost frame), alias → error type, `c4` passes | loud false positive at the alias; the genuine tsc error is still unreported (m22 suppression) |
| p3 | outer `infer U` as nested check + nested node declares its **own** `infer V` (both index 0 of their nodes — the collision shape) | `P3<{a:string[]}>` = `string` → errors on `const c6: "no"` | evaluates to `"no"` → **file completely silent, zero diagnostics** | **pure dropped error** |

**Verdict: this limitation CAN drop errors in-scope** (p1 and p3 are silent false negatives;
p3 has no diagnostic anywhere in the file), not merely over-report. Mechanism: (a)
`lower_conditional_type` lowers the nested **check** while the OUTER infer frame is still
innermost, so the outer binder lands as a raw `Infer(i)` inside the nested node, where the
index is meaningless (and collides with the nested node's own binders — flat per-node indices
cannot distinguish them without de Bruijn shifting); (b) the evaluator's `substitute_infers`
deliberately does not descend into nested conditionals, so the matched candidate never reaches
the stale `Infer` node, which then fails the extends test as an opaque leaf → wrong branch. A
fix needs index shifting on embed or level-tagged binders — a design call.
**Stopped without implementing, per the coordinator's instruction; awaiting their call.**

### Amendment — cross-binder nested infer: the poison stopgap (leader call, spec 8a758d9)

**The call.** No binder arithmetic in this milestone — ship the sound stopgap; the proper de
Bruijn shifting is filed as backlog [`26`](../backlog/26-cross-binder-nested-infer.md). Spec
pinned in `m25_conditional_types/nested_infer.ts` (+ README divergence note, leader commit).

**Implementation.**
- **`poisoned: bool` on `ConditionalType`** — identity-bearing like `distributive` (folded
  into `StructuralKey::Conditional`, the structural hash, and the interner's eq tie-break).
- **Lowering** (`annotations.rs`): the flat `infer_scopes` stack became `cond_frames:
  Vec<CondFrame>` — one context per `lower_conditional_type` call, pushed for the WHOLE node
  (check/extends/true/false positions), with the node's binders in scope (`active`) only
  during extends + true. `resolve_infer_reference` searches ACTIVE frames innermost-first: a
  hit on the innermost context is an own-binder reference (normal); a hit on an OUTER frame is
  cross-binder — it resolves (no spurious `TK2304`, the p2 fix) and poisons every frame from
  the owner to the innermost (covers check, extends, true, AND nested-false positions; N-level
  nesting poisons the whole chain). A name in no active frame still falls through to `TK2304`
  (`BadScope` in `infer_extraction.ts` unchanged — an own binder referenced from the node's
  own false branch).
- **Evaluation off, substitution on**: `eval_conditional` returns a poisoned node as-is
  (before the step budget / in-flight machinery — it is a value, like a deferred check);
  `expand_instantiation` treats a poisoned base as non-distributive (plain-substitute
  fallback → a substituted, still-poisoned node); `apply_conditional` skips the distribution
  guard for poisoned nodes and carries the flag through the plain rewrite — so declaration
  params still instantiate structurally, only evaluation is off. Deferred-node conservative
  relations (WU3) then apply unchanged: nothing assignable in; the both-branches source rule
  bottoms out on the `contains_infer` gate at the level where the stale `Infer` leaf actually
  sits (sound — resolution always picks a branch, recursively).

**Probe re-verification (p1–p3 vs tsc 6.0.3 --strict).** No silent files, no verdict
inversions, no `TK2304` — only over-reports remain: p1 line 4 match + line 5 over-report;
p2 line 5 match + line 4 over-report (the spurious alias `TK2304` is gone); p3 line 6 match +
line 5 over-report. Every genuine tsc error is now reported.

**Gates.** `cargo test` **178** green (conformance incl. `nested_infer.ts` — x1/x2/x3
conservative TK2322s, `OwnInfer` regression guard evaluating normally; new unit test
`poisoned_conditional_never_evaluates` pins both the direct and the through-instantiation
paths); clippy clean; release rebuild + official-suite `run --check` exit 0 — regressions
**0**, progress **0**, missing **0** (conditional/`infer` files remain harness-gated —
`syntax:conditional-type` 8, `syntax:infer` 2 — so the poisoning flips nothing in the in-scope
diff; no other movement). No `--save`, no commit.

### Amendment — adversarial review FAIL → fixed (spec 390af62)

**Review verdict.** WU4 returned FAIL: one root cause, three dropped-error finding classes,
plus a specced review note. Root cause: the freshening fallback reused `infer_from_types`
wholesale, whose TypeParam-target arm **widens literal candidates** (`widen(interner,
source)`) — correct for M10 call-site inference (an inferred type argument is never narrower
than the value denotes; must stay), wrong for conditional `infer` (tsc never widens there).

**Findings → fixes.**
- **HIGH-1** (result too wide → assignments in silently accepted: `ElementOf<"x"[]>` →
  `string` instead of `"x"`, same for `Ret`/`PropA`/`Both`/top-level `Idish<"hi">`) and
  **HIGH-2** (sharper: widening a **contravariant-position** candidate corrupts the extends
  test itself — `PF<(a:"x") => void>` failed the test and took the FALSE branch → `unknown` →
  everything assignable): fixed by a **collection mode on `InferenceContext`**
  (`widen_literals` / `union_target_descent` flags; new entry point
  `infer_from_types_for_conditional` sharing the walker). The evaluator's
  `run_extends_test` now collects un-widened; `infer_from_types` (M10 call inference) keeps
  widening — guarded by the existing `infers_from_scalar_argument` unit test and the
  m9/m10/m16/m24 corpora (unregressed).
- **MEDIUM-3** (infer inside a union member of the extends type collected nothing →
  `unknown`: `UInfer<number[]>` for `T extends string | (infer U)[] ? U : "no"`): in
  conditional mode a **union target** with a non-union source descends into every member —
  a member the source does not shape-match contributes nothing; same-name contributions
  union. Call-site mode keeps the M10 equal-length pairwise rule only (no m10 change —
  pinned by the new `conditional_mode_keeps_literals_and_descends_union_targets` unit test,
  incl. the call-mode-stays-conservative case). The `unknown` fallback for a genuinely
  unbound infer in a taken branch stays (matches tsc; README bullet).
- **Review note (param-position conditionals at call sites)**: `infer_call` now evaluates
  each **substituted parameter type** through the same `evaluate_type` demand as the return
  (span = the call), so `g2("abc", "yes")` is clean and `g2("abc", "no")` reports `TK2345`
  against the RESOLVED `'"yes"'` — previously the deferred node rejected every call (4
  errors where tsc has 2). Covers both the inferred and explicit-args paths (both flow
  through `infer_call`'s parameter snapshot). `new`/`super` parameter positions are
  untouched (no fixture; conditional class-ctor params remain out of corpus).

**Verification.** Reviewer probes re-run as my own: **fn1–fn8 + p12 all tsc-matching**
(identical error-line sets; mismatch messages name the resolved types — e.g. p12
`'"n"' → '"y"'`, fn6 `string → '"x" | "y"'`). Gates: `cargo test` **179** green (amended
`infer_extraction.ts` L1–L12 + `deferred_generics.ts` g2 block; m9/m10/m16/m24 unregressed);
clippy clean; release rebuild + official-suite `run --check` exit 0 — regressions **0**,
progress **0**, missing **0** (no movement; conditional/`infer` files remain harness-gated).
No `--save`, no commit.

### Amendment — round 4: naked union-member infer yields to structural (spec 1b0c114)

**Re-review residual (probes e13/e15).** The MEDIUM-3 union-target descent unioned ALL
same-name contributions, so a binder appearing both as a **naked** union member and inside a
**structural** member (`Unbox<T> = T extends { v: infer U } | infer U ? U : never`) fixed too
wide (`Unbox<{v: string}>` → `string | {v: string}` where tsc gives `string`) — an
assignment of the boxed value into the result passed silently (dropped error). tsc's rule is
**priority, not union**: the naked member's whole-check candidate is LOW priority and is
discarded when any structural member bound the same binder.

**Fix (localized to the descent arm, `src/check/infer.rs`).** The union-target descent now
partitions members into naked (`TypeParam`-tagged after freshening) vs structural, collects
the structural members first into a local candidate set, and records a naked member's
whole-check candidate only when THIS union's structural members bound nothing for that
binder. Naked-only stays kept (`NakedOnly<T> = T extends string | infer U ? U` — L17/L18);
a different-name naked member is never blocked by another binder's structural hit
(`infer A | (infer B)[]` — e16). Pinned by the new unit test
`naked_union_member_candidate_yields_to_structural` (all three shapes). Also fixed the
stale `infer_from_types` doc mention in `eval.rs` (review cosmetic).

**Verification.** Reviewer probes **e12–e16 all tsc-matching** (identical per-line error
sets; e12 source-position stays safe — error only on `number[] = d` in both), plus e1
(call-site widen regression guard) matching. Gates: `cargo test` **180** green (amended
`infer_extraction.ts` L13–L18); clippy clean; release rebuild + official-suite `run --check`
exit 0 — regressions **0**, progress **0**, missing **0** (no movement). No `--save`, no
commit.
