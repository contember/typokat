> **OUTCOME - shipped 2026-07-09.** Shipped M32 signature shape: identity-bearing
> optional/default/rest parameters and tuple rest, syntax lowering, call/new/super
> arity, relation/inference parity, and rest-based `ReturnType`. Commit map: sprint
> plan -> `b224058`, WU1 -> `03c0e45`, WU2 -> `3883807`, WU3 -> `0e3c746`,
> WU4 -> `94208a5`, review fix -> `dc30516`, WU5 ratchet/docs -> final ratchet
> commit. Verification: `cargo test` (217 unit tests + 183-file conformance corpus,
> 684 expected diagnostics), `cargo clippy --all-targets -- -D warnings`,
> `git diff --check`, `tsc 6.0.3 --strict --noEmit` per M32 fixture, and
> official-suite `run --check` after an audited scoreboard ratchet (874 corpus,
> 507 in-scope, 0 regressions). Backlog closed: `24`, `39`. Deferred: utility alias
> constraints remain backlog `67`; embedded tuple-rest inference and variadic source
> tuple infer parity are filed as backlog `69`; overloads/generic methods/namespaces
> remain on track A.

# Sprint - signature shape: rest + optional parameters (2026-07-09)

**Goal.** Ship backlog `24` + `39` as M32: represent function rest parameters,
optional/default parameters, and tuple rest elements without silently permissive
fallbacks or spurious fixed-arity diagnostics.

**Theme.** This is the model-completeness signature migration on the `lib.d.ts`
critical path. Rest and optional/default parameters share the same representation,
hashing, call-arity, relation, substitution, rendering, and inference surfaces, so
they should land as one coordinated shape change rather than two incompatible
half-migrations.

## Refs re-verified at HEAD (2026-07-09)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `ParameterType` already has an `optional` flag, but current lowerers write
  `optional: false` everywhere and there is no rest marker or default/required
  arity split - `src/types/repr.rs:349`, `src/check/checker/calls.rs:754`,
  `src/check/checker/annotations/functions.rs:41`,
  `src/check/checker/annotations/signatures.rs:92`.
- ✔ `FunctionType` stores only `params` + `ret`; rest-ness and required/total
  arity must become identity-bearing through the same hash-consed node -
  `src/types/repr.rs:404`, `src/types/hash.rs:167`,
  `src/types/intern/composites.rs:70`.
- ✔ Strict signature lowering explicitly rejects rest parameters, optional
  parameters, and default initializers today - `src/check/checker/annotations/functions.rs:226`.
- ✔ Tuple annotations are fixed-position only; tuple rest/optional/named elements
  abort lowering via `as_ts_type()?`, and readonly tuple lowering repeats the same
  fixed-element assumption - `src/check/checker/annotations/composites.rs:28`,
  `src/check/checker/annotations/composites.rs:185`.
- ✔ Call checking uses exact arity (`arg_types.len() != param_types.len()`) and
  reports only `Expected N arguments, but got M`; there is no min/max/rest arity
  model or range wording - `src/check/checker/calls.rs:333`,
  `src/diagnostics/mod.rs:438`.
- ✔ Function relation still treats `src.params.len() > tgt.params.len()` as the
  only arity failure, with comments assuming no optional/rest parameters -
  `src/relate/relation/objects.rs:300`.
- ✔ Tuple relation still requires equal lengths and has no rest-tail rule -
  `src/relate/relation/collections.rs:36`.
- ✔ Substitution and conditional-infer freshening rewrite parameter and tuple
  element types but have no rest slot to carry - `src/types/substitute/apply.rs:118`,
  `src/check/checker/eval/instantiation.rs:313`,
  `src/check/checker/eval/instantiation.rs:385`.
- ✔ Inference recurses over function params and tuple elements positionally only;
  rest capture (`(...args: infer P)` / `[H, ...infer R]`) is not modeled -
  `src/check/infer/context.rs:141`, `src/check/infer/context.rs:396`.
- ✔ The prelude still documents and uses the zero-arity `ReturnType` workaround
  because rest parameters are missing - `src/prelude.ts:5`,
  `src/prelude.ts:22`; `docs/reference/divergences.md:189`.

## Work units

### WU1 - Disabled M32 corpus for signature shape (effort M)

- **Problem.** Backlog `24` and `39` have prose probes but no acceptance corpus, and
  the current M25/M28 fixtures deliberately avoid rest-based idioms.
- **Verify first.** Cross-check every new fixture with `tsc 6.0.3 --strict --noEmit`,
  especially arity wording for optional/default/rest calls and relation parity for
  function assignment.
- **Scope.** Add `tests/cases/m32_signature_shape/` disabled in `MILESTONE_DIRS`.
  Cover: optional parameters, defaulted parameters, function rest params, rest call
  arity, tuple rest assignability, readonly tuple rest controls, conditional
  rest-infer patterns (`[H, ...R]`, `(...args: infer P) => R`), and utility/prelude
  witnesses that currently need the zero-arity `ReturnType` workaround.
- **Acceptance / witness.** `cargo test` is behavior-neutral while disabled;
  enabling the directory at HEAD fails on the pinned FP/FN families.
- **Touch points.** `tests/cases/m32_signature_shape/**`, `tests/conformance.rs`,
  `tests/cases/README.md`, focused `tsc` probe files in scratch as needed.

### WU2 - Signature-shape representation migration (effort L)

- **Problem.** Function and tuple shape metadata that affects assignability is not
  represented, so unsupported syntax can only degrade or be mis-modeled as fixed
  required parameters/elements.
- **Verify first.** Unit-probe hash-consing before behavior work: two signatures
  that differ only by optional/rest/default shape must not collide; identical shapes
  must dedup.
- **Scope.** Extend the type model with identity-bearing signature shape:
  `ParameterType` records required vs optional and rest where needed; `FunctionType`
  exposes required count, total fixed count, and optional rest element semantics via
  helpers rather than ad hoc length math; `TupleType` records ordered fixed elements
  plus a distinct rest element shape sufficient for terminal and middle rest forms
  such as `[A, ...B[]]` and `[...T, X]`. Carry the new fields through
  `StructuralKey::Function`, tuple hashing, intern lookup equality,
  `function_params_eq`, substitution, conditional-infer substitution, render
  helpers, and unit tests.
- **Acceptance / witness.** New unit tests show structural identity includes
  optional/rest tuple and function shape; substitution over all new slots
  re-interns only when a child changes; `cargo test` stays green with the corpus
  still disabled.
- **Touch points.** `src/types/repr.rs`, `src/types/hash.rs`,
  `src/types/intern/composites.rs`, `src/types/store.rs`,
  `src/types/substitute/apply.rs`, `src/check/checker/eval/instantiation.rs`,
  `src/diagnostics/render_type.rs`, `src/types/intern/tests.rs`,
  `src/types/substitute/tests.rs`.

### WU3 - Lowering + call/new/super arity semantics (effort L)

- **Problem.** The checker rejects signature syntax before it reaches the type model,
  then call checking assumes exact arity even when TypeScript allows omitted optional
  or variadic rest arguments.
- **Verify first.** Re-run probes for `f(1)` with `b?: string`, `b = "x"`, too-few
  required args, too-many fixed args, rest args of the wrong type, and constructor /
  `super` calls. Confirm the exact `TK2554` / `TK2345` split against tsc.
- **Scope.** Lower optional and defaulted parameters in free functions, arrows,
  class methods/constructors, function-type annotations, call signatures, construct
  signatures, and object/interface method signatures. Mark defaulted params optional
  while still checking the initializer against the declared parameter type. Lower
  function rest parameters and tuple rest elements, including readonly tuple syntax.
  Replace exact call arity with min/max/rest checks and range-aware diagnostics,
  then thread the selected parameter/rest element target into the existing
  contextual-literal final obligation path for calls, `new`, and `super`.
- **Acceptance / witness.** M32 optional/default/rest call fixtures pass; fixed-arity
  M3/M11/F1 fixtures retain their existing `TK2554` messages where no range is
  involved; rest argument mismatches produce `TK2345` on the bad argument instead of
  being hidden by arity.
- **Touch points.** `src/check/checker/annotations/functions.rs`,
  `src/check/checker/annotations/composites.rs`,
  `src/check/checker/annotations/signatures.rs`,
  `src/check/checker/calls.rs`, `src/check/checker/classes/mod.rs`,
  `src/check/checker/decls/interface.rs`, `src/diagnostics/mod.rs`,
  `tests/cases/m3_functions/`, `tests/cases/m11_classes/`,
  `tests/cases/f1_object_interface_call/`,
  `tests/cases/f1_object_interface_construct/`.

### WU4 - Relation + inference parity for signature rest shape (effort L)

- **Problem.** Assignability and inference still compare function and tuple shapes by
  fixed positional prefixes only, which either drops errors or over-reports once the
  syntax lowerers accept rest/optional shapes.
- **Verify first.** Build an adversarial matrix for function assignment:
  required-to-optional, optional-to-required, rest-to-fixed, fixed-to-rest,
  empty-rest, wrong rest element, tuple rest source/target, and `infer` capture from
  tuple/function rest positions. Cross-check with tsc under `--strictFunctionTypes`.
- **Scope.** Update function relation to use required-count and rest-aware target
  supply rules while preserving typokat's existing contravariant parameter policy.
  Update tuple relation for rest tails and tuple-to-array covariance. Extend
  inference to collect candidates from rest parameter/tuple positions, including
  `(...args: infer P)` and `[Head, ...infer Rest]` patterns, without reopening
  backlog `65`'s multi-argument candidate policy.
- **Acceptance / witness.** M32 relation and inference fixtures pass; existing M10,
  M18, M25, and b57 tuple-array inference coverage stays green; no relation-cache
  invariant is weakened.
- **Touch points.** `src/relate/relation/objects.rs`,
  `src/relate/relation/collections.rs`, `src/check/infer/context.rs`,
  `src/check/infer/helpers.rs`, `src/check/checker/eval/extends.rs`,
  `src/relate/relation/tests.rs`, `src/check/infer/tests.rs`.

### WU5 - Prelude, divergence cleanup, independent review, and ratchet (effort M)

- **Problem.** The prelude and docs still encode the rest-parameter workaround, and
  this migration is exactly the kind of representation change that can create silent
  false negatives if reviewed only locally.
- **Verify first.** Re-run the M28 utility corpus and focused `ReturnType` probes
  before changing the prelude; run official-suite `run --check` after the corpus is
  enabled because rest/optional signatures are common in conformance baselines.
- **Scope.** Change `ReturnType` to the ordinary rest-parameter conditional shape,
  and add `Parameters` / `ConstructorParameters` only if the implemented rest tuple
  shape supports them without pulling in overloads or constructor overload sets.
  Update `docs/reference/divergences.md`, `README.md`, `tests/cases/README.md`,
  and `docs/INDEX.md`; then run the independent adversarial review required by the
  dev method, focused on dropped errors in arity/relation/inference and false
  accepts from unsupported tuple rest forms.
- **Acceptance / witness.** `m32_signature_shape` is enabled; `cargo test`,
  `cargo clippy --all-targets -- -D warnings`, focused typokat/tsc probes, and
  official-suite `run --check` are green or have an audited safe-direction ratchet.
- **Touch points.** `src/prelude.ts`, `docs/reference/divergences.md`, `README.md`,
  `tests/cases/README.md`, `docs/INDEX.md`, `tooling/official-suite/`, backlog
  `24`, backlog `39`, possibly backlog `67` if the `ReturnType` constraint story
  changes.

## Out of scope (explicit)

- Function overload declarations/resolution (`40`) and `TK2769`.
- Generic methods / method-level type parameters (`41`, subsumes `23`).
- Full `lib.d.ts` loading (`14`) and module-resolver breadth (`15`).
- Optional tuple elements (`[A?]`) and named tuple element display unless they are
  unavoidable for the rest-element parser shape. Do not silently accept them as
  required elements.
- Required-after-optional parameter syntax diagnostics (`TS1016` family). This
  sprint should model signature shape for already-accepted ASTs, not expand the
  parser/syntax-diagnostic surface.
- Spread call expressions (`f(...xs)`) unless a small, well-reviewed slice is needed
  to prove rest parameters; ordinary positional calls to rest signatures are enough.
- Backlog `65` multi-argument candidate priority/fix-then-check policy. Rest
  inference may collect candidates, but it must not rework candidate fixing broadly.
- Utility alias constraint enforcement (`67`) unless the rest migration makes the
  correct fix trivial and reviewable.

## Decisions

- Land `24` and `39` together as M32 because they share the `FunctionType` identity
  and call/relation arity model.
- Use one corpus directory, `m32_signature_shape/`, so rest and optional/default
  witnesses can exercise the combined representation.
- Keep soundness-first behavior for unsupported tuple shapes: over-report or degrade
  with diagnostics where possible, but do not accept a tuple/rest form as a looser
  fixed tuple.
- Preserve typokat's stricter function-parameter contravariance policy; this sprint
  changes arity/rest supply, not the variance decision.

## Sequencing

| Order | WU | Rationale |
|---|---|---|
| 1 | WU1 corpus | The fixture corpus is the acceptance spec and must be committed first. |
| 2 | WU2 representation | Every later step depends on the identity-bearing shape being stable. |
| 3 | WU3 lowering/calls | Once shapes exist, accept syntax and check runtime call sites. |
| 4 | WU4 relation/inference | Then wire assignability and type-level extraction through the same shape. |
| 5 | WU5 review/docs/ratchet | Close only after adversarial review and official-suite audit. |

Parallelism: WU1 fixture authoring can split by topic, but WU2-WU4 should run in
one implementation context because the same representation invariants thread through
all of them. The independent review must be a different agent after WU2-WU4 are
implemented and the M32 corpus is enabled.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* -> ../decisions/NNNN ; new future work -> ../backlog/NN ;
     transient -> leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("-> ADR-0007"). -->
