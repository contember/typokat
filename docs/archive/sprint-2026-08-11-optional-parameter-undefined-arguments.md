> **OUTCOME — shipped 2026-08-11.** Optional and defaulted fixed parameters now admit direct
> `undefined` and matching undefined-bearing arguments through one shared call-only descriptor.
> Stored parameter types, required/rest targets, contextual typing, overload diagnostics, and
> body-local parameter meaning remain unchanged. Generic inference treats exact `undefined` at an
> optional fixed slot as per-binder fallback evidence and preserves the reviewed non-fresh
> singleton candidate when that fallback is discarded. Commit map: plan → `281c27b`; WU0 corpus
> and inference falsifiers → `7eb20df`, `5857cea`, `8b595d2`, `fa77026`, `1239a6c`, `2c6241e`;
> WU1 → `2be673b`; WU2 differential matrix and gate → `2c84db0`, `ab02d3d`.
>
> The full WU2 gate passed workspace tests, all-target clippy with warnings denied, formatting, and
> the 874-file official-suite ratchet with zero regressions. The differential harness passed 60/60
> unit tests, the exact 5/5 tsc matrix, all five pre-change falsifiers, committed repros, and seeds
> 1/2/3 × 400 with 1,200/1,200 matches against exact pre-change `281c27b`. The immutable WU3
> re-screen preserved 9 roots, 9/0/0 file accounting, 13 resolutions, and zero notices/parse errors;
> `src/core/generator.ts:36:48 TK2345` disappeared, reducing diagnostics from 6 to 5 with no new
> identity. Independent WU1, WU2-matrix, and WU3 reviews returned **PASS**.
>
> Backlog `109` is closed. Backlog `72` remains open at 7 incomplete records and 5 diagnostics;
> target/library meaning equivalence is still unproved, so no mutation, witness, or CI work started.
> Review probes graduated the independent contextual-return literal gap to backlog `110` and the
> fresh structural inference provenance under-report to backlog `111`. No performance result is
> claimed.

# Sprint — optional parameter `undefined` arguments (2026-08-11)

**Goal.** Accept explicit `undefined`-bearing arguments at optional and defaulted fixed parameters
without widening stored parameter types or weakening required-parameter diagnostics, then re-screen
the immutable `placetext` project to establish its next general blocker.

**Theme.** Backlog `109` is the smallest general blocker exposed after object-binding publication.
The signature model and arity are already correct; one call-boundary relation rule is missing. This
sprint closes that narrow rule across every shared call consumer and keeps backlog `72`'s zero-clean
witness threshold unchanged.

## Refs re-verified at HEAD (2026-08-11)

Planning HEAD is `c2941bcb2316e20844abf2453dc34be20ae896e3`.
`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `ParameterType` already carries identity-bearing `optional` and `has_default` flags. Both
  optional and defaulted fixed parameters set `optional: true`; required and rest parameters do
  not — `crates/typokat-types/src/types/repr.rs:517`,
  `crates/typokat-types/src/types/repr.rs:542`,
  `crates/typokat-types/src/types/repr.rs:553`.
- ✔ Required/min/max call arity already consumes that shape correctly; omission is not the bug —
  `crates/typokat-check/src/check/checker/calls.rs:2887`,
  `crates/typokat-check/src/check/checker/calls.rs:2925`.
- ⚠ Fixed call target selection discards the optional flag and returns raw `param.ty` in the
  no-rest, represented-rest, fallback-rest, and union-rest-mismatch paths —
  `crates/typokat-check/src/check/checker/calls.rs:2624`,
  `crates/typokat-check/src/check/checker/calls.rs:2988`,
  `crates/typokat-check/src/check/checker/calls.rs:2998`,
  `crates/typokat-check/src/check/checker/calls.rs:3018`,
  `crates/typokat-check/src/check/checker/calls.rs:3034`.
- ✔ The same target machinery feeds contextual generic inference, overload candidate trials,
  compatible rest alternatives, committed calls, `new`, and `super`; the fix must be shared rather
  than patched at the `placetext` call site —
  `crates/typokat-check/src/check/checker/calls.rs:895`,
  `crates/typokat-check/src/check/checker/calls.rs:2410`,
  `crates/typokat-check/src/check/checker/calls.rs:2602`,
  `crates/typokat-check/src/check/checker/calls.rs:2738`.
- ⚠ Existing M32 fixtures prove omission, arity, and wrong non-undefined arguments, but never pass
  explicit `undefined` or a `T | undefined` value —
  `tests/cases/m32_signature_shape/optional_default_params.ts:4`,
  `tests/cases/m32_signature_shape/construct_signatures.ts:4`.
- ⚠ A fresh strict-6.0.3 probe accepts six rows that current typokat rejects: direct optional
  `undefined`, forwarded `number | undefined`, a defaulted parameter, constructor, method, and
  overload. Typokat emits five `TK2345` plus one `TK2769`; both checkers still reject the five
  wrong-string/boolean controls. A mixed `string | undefined` failure also proves that blindly
  rendering every optional target as `T | undefined` would change diagnostic shape.
- ✔ The immutable WU3 evidence retains the exact next failure at
  `src/core/generator.ts:36:48 TK2345`, with 9 roots, 9/0/0 file accounting, 13 resolutions, zero
  notices/parse errors, 7 incomplete records, and 6 diagnostics —
  `docs/archive/real-project-rescreen-2026-08-10/README.md`.

## Work units

### WU0 — commit the disabled RED call-boundary corpus (effort S)

- **Problem.** The current corpus cannot distinguish correct optional arity from correct explicit-
  `undefined` acceptance, and the immutable project is too broad to serve as the only semantic spec.
- **Verify first.** Cross-check every row with pinned `tsc 6.0.3 --strict --noEmit`. Record exact
  pre-change typokat identities and prove each clean acceptance row fails for the raw-target reason.
- **Scope.** Add disabled `tests/cases/b109_optional_parameter_undefined_argument/` fixtures for
  free functions, defaulted parameters, methods, constructors, `super`, object call/construct
  signatures, overload selection, explicit generic arguments, inferred generic controls, and fixed
  prefixes before rest. Cover direct `undefined`, forwarded `T | undefined`, parentheses, and
  declaration/body-local type controls. Pin required parameters, `null`, wrong scalar values,
  invalid union members, arity, and rest arguments as negative controls. Keep fresh object/array/
  callback contextual checks represented so the fix cannot erase their existing obligations.
- **Acceptance / witness.** The disabled corpus is a behavior-neutral spec commit. Its tsc oracle
  has no diagnostics on the admitted optional/defaulted rows and exact markers on every invalid
  control. A forced isolated run is RED at current HEAD on all six direct consumer families; the
  invalid controls already pass and must not be rebaselined away.
- **Touch points.** `tests/cases/b109_optional_parameter_undefined_argument/`,
  `tests/conformance.rs`, `tests/cases/README.md`, and `docs/reference/divergences.md` only if the
  oracle establishes a diagnostic-display difference that the sprint deliberately retains.

### WU1 — implement one call-only effective acceptance rule (effort S)

- **Problem.** `Vec<Option<TypeId>>` target alternatives retain only the declared type, so later
  trial/commit code cannot tell whether a fixed slot accepts `undefined`.
- **Verify first.** Trace one optional row through raw argument inference, contextual rewalk,
  overload trial, committed obligation, and diagnostic rendering. If preserving deferred-obligation
  ordering requires a global relation or type-model change, stop and ask before changing that
  boundary.
- **Scope.** Carry call-only fixed-slot metadata through the shared target-alternative machinery.
  Treat `undefined` as an admitted constituent only when the selected fixed parameter is optional
  (including defaulted). Apply the same rule to candidate trials and committed call/`new`/`super`
  checks, including fixed prefixes around rest alternatives. Preserve the declared `param.ty` for
  parameter identity, body-local binding, contextual literal shape, relation/signature semantics,
  and wrong-value diagnostic rendering. Keep required and rest slots strict. Reuse the deferred
  obligation/effect pipeline; do not emit eagerly or bypass exhaustion/replay ownership.
- **Acceptance / witness.** Enable WU0. All acceptance rows become clean; overloads select the
  optional candidate; invalid rows retain their exact code/span and existing direct-scalar target
  text; defaulted bodies remain non-undefined while optional bodies retain their declared
  undefined-bearing view. No required/rest argument becomes permissive and no contextual argument
  diagnostic disappears or duplicates.
- **Touch points.** Primarily `crates/typokat-check/src/check/checker/calls.rs`; narrowly
  `crates/typokat-check/src/check/checker/context.rs`, `checker/mod.rs`, or
  `checker/statements.rs` only if WU0 proves that relation acceptance and diagnostic rendering need
  separate obligation metadata. No `typokat-types` representation change is expected.

### WU2 — independent adversarial review and semantic gates (effort M)

- **Problem.** This shared seam controls contextual inference, overload selection, committed
  diagnostics, constructors, `super`, and rest alternatives; a six-row green corpus cannot exclude
  a silent false negative in another consumer.
- **Verify first.** A different subagent reviews WU0–WU1 without the implementer's conclusions and
  cross-checks adversarial probes against pinned tsc 6.0.3.
- **Scope.** Hunt accidental acceptance at required/rest parameters, defaulted-body widening,
  overload mis-selection, invalid-member loss from unions, `null` acceptance, fresh-literal or
  callback contextual weakening, duplicate/provisional diagnostics, eager relation effects, error/
  `any` recovery drift, and call/`new`/`super` inconsistency. Inspect every raw `param.ty` fixed-slot
  path rather than allowlisting one bypass.
- **Acceptance / witness.** Independent review returns no HIGH or MEDIUM finding. Run focused tests,
  full workspace tests, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the
  874-file official-suite ratchet. Because the target machinery feeds contextual inference, run the
  differential unit/repro gates and seeds 1/2/3 at 400 cases. Add an exact optional-parameter matrix
  against tsc and pre-change HEAD if the randomized grammar cannot emit this syntax; a generator
  that never reaches the changed rule is not evidence. Triage every official or differential delta
  before closure.
- **Touch points.** Review and gates; `tooling/differential/` changes only if a general optional-
  parameter matrix cannot be expressed by the existing focused harness.

### WU3 — immutable `placetext` re-screen and close (effort S)

- **Problem.** Removing `TK2345` may reveal a later diagnostic or route drift hidden by the old
  call failure, and backlog `72` still requires an independently reviewed immutable result.
- **Verify first.** Build a fresh release binary from reviewed HEAD and record its SHA-256 plus the
  production library route attestation.
- **Scope.** Repeat the exact detached `faf233107146ceca63bf8a6fec8f07ad43ab17e2` checkout,
  license/lock/config/source hashes, native and overlay pinned-tsc runs, and two byte-identical
  typokat summaries. Compare identities with the 2026-08-10 evidence, not aggregate counts alone.
- **Acceptance / witness.** `src/core/generator.ts:36:48 TK2345` disappears. Roots remain 9,
  checked/skipped/excluded remain 9/0/0, resolutions remain 13, and notices/parse errors remain 0/0.
  Record any newly visible output and the next first blocker; the seven known incomplete records
  and five independent diagnostics may remain. A different reviewer reproduces the evidence and
  returns PASS. Backlog `72` remains open unless every unchanged zero-clean and meaning-equivalence
  gate genuinely passes; no mutation/witness/CI work starts otherwise.
- **Touch points.** New compact evidence under `docs/archive/`, backlog `72`, removal of backlog
  `109` and its divergence row on ship, README/root/index updates, and this sprint's closure record
  plus archive move.

## Out of scope (explicit)

- Optional methods/accessors and possibly-nullish receiver diagnostics (`49`); this sprint concerns
  arguments passed to already represented optional/defaulted parameters.
- Optional tuple elements, spread-call traversal, callable-rest parity, and general signature
  relation/inference changes (`69`, `71`, `75`).
- Enums (`42`), computed object keys (`75`), template interpolation (`71`), predicate/call-flow
  narrowing (`50`), and apparent array surfaces (`107`) still visible in `placetext`.
- `exactOptionalPropertyTypes`, new compiler flags, parameter-body widening, type-store identity
  changes, project-specific edits/shims, witness mutations, CI qualification, and performance work.

## Decisions

- Model explicit-`undefined` acceptance at the call boundary. Do not rewrite `ParameterType.ty` as
  `T | undefined`; that would alter body-local types, signature identity/relation, contextual
  targets, and existing diagnostic text.
- Preserve the deferred relation/effect pipeline. Optional acceptance is not permission for an
  eager diagnostic shortcut or a new global relation rule.
- One shared fixed-slot representation must serve call, overload, `new`, `super`, contextual
  inference, and rest-prefix consumers. Repeated consumer-specific conditionals are a review
  failure.
- The immutable project gate remains exactly zero. Removing one blocker records progress; it does
  not qualify backlog `72` or authorize mutations.

## Sequencing

WU0 → WU1 → WU2 → WU3. Only read-only tsc diagnostic-shape probes and official-suite triage may run
in parallel with WU1; implementation, evidence mutation, and sprint closure remain ordered.

## Run log

- **2026-08-11 — WU0 established the shared boundary.** The enabled corpus covers direct calls,
  overloads, function values, methods, constructors, `new`, and `super`, plus required/rest/null/
  unknown/recovery controls. Pinned tsc 6.0.3 established that optional acceptance must not rewrite
  stored parameter types or wrong-scalar diagnostic targets.
- **2026-08-11 — WU1 review isolated two unrelated inference gaps.** A contextual dependent-default
  witness failed without any optional fallback, and a fresh structural multi-member witness failed
  without the optional argument. They were removed from backlog `109` rather than widening its seam.
  Durable owners: [`110`](../backlog/110-contextual-return-literal-inference.md) and
  [`111`](../backlog/111-structural-inference-literal-provenance.md).
- **2026-08-11 — WU1 and WU2 passed.** The final independent implementation review found no HIGH or
  MEDIUM issue. Workspace tests, clippy, formatting, the official suite, focused differential
  matrix, committed repros, and 1,200 randomized pre-change comparisons all passed.
- **2026-08-11 — WU3 passed.** Two typokat summaries were byte-identical at
  `49c9b3f69d3a2608bba59acd68db5e6d8d41ea96d6a90fd0dc05ccbebe83a649`. The optional-parameter
  `TK2345` disappeared with no route, incomplete, or independent diagnostic drift. The first
  remaining normalized diagnostic is the enum-owned `repo/src/core/types.ts:68:12 TK2304`.
  Independent evidence review returned PASS; backlog `72` retains the zero-clean and meaning-
  equivalence gates.
