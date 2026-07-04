<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

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

<!-- Append as you work. -->
