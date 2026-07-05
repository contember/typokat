<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — utility types / M28 (2026-07-05)

**Goal.** Ship backlog [`12`](../backlog/12-utility-types.md): the ten standard utility
aliases become **built-ins via a prelude compilation unit** (each is its ordinary
mapped/conditional definition — no second evaluator), `Omit`-style composition works
(mapped key sources that are alias instantiations evaluate on demand), and the
`Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` **intrinsics** evaluate natively.
Closes the type-level evaluation phase.

**Theme.** Leader scope-probe (`m28_scope.ts`): 9 of 10 utility shapes already work as
hand-written aliases on the M24–M27 machinery; only the `Omit` composition fails (its mapped
key source is an alias instantiation the evaluator never demands). So M28 is: (1) prelude
infrastructure — the architecturally anticipated "minimal ambient/prelude slice" (backlog
`14`), (2) one evaluator demand fix, (3) four intrinsics.

## Refs re-verified at HEAD (2026-07-05, 9bc1123)

- ✔ **Driver is single-source** — `check_source(&source)` (`src/driver.rs`): parse → bind →
  check one unit. The prelude needs a second unit bound into a ROOT scope the user scope
  chains to; prelude diagnostics must never surface (trusted source — assert clean in a unit
  test instead).
- ✔ **Omit composition gap** — `type MyOmit<T, K> = MyPick<T, MyExclude<keyof T, K>>` leaves
  the mapped node deferred: `eval_mapped` reads the key source directly and a lazy
  `Instantiation` key source never evaluates. Fix: demand-evaluate the (substituted) key
  source through the shared work-stack before key iteration.
- ✔ **`Pick`'s `K extends keyof T` constraint works at concrete instantiation** (M24
  substitutes the constraint before checking — scope probe t8 + fixture pk3's TK2344).
- ✔ **tsc probes**: fixture cross-check green; `Pick<P, "q">` → TS2344 AND still
  instantiates; `Omit<P, "a">` excess-flags via TS2353 (b optional); TS2820 (did-you-mean
  2322 variant) avoided in fixtures; Uppercase distributes over unions.

## Work units

### WU1 — Prelude compilation unit (effort M)

An embedded prelude source (Rust `include_str!` or const) parsed + bound + resolved BEFORE
user code; user's top scope chains to the prelude scope; user declarations SHADOW prelude
names (tsc-like). Prelude content: the ten aliases (`Partial`, `Required`, `Readonly`,
`Record`, `Pick`, `Omit`, `Exclude`, `Extract`, `NonNullable`, `ReturnType`) written as
ordinary TS type aliases, plus the four intrinsics declared `type Uppercase<S extends
string> = intrinsic;`. Prelude diagnostics: assert-clean unit test; never surfaced to users.
Spans from prelude types render by NAME (alias display), never by prelude location.
Touch: `src/driver.rs`, binder entry, a new `src/prelude.ts`-equivalent asset.

### WU2 — Type-model work for utility composition (effort L; expanded 2026-07-05)

Originally "one evaluator demand fix (S/M)"; the impl agent's pre-implementation probes
(run log below) proved the committed fixtures require **three type-model features**, each
an extension of existing machinery (leader-approved):

1. **Deferred `keyof` node** (`TypeTag::Keyof`): `keyof <free param>` currently collapses
   to the error type (permissive — a silent-FN generator). New deferred node survives
   substitution and evaluates once its operand is concrete, routing to the SAME existing
   keyof computation used for concrete operands today (single source of truth, no new
   semantics). Unevaluable → relates conservatively (identical-node only). Touches
   repr/store/hash/intern/substitute/eval/relate/annotations/render.
2. **Modifier/value-preserving Pick** (`MappedType.modifiers_source: Option<TypeId>`,
   identity-bearing): tsc treats `{ [P in K]: T[P] }` as homomorphic-with-key-filter
   (modifiersType = T). Capture `T` from the value's `T[P]` at lowering; in
   `assemble_mapped` resolve each filtered key against T (value type + `?`/`readonly`).
3. **Recursive mapped-alias seeding** (`reserve_mapped` + mapped template on
   `TypeDecl::Alias`), mirroring the conditional-template machinery, so `DeepPartial`'s
   self-reference is a lazy node instead of the error type.

Plus the original demand fix: in `eval_mapped`/`assemble_mapped` an evaluable key source
(Instantiation/Conditional/Mapped/deferred-keyof) evaluates through the shared work-stack
first; only then the iterable-object/union checks run (the M26 no-permissive-fallback rules
unchanged). Witness: `builtin_utilities.ts` om1–om3, `intrinsics_and_recursion.ts` dp1–dp3,
leader probe t10.

### WU3 — String intrinsics (effort M)

The evaluator intercepts the four prelude intrinsic aliases by identity: literal argument →
transformed literal (Rust `to_uppercase`/`to_lowercase`; Capitalize/Uncapitalize = first
char only); union → distribute per member; boolean/number literals in holes are already
strings by construction time; anything else (patterns, `string`, free params) stays a
symbolic instantiation relating conservatively (identical-node only; → `string` allowed).
Composes with template construction (`Greet` fixture). Unicode: use Rust char-wise
uppercase; document any multi-char-mapping divergence (ß → SS vs tsc) as it arises.

### WU4 — Independent adversarial review + ratchet (effort M)

Attack: prelude shadowing (user `type Partial<T> = T` wins; no double-diagnostics), prelude
+ TK2344 spans on user code, Omit over derived interfaces/unions, DeepPartial-style
recursion depth/memo, intrinsics on patterns/`string`/empty string/unicode, intrinsic-in-
conditional/mapped composition, `run --check` audit (utility-heavy suite files may enter
scope — there is no harness gate for utilities, so movement is organic). Ratchet.

## Out of scope (explicit)

- `Parameters`/`ConstructorParameters` (rest elements — backlog `24`); `InstanceType`,
  `ThisType`, `Awaited`, `NoInfer`, `ThisParameterType`/`OmitThisParameter`.
- Full lib.d.ts loading (backlog `14`) — the prelude carries ONLY the utility slice.
- `intrinsic` keyword outside the four string intrinsics.
- tsc's TS2820 did-you-mean variant of TK2322.

## Decisions

- **Prelude as a real compilation unit** (not programmatic type synthesis): exercises the
  same lowering/binding paths, scales to backlog `14`, and keeps definitions readable.
- **Intrinsics intercepted by prelude-declared identity** — no new node kind unless forced;
  record the representation choice in the run log.

## Run log

<!-- Append as you work. -->

### 2026-07-05 — impl agent pre-implementation probe: WU2 is materially under-scoped (BLOCKER, needs leader direction)

Verified refs re-read at HEAD; oxc 0.137.0 **does** parse `= intrinsic`
(`TSType::TSIntrinsicKeyword`) — so WU3's representation can stay keyword-based, no
sentinel needed. Then ran the leader's t10-style probe plus targeted probes
(`scratchpad/probe{A..L}.ts`) against the release binary to pin what the committed
fixtures actually require. **Three substantial evaluator/type-model gaps surfaced,
two of them entirely outside the WU descriptions. `keyof`/`Pick`/`DeepPartial` do NOT
work as the sprint assumed.** Evidence:

1. **`keyof T` over a free param collapses to the error type** (not deferred). Probe:
   `type Keys<T> = keyof T; const k: Keys<P> = 123;` → **no error** (error type is
   permissive). Direct `keyof P` (concrete) works. So Omit's key set
   `Exclude<keyof T, K>` bakes `keyof T = error` at template-build time →
   `Exclude<error, K>` → the mapped defers/mis-keys. **Fix needed: a deferred `keyof`
   node** that survives substitution and evaluates once its operand is concrete (the
   "keyof-of-concrete" the WU2 bullet gestures at — but it is a *new type node*, not a
   one-line demand fix). New `TypeTag::Keyof` (operand inline in `payload`, like
   `Infer`) touches repr/store/hash/intern/substitute/eval/relation/annotations/render.

2. **Non-homomorphic `Pick`/`Omit` drop value types AND modifiers.** `{ [P in K]: T[P] }`
   is classified non-homomorphic (constraint `K`, not `keyof <src>`), so `T[P]` →
   `MappedValue` → `assemble_mapped` substitutes the **error type** for it and emits
   **required** members. Probe: `MyPick<P,"b"> = {}` → `TK2741 { b: any }` (b is
   *required any*, tsc gives `{ b?: string }`); `MyPick<P,"a"> = { a: "wrong" }` → no
   error (value is `any`, not `number`). Consequence: **fixture `om3`
   (`Omit<P,"a"> = { a: 1 }`, expects only `TK2353`) cannot pass** — typokat yields
   `{ b: any }` (required), so it emits `TK2353` (excess `a`) **plus** an extra
   `TK2741` (missing `b`) [verified: excess + missing both fire on one literal,
   probeC x1]. **Fix needed: modifier/value-preserving Pick** — tsc treats
   `{ [P in K]: T[P] }` with `K extends keyof T` as *homomorphic with an explicit key
   filter* (modifiersType = T). Concretely: add `modifiers_source: Option<TypeId>` to
   `MappedType` (identity-bearing), capture the object `T` from the value's `T[P]` at
   lowering, and in `assemble_mapped` resolve each filtered key against T (its type +
   `?`/`readonly`). Surgical to the non-homomorphic path, but a `MappedType`
   representation change (hash/eq/substitute/child_types).

3. **Recursive mapped aliases don't self-seed → `DeepPartial` is inert.** Recursive
   *conditional* aliases get `reserve_conditional`/`conditional_template`; recursive
   *mapped* aliases get nothing, so the self-reference `DeepPartial<T[K]>` inside the
   body resolves via the `resolving:true` guard to the **error type**. Probe: a
   self-recursive `DP<T> = { [K in keyof T]?: DP<T[K]> }` → `DP<Two> = { a: { d: "s" } }`
   **clean** (should be `TK2322`); the identical *non-recursive* two-alias nesting
   catches it. **Fixture `dp3` cannot pass** without this. **Fix needed: seed a reserved
   mapped-alias template** (`reserve_mapped` + a `mapped_template` on `TypeDecl::Alias`),
   mirroring the existing conditional-template machinery, so the self-reference is a
   lazy node. Confirmed a deferred mapped **target** already rejects a literal
   (probeL: `"s"` vs `{ [K in keyof number]: … }` → `TK2322`), so seeding **alone**
   fixes `dp3` — no `keyof <primitive>`/lib.d.ts modeling required.

**Net:** WU1 (prelude, decided/foundational) and WU3 (4 intrinsics via new
`IntrinsicKind` markers + `eval_instantiation` interception + evaluating template
holes for the `Greet` fixture) are as-described. **WU2 is really three type-model
features** (deferred `keyof` node + modifier-preserving Pick + recursive-mapped-alias
seeding), each an extension of the shared mapped/conditional evaluator (in the spirit
of backlog 12 "no second evaluator path"), but well beyond the "one evaluator demand
fix / effort S/M" the plan states, and each touches the interned type model. Per the
binding rules (ask before type-model/architecture changes; don't invent scope),
**paused for leader direction** before implementing 1–3. No source touched; only
scratchpad probes + this log entry. Recommended: keep the WU1/WU3 plan; expand WU2 to
the three items above (est. L, not S/M), sequence deferred-keyof → modifier-preserving
Pick → mapped-alias seeding, and re-run an adversarial pass on the deferred-`keyof` /
`modifiers_source` relation surface (regression risk: `keyof <free param>` currently
error-lenient → deferred is stricter; audit the official suite for movement).

### 2026-07-05 — leader decision: WU2 expansion APPROVED

All three items are extensions of established patterns, not architecture deviations:
deferred `keyof` mirrors the existing deferred conditional/mapped/instantiation nodes
(and removes an error-type permissive hole — the sound direction); `modifiers_source`
follows the "identity-bearing metadata folded into the hash" invariant and tsc's own
modifiersType model; `reserve_mapped` is symmetric to `reserve_conditional`. WU2 section
rewritten above (effort L). Sequencing confirmed: WU1 → WU2 (keyof → modifiers-source →
seeding) → WU3. Extra review attention ordered on the deferred-keyof/`modifiers_source`
relation surface in WU4; official-suite movement from stricter `keyof <free param>` is
expected to be sound over-reporting — audit, don't gate-lift.

### 2026-07-05 — impl agent: WU1–WU3 implemented (all gates green; audit below)

**WU1 — prelude compilation unit.** `src/prelude.ts` (include_str!), checked as a REAL
unit inside `check_program` — the driver is untouched, so every entry point (CLI, unit
tests, conformance, official suite) gets the prelude automatically. Integration
decisions:

- **Two-pass, lifetime-free handoff.** The prelude AST has its own allocator local to
  `check_program`; a scoped prelude `Pass` reserves + fills ONLY prelude decls (in the
  prelude scope, so `Omit`'s `Pick`/`Exclude` refs can never be captured by user
  shadows — resolution is eager + memoized), then the user `Pass` gets lifetime-free
  `TypeDecl::Resolved { params }` placeholders for prelude indices. No oxc-AST
  covariance gamble, no leaked allocator.
- **Scope chaining.** `bind_module_with_prelude`: one scope graph; prelude Module scope
  (parent None) → user Module scope (parent = prelude). Shadowing = ordinary
  innermost-first resolution; no duplicate-name diagnostics possible (two scopes).
  DeclId numbering (both spaces) runs prelude-first, keeping decl tables index-aligned.
- **Diagnostics filtering.** The prelude pass `debug_assert!`s clean and its
  diagnostics are dropped (trusted asset; spans would be meaningless against user
  source). Unit tests pin: empty program clean, prelude-as-user-source clean (full
  shadowing exercise), user `type Partial<T> = number` shadow wins.
- **Intrinsic representation** (Decisions §: "no new node kind unless forced"): four
  new `IntrinsicKind` variants (`Uppercase`…`Uncapitalize`) appended AFTER the keyword
  kinds (well-known ids stable) — marker ids exposed on `WellKnown`, only ever appearing
  as `InstantiationType::base`. Seeded by NAME from the prelude unit AFTER its fill (so
  `S extends string` constraints record via the normal path → `Uppercase<123>` is
  TK2344, tsc parity); the `= intrinsic` body itself never lowers (oxc parses
  `TSIntrinsicKeyword`; a USER `= intrinsic` alias degrades silently — out of scope).
  Rendering falls out: `Uppercase<T>` via the instantiation renderer.
- **Prelude divergences** (documented in `tests/cases/README.md`): `ReturnType` uses
  zero-arity `() => infer R` (rest params out of model — non-nullary functions give
  `never`, over-report) and drops the lib constraint (no TS2344 on `ReturnType<number>`
  — under-report, documented). `Record<K extends keyof any, V>`: `keyof any` stays the
  M20 error → constraint unrecorded → Record args unchecked (lenient, avoids
  over-reporting `Record<number, X>`).

**WU2 — the three approved type-model features.**

1. **Deferred `keyof`** (`TypeTag::Keyof`, operand inline in payload): lowered only for
   pending-computation operands (TypeParam / Conditional / Instantiation / Mapped /
   Template / Keyof / Infer / MappedValue); concrete objects stay eager; primitives /
   unions / arrays / tuples stay the M20 error (unchanged). Single source of truth:
   `eval::keyof_of_object` shared by the eager path and the evaluator's `BuildKeyof`
   (leader constraint 1) — pinned by a unit test asserting eager == deferred result.
   Evaluation: operand evaluates through the shared work-stack first (so
   `keyof Omit<…>` composes); error/any → error; concrete-non-object → stays deferred
   (never permissive). Relations: conservative both directions (identical-node only).
   Substitution is a pure operand rewrite (never evaluates).
2. **`modifiers_source`** on `MappedType` (identity-bearing: hash + eq + substitute +
   `child_types`, leader constraint 2): captured at lowering ONLY when the value
   template contains `X[P]` on the active mapped key with `X` a bare in-scope type
   param (the Pick shape — nothing else is captured, so no new lowering paths or
   diagnostics), and only for NON-homomorphic nodes. At assembly each literal key
   resolves against the (work-stack-evaluated — `Pick<Partial<P>, …>` composes)
   modifiers object: value replaces the placeholder, `?`/`readonly` seed the modifier
   arithmetic, `-?` strips undefined like the homomorphic path; a missing key keeps
   the M26 defaults (error value, flags absent — `Pick<P, "q">` still instantiates
   after its TK2344).
3. **Recursive mapped-alias seeding** (`reserve_mapped`/`fill_mapped` + `mapped_template`
   on `TypeDecl::Alias`, mirroring the conditional machinery faithfully — leader
   constraint 4): every alias with a top-level mapped body is seeded + filled in a new
   fill segment (with `resolving_alias` set, so the M26 `TK2456` key-source cycle
   detection is preserved), and — faithful to conditionals — **every mapped-template
   instantiation is lazy** (an `InstantiationType`). Deviation note: this makes ALL
   generic mapped aliases lazily instantiated, not just recursive ones. Verified
   behavior-equivalent: the evaluator's plain expansion runs the same `substitute`
   (including the M26 distribution guard), both pre/post forms relate conservatively
   while deferred, and the full corpus + official suite confirm (only deferred-node
   RENDERING differs, asserted nowhere).

   Plus the original demand fix, generalized: a distributive instantiation whose check
   ARG is an evaluable node (the `Exclude<keyof P, K>` shape) pre-evaluates the arg
   through the work-stack (`ExpandDistributive`) so distribution derives from its
   VALUE; template holes likewise pre-evaluate (`FinishTemplate`, never re-schedules —
   no loop on still-deferred holes), which is what makes the `Greet` composition
   collapse.

**WU3 — string intrinsics.** `eval_instantiation` intercepts marker bases
(`ApplyStringIntrinsic`): literal → Rust `to_uppercase`/`to_lowercase` /
first-char-only for Capitalize/Uncapitalize (empty string unchanged); union →
per-member (literal members transform, others stay symbolic per-member applications);
error/any → error; anything else → symbolic rebuild over the evaluated arg
(hash-consing keeps identical-node relations total). Relation rule "→ string allowed":
a symbolic-intrinsic SOURCE relates as `string` would (so unions containing string
work); nothing flows INTO it. Unicode: Rust char-wise mapping agrees with JS for the
corpus (`ß` → `"SS"` both) — no divergence observed; noted in the README.

**Cache-soundness (leader constraint 3):** all new evals run inside the existing
`SetMemo`/`in_flight`/budget discipline — exhausted/provisional results are never
durably memoized (`SetMemo` already gates on `exhausted`; cycles re-enter `in_flight` →
error, unmemoized); deferred keyof/template symbolic results stay unmemoized, matching
the deferred-conditional convention. No relation-cache changes at all.

**One post-implementation fix (official-suite regression, mine to fix per constraint
5):** `intersectionTypeInference2.ts` went fp 0→2 — `Key extends keyof T` is now
RECORDED (pre-M28 it was silently unrecorded via the error type), so `fix_params`
clamped candidates against an undecidable substituted `keyof unknown`. Fix: the same
deferred-keyof undecidability gate used for TK2344 now also gates the inference
clamp/fallback (`fix_params` treats such a parameter as unconstrained — exactly the
pre-M28 behavior). Re-run: 0 regressions.

**Gates.** `cargo test`: 203 unit (was 194; +9 new: prelude-clean/shadowing/Pick-
preservation/Omit-composition/symbolic-relations end-to-end, deferred-keyof +
intrinsics + modifiers-source evaluator units, keyof/modifiers-source no-capture
substitution units; 1 M20 unit test UPDATED — it pinned the old permissive
`keyof T = error` hole, now pins the sound TK2322) + conformance with
`m28_utility_types` flipped `true` — all green, no existing fixture perturbed.
`cargo clippy --all-targets -- -D warnings` clean. `cargo build --release` +
`tsofficial.py run --check` exit 0.

**Official-suite audit (every state change; no --save, scoreboard untouched):**

| file | before → after | cause |
|---|---|---|
| types/intersection/intersectionTypeInference2.ts | IN 0fp → (transient 2fp) → **0fp** | recorded `keyof T` constraint clamping in inference; FIXED (gate extended) |
| controlFlow/controlFlowComputedPropertyNames.ts | OOS:unresolved → IN, 0 exp / 0 fp | `Record` now resolves (was TK2304-OOS); clean gain |
| types/keyof/keyofIntersection.ts | OOS:unresolved → IN, 0 exp / 0 fp | utility names resolve; clean gain |
| expressions/typeGuards/typeGuardOfFormTypeOfFunction.ts | OOS:unresolved → IN, 0 exp / 2 fp (79:2322, 84:2339) | `Record` resolves; `Record<keyof S,…>`/`Record<string,any>` stay deferred (K-in-string index production is the documented M26 deferral) → typeof-narrowing keeps the deferred member (over-report) and member access on the deferred node is TK2339 (over-report) |
| types/literal/stringLiteralsAssignedToStringMappings.ts | OOS:unresolved → IN, **3/3 expected matched**, 2 fp (4, 12) | `Uppercase`/`Lowercase` resolve; fps = literals flowing INTO a symbolic nested mapping (`x = "A"` where tsc computes the uppercase-closure) — the documented conservative symbolic rule |
| types/literal/stringMappingDeferralInConditionalTypes.ts | OOS:unresolved → IN, 0 exp / 1 fp (13) | intrinsics resolve; the fp's check type involves `S & string` — intersections are out of the type model (backlog 25), so the extends test stays conservative → false branch (over-report) |
| types/literal/stringMappingOverPatternLiterals.ts | OOS:unresolved → IN, **29/29 expected matched**, 26 fp | intrinsics resolve; fps = tsc's pattern algebra over string mappings (`Uppercase<\`${string}px\`>` relations), out of scope — symbolic relates to `string` only (over-report, safe) |

All movement is organic gained coverage in the sound direction (expected diagnostics
matched where present; over-reports only), per the sprint's "no harness gate for
utilities" note — left for the leader/WU4 to assess and ratchet. `tests/cases/README.md`
utility-types bullet rewritten to shipped state + full divergence list; the stale M20
"generic keyof deferred" and M27 "intrinsics are backlog 12" cross-references updated.

### 2026-07-05 — impl agent, round 2: review FAIL fixed (conditional operand demand-eval)

**Root cause confirmed** exactly as the reviewer pinned: `eval_conditional` gated only
on `is_concrete(cond.check)` and `run_extends_test` related the RAW substituted
operands — a concrete-argument `Uppercase<"ABC">` (Instantiation) or `keyof {a:1}`
(Keyof) passed the concreteness gate unevaluated, the extends relation saw the
symbolic node → conservative `No` → false branch forced. Reproduced pre-fix: all four
true-branch fixture groups (iu1/nm1/h1/kc1) mis-branched — an FP on the correct
assignment AND the marked true-branch error silently dropped (HIGH-1/HIGH-2). My WU2
demand-eval hooks covered instantiation args / mapped sources / template holes but
not conditional check/extends operands.

**Fix (diff summary — `src/check/checker/eval.rs` only):**

- `Task::DecideConditional(TypeId)` — new work-stack task.
- `eval_conditional`: after the in-flight/budget checks, if either operand
  `arg_needs_pre_eval`s (Conditional/Instantiation/Mapped/Template/Keyof, or a union
  containing one) → schedule `[SetMemo, DecideConditional, Eval(check),
  Eval(extends)]` instead of testing raw operands. Same memo/in-flight/budget
  discipline as every other hook; both the infer and non-infer paths are covered
  because `run_extends_test` now receives the rebuilt (`Copy`, no re-interning)
  `ConditionalType` with EVALUATED operands.
- `decide_conditional`: pops both values, runs the shared extends test, then the
  **no-false-on-undecidable rule**: `Yes` is always sound → true branch; a `No` where
  either evaluated operand still carries an unevaluable deferred node
  (`operand_undecidable`: Conditional/Instantiation/Mapped/Keyof at top level or as a
  union member) does NOT pick the false branch — the conditional stays its own
  deferred value (conservative relations; SetMemo commits ty→ty, idempotent).
  **Template patterns are deliberately excluded from the gate** — the M27
  anchored-matching model decides them, and gating them would regress the
  pattern-extends fixtures (recorded deviation from a maximal reading of the rule;
  the evaluated-operand pre-pass still applies to them, which is what makes
  intrinsic-in-template-hole compositions test correctly).

The distribution path is covered for free: `Has<T, K>` is distributive on K, and each
per-member conditional re-enters `eval_conditional`, where the extends (`keyof T`)
now pre-evaluates.

**Spec:** `conditional_positions.ts` (c898c77) — all 16 cases pass, exactly the 8
marked lines error (verified on the release binary; conformance harness green).

**MED docs:** three reviewer-verified over-report divergences added to the
`tests/cases/README.md` utility bullet in the established format — Pick/Omit deferred
over union operands + `K = never` (verified: `Pick<A|B, never>` renders deferred and
rejects `{}`); Record softened to "literal-union key sets only" (template-literal
keys defer); DeepPartial-over-primitive-leaves referencing the existing no-lib
`keyof <primitive>` M20 gap. Plus the new no-false-on-undecidable rule itself.

**Gates:** `cargo test` green (203 unit + conformance incl. the new fixture; zero
existing tests perturbed), `clippy --all-targets -- -D warnings` clean, release
build + `tsofficial.py run --check` **exit 0, regressions 0, progress 6** (unchanged
set). Full per-file diff vs the committed scoreboard re-enumerated: exactly the six
round-1 OOS→IN files and nothing else; round 2's only suite movement is
`stringMappingDeferralInConditionalTypes.ts` fp 1 → **0** (file now fully clean;
strict fp total 35→34): its check involves `S & string` (intersections out of model →
error-typed operand), and the evaluated-operand path now takes the M22 any-like true
branch instead of the raw-node false branch. No fp/fn moved anywhere else.

### 2026-07-05 — impl agent, round 3: deep undecidable walk + TK2344 argument gate + alias-name rendering

**HIGH — root cause confirmed:** `operand_undecidable` (round 2) checked only the
top-level tag + union members, AND the whole gate lived only on the pre-eval path
(`decide_conditional`) — a nested shape like `{ v: keyof T } extends { v: "a" }` has
top-level tag Object, so `arg_needs_pre_eval` never fired, the DIRECT path related raw
operands, the relation returned a conservative `No` *through* the nested Keyof, and
the shallow gate wasn't even consulted → false branch → silent FN. Reproduced
pre-fix: `WK<{a:1}> = "n"` clean; the amended fixture erred on exactly ONE line of
each nested pair (45/50/56/60/66).

**Fix (diff summary):**

- `eval.rs eval_conditional`: the direct (non-pre-eval) path now applies the same
  matched/undecidable/false decision as `decide_conditional` — `Yes` → true branch;
  unmatched + `operand_undecidable(check|extends)` → stay deferred; else false branch.
  Deep PRE-evaluation deliberately NOT added, per the leader arbitration (tsc's mixed
  eager-false behavior; backlog 36).
- `eval.rs operand_undecidable`: deepened into a structural walk (visited-set,
  iterative) — returns true on a Conditional / Instantiation / Mapped / Keyof at any
  depth through object property types, index-signature values, call/construct
  signatures, function params + returns, tuple/array elements, and union members.
  Template patterns remain excluded (round-2 recorded deviation — the M27 matching
  model decides them); intrinsics/literals/type-params/infer/mapped-value are
  decidable leaves. Generalized from the `contains_deferred_keyof` walker shape as
  directed.
- Result: all 18 marked fixture lines fire (8 original + 10 nested — BOTH lines of
  every nested pair), zero FN across the arbitration table; the five tsc-clean lines
  are the documented over-reports.

**LOW-2:** (a) the TK2344 gate's ARGUMENT side now also skips on a deferred
conditional / lazy instantiation (`contains_deferred_argument`, sharing one walker
skeleton `contains_nodes` with the keyof gate; the CONSTRAINT side stays keyof-only,
preserving pre-M28 conditional/mapped-constraint behavior) — `type UE<K> =
Uppercase<Extract<K, string>>;` no longer errors at declaration (tsc-defer parity).
(b) Root cause of the raw-body rendering: intrinsic markers name themselves via
`IntrinsicKind::display_name`, but reserved conditional/mapped template rows carried
NO name. Added a rendering-only `template_names` side column on the `Store` (set at
reserve time in `reserve_type_decls`, never part of identity — reserved rows are
nominal and un-deduped anyway); the Instantiation renderer prefers it. Verified:
`Type 'Extract<K, string>' is not assignable to type 'number'` (and `DP<T[K]>` inside
deferred mapped renders too). Note: an already-SUBSTITUTED conditional (the eager
per-member expansion of a distributive template) is not an Instantiation-of-alias
node and still renders structurally — out of the directive's scope, provenance
through substitution would be a new mechanism.

**LOW-3:** README divergence entry rewritten to match the implementation exactly
(deep walk with the position list, template-pattern exclusion, both TK2344 gate
sides); added the nested-operand divergence line (five tsc-clean over-reports, the
arbitration summary, pointer to backlog 36).

**Gates:** `cargo test` green (203 unit + conformance incl. the amended fixture — all
18 marked lines, no existing test perturbed); `clippy --all-targets -- -D warnings`
clean; release build; `tsofficial.py run --check` **exit 0, regressions 0, progress 6
(unchanged)**. Per-file diff vs the committed scoreboard re-enumerated: still exactly
the six round-1 OOS→IN files with IDENTICAL numbers — the deep gate produced **zero
suite movement** (no suite conditional newly deferred: the shapes it affects — nested
deferred nodes in operands with a No verdict — evidently don't occur in the in-scope
set beyond what already deferred).

### 2026-07-05 — impl agent, round 4: TK2344 argument side — evaluate-first replaces the round-3 gate (direction reversal)

**Why the reversal:** the round-3 LOW-2a directive ("gate arguments carrying deferred
conditionals/instantiations — tsc defers these") was disproven by the leader's own tsc
probe (m28_arb3.ts): tsc DOES error TS2344 at the declaration on decidable argument
compositions (`Pick<P, Exclude<"a" | 1, "a">>`) AND on unprovable deferred arguments
(`Uppercase<MyExclude<K, "a">>`); it stays clean only on PROVABLE shapes
(`Extract<K, string>` — its constraint approximation). The round-3 blanket gate was
therefore an FN generator in both subclasses — confirmed pre-fix: the new fixture's
four TK2344 markers (B1/A1/S1/UE) were all silent, only mo2's TK2353 fired.

**Fix (diff summary):**

- `calls.rs check_type_argument_constraints`: the argument-side
  `contains_deferred_argument` gate is REMOVED. The argument now ALWAYS evaluates
  through the shared evaluator (`evaluate_type` — memo/in-flight/budget discipline as
  everywhere), and the check ALWAYS runs on the evaluated form: concrete result →
  precise check; still-deferred result → the conservative relation verdict stands.
  The CONSTRAINT-side keyof gate is untouched (round-4 directive §2) — verified: the
  prelude assert-clean unit test and every-run debug_assert still pass (the prelude
  Omit body's Pick check is skipped by the constraint gate before any argument
  handling), and the fixture's MyOmit block is clean at declaration with mo2's
  TK2353 at use. All three call sites (decls.rs instantiate_type_reference,
  calls.rs explicit-generic-call + new-expression) route through this one function,
  so the evaluate-first path covers them uniformly (directive §3); value-annotation
  sites are unchanged (their args were already pre-evaluated — evaluation is
  memoized and idempotent).
- `eval.rs contains_deferred_argument`: repurposed as a **message-form chooser only**
  (doc rewritten): a failing argument whose EVALUATED form is still deferred renders
  by its WRITTEN form — which carries the alias name via the round-3 `template_names`
  column — so the surviving conservative TK2344 on `UE<K>` renders exactly
  `Type 'Extract<K, string>' does not satisfy the constraint 'string'.` (verified);
  a fully-evaluated argument renders as its value (`1` — tsc-like). Bonus tsc-parity:
  S1's elaboration derives from the evaluated conditional's branch relation —
  "Type 'K' is not assignable to type 'string'", tsc's exact wording.

**Acceptance (`constraint_arguments.ts`, 3ffb601):** errors on exactly the five
marked lines — B1 (11), A1 (13), S1 (18), UE (22) TK2344; mo2 (29) TK2353 — and
B2/A2/MyOmit-declaration/mo1 clean. Conformance harness green.

**README (directive §5):** the round-3 entry rewritten — the argument side now reads
"evaluate first, then always check: decidable and unprovable shapes tsc-exact;
over-report ONLY on provable shapes (tsc constraint approximation, backlog 37)";
the probe-disproven "tsc defers these" wording is kept ONLY for the constraint-side
keyof gate, where it remains accurate (the Omit idiom's check does land at concrete
instantiation).

**Gates:** `cargo test` green (203 unit + conformance incl. constraint_arguments.ts);
`clippy --all-targets -- -D warnings` clean; release build; `tsofficial.py run
--check` **exit 0, regressions 0, progress 6 (unchanged)**. Per-file diff vs the
committed scoreboard: still exactly the six round-1 OOS→IN files with identical
numbers — evaluate-first argument checking produced **zero suite movement** (no
in-scope file has a constrained utility-composition argument site that flips either
way).
