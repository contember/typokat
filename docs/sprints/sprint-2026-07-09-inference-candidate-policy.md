# Sprint - inference candidate policy (2026-07-09)

**Goal.** Ship backlog `65`: call-site inference must stop accepting incompatible
same-type-parameter arguments by unioning every candidate into a too-wide target.

**Theme.** This is a dedicated inference-policy sprint, not a small `fix_candidates`
tweak. The current engine intentionally unions multiple candidates, and the previous
quick-wins sprint deferred `65` because matching tsc requires candidate priority,
literal widening, freshness, rest/tuple provenance, and post-fix argument replay to
line up.

## Refs re-verified at HEAD (2026-07-09)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The inference module still documents the current policy as "multiple distinct
  candidates fix to their union" - `src/check/infer/mod.rs:4`.
- ✔ Call-site inference collects raw `TypeId` candidates in one `Vec` per type
  parameter, with only fresh/non-fresh sets retained per parameter; it does not retain
  argument index, parameter position, candidate priority, or variance provenance -
  `src/check/infer/mod.rs:93`, `src/check/infer/mod.rs:103`,
  `src/check/infer/mod.rs:134`.
- ✔ Rest-argument inference from M32 feeds the same raw candidate map, including the
  direct-rest-type-param fast path - `src/check/infer/mod.rs:117`.
- ✔ `fix_candidates` widens prepared call-site candidates and unions every distinct
  prepared candidate when more than one remains - `src/check/infer/mod.rs:229`,
  `src/check/infer/mod.rs:241`.
- ✔ Generic calls already instantiate the signature from the inference map and then
  run the ordinary per-argument relation against the substituted parameters; if the
  fixed map becomes narrower, this path is the witness for `TK2345` -
  `src/check/checker/calls.rs:172`, `src/check/checker/calls.rs:267`,
  `src/check/checker/calls.rs:348`.
- ✔ Generic class constructor inference also uses `infer_type_arguments_from_params`,
  so the policy change can affect `new Box(...)` shapes as well as ordinary calls -
  `src/check/checker/calls.rs:719`.
- ✔ Existing tests and fixtures deliberately pin the union policy and must be rewritten
  as part of the spec/implementation loop - `src/check/infer/tests.rs:244`,
  `src/check/infer/tests.rs:467`,
  `tests/cases/m10_inference/inference_multi.ts:1`.
- ⚠ Backlog `65` still names the old monolithic `src/check/infer.rs`; HEAD has the
  split module layout, so touch points are `src/check/infer/mod.rs` and
  `src/check/infer/context.rs` - `src/check/infer/mod.rs:8`,
  `src/check/infer/context.rs:1`.

## Work units

### WU1 - b65 corpus and tsc candidate matrix (effort M)

- **Problem.** The only current conformance witness for same-`T` multi-argument
  inference says `both(1, "s")` is valid by inferring `number | string`, which is the
  silent false negative this sprint is meant to remove.
- **Verify first.** Re-run a small matrix against `tsc 6.0.3 --strict --noEmit` before
  writing fixtures: `same<T>(a: T, b: T)`, return-position `T`, constrained primitive
  `T`, widened variables, tuple/array-plus-scalar, rest `T[]`, fresh object literals,
  and generic constructors. Do not encode exact target wording where tsc is
  shape-sensitive; pin the `TK2345` family and stable substrings.
- **Scope.** Add a disabled `tests/cases/b65_inference_candidate_policy/` corpus.
  Cover scalar same-`T` mismatches, clean same-family arguments, widened variable
  controls, tuple/array inference from the b57 surface, M32 rest-parameter calls,
  fresh-literal constraint exemptions from M24, and a generic constructor smoke test.
  Update `tests/cases/README.md` for the new bug-fix directory.
- **Acceptance / witness.** `cargo test` stays behavior-neutral while the directory is
  disabled; enabling the directory at HEAD fails on the pinned dropped `TK2345`
  cases and on the `m10_inference/inference_multi.ts` expectation that must change.
- **Touch points.** `tests/cases/b65_inference_candidate_policy/**`,
  `tests/cases/m10_inference/inference_multi.ts`, `tests/conformance.rs`,
  `tests/cases/README.md`, scratch `tsc` probes.

### WU2 - Candidate provenance model for call-site inference (effort L)

- **Problem.** A `FxHashMap<TypeParamId, Vec<TypeId>>` is not enough to implement
  tsc-like fixing without guessing: it has already lost which argument supplied a
  candidate, whether it came from a rest/tuple expansion, and whether the candidate is
  allowed to use the fresh-literal exemption.
- **Verify first.** Unit-test candidate collection independently from fixing: two
  arguments binding the same `T` must produce two distinguishable call-site
  contributions; conditional `infer` collection must still use its existing covariant
  same-name behavior.
- **Scope.** Introduce a call-site candidate representation that keeps provenance
  needed by fixing and replay, while leaving conditional-type `infer` semantics
  unchanged. Fold the existing fresh/non-fresh exemption into that representation
  instead of parallel side sets. Keep the public inference entry points stable unless
  the implementation proves a narrower signature is simpler and localized.
- **Acceptance / witness.** New unit tests show provenance survives direct parameters,
  tuple/array element inference, and rest-argument inference. Existing M25 conditional
  infer tests still pass without adopting call-site priority rules.
- **Touch points.** `src/check/infer/mod.rs`, `src/check/infer/context.rs`,
  `src/check/infer/tests.rs`.

### WU3 - Fix-then-check policy for multi-source candidates (effort L)

- **Problem.** `fix_candidates` currently makes the final parameter type wide enough
  to satisfy every argument, so the later assignability check has no chance to report
  the mismatched argument.
- **Verify first.** For each WU1 tsc probe, record the fixed target shape tsc appears
  to use: unconstrained `void` calls widen literals, return-position/constrained cases
  may preserve literals, tuple sources can preserve element unions, and genuinely
  common same-family candidates must remain clean.
- **Scope.** Replace union-everything fixing with a conservative tsc-like policy for
  call-site candidates: choose a fixed type per candidate priority/literal-widening
  rules, substitute it into the signature, and rely on the existing per-argument
  relation to report `TK2345`. Preserve the M24 constraint clamp and fresh-literal
  exemption; do not use `any`, casts, or a permissive fallback to hide unknown cases.
- **Acceptance / witness.** The b65 corpus reports `TK2345` for incompatible
  same-`T` scalar, array/tuple, and rest arguments; `both(1, 2)`, widened-compatible
  calls, and fresh object-literal controls stay clean. Unit tests no longer describe
  unioning as the general call-site policy.
- **Touch points.** `src/check/infer/mod.rs`, `src/check/infer/helpers.rs`,
  `src/check/checker/calls.rs`, `src/check/infer/tests.rs`,
  `tests/cases/m10_inference/inference_multi.ts`.

### WU4 - Integration audit: rest, constructors, constraints, and regressions (effort M)

- **Problem.** The same helper now powers ordinary generic calls, M32 rest calls, and
  generic class constructor inference; changing the fixing policy can easily create
  safe-looking false positives or re-open an older inference false negative.
- **Verify first.** Re-run focused controls from M10, M24, b57, M32, and generic class
  constructor fixtures before and after implementation. Cross-check any changed
  verdict with `tsc 6.0.3`.
- **Scope.** Add regression coverage for constructor inference if WU1 shows a live
  gap; keep rest `T[]` and tuple-rest candidates sound after WU3; preserve constraint
  evaluation and deferred-`keyof` behavior from the b34 fix.
- **Acceptance / witness.** `cargo test conformance` passes with the b65 directory
  enabled; M10/M24/b57/M32 focused fixtures keep their intended diagnostics; no
  official-suite regression is accepted without an audited safe-direction reason.
- **Touch points.** `src/check/checker/calls.rs`, `src/check/infer/mod.rs`,
  `tests/cases/m10_inference/`, `tests/cases/m24_generic_constraints/`,
  `tests/cases/b57_tuple_array_infer/`, `tests/cases/m32_signature_shape/`,
  constructor-related class fixtures if needed.

### WU5 - Review, docs, and ratchet (effort M)

- **Problem.** This is a policy change in the generic inference engine, so local green
  tests are not enough; a too-aggressive fix could silently reject valid TypeScript or
  mask a different dropped-error family.
- **Verify first.** Run the independent adversarial review required by the dev method,
  focused on false negatives in same-`T` calls, false positives in common-type calls,
  freshness/constraint interactions, and rest/tuple regressions.
- **Scope.** Update stale inference comments, the M10 fixture wording, and
  `docs/reference/divergences.md` if the gap is currently documented or if a new
  conservative deferral is discovered. Run the official-suite harness and ratchet only
  audited changes.
- **Acceptance / witness.** `cargo test`, `cargo test conformance`,
  `cargo clippy --all-targets -- -D warnings`, focused typokat/tsc probes, and
  official-suite `run --check` are green or have an explicitly reviewed
  safe-direction ratchet. Backlog `65` is deleted only after the implementation ships.
- **Touch points.** `docs/reference/divergences.md`, `tests/cases/README.md`,
  `docs/INDEX.md`, `docs/backlog/65-multi-arg-candidate-union-fn.md`,
  `tooling/official-suite/`.

## Out of scope (explicit)

- Track-A model completeness: overloads (`40`), generic methods (`41`), enums (`42`),
  namespaces/declaration merging (`43`), and `satisfies`/`as const` (`44`).
- Function overload resolution and `TK2769`; this sprint changes inference for a
  single selected generic signature only.
- Conditional-type variance parity (`68`) and signature rest parity tail (`69`),
  unless a tiny unit assertion is needed to prove they are unchanged.
- Other known silent-FN tail items: `56`, `60`, `62`, `32`, `21`, `22`, `66`, `67`.
- A broad rewrite of TypeScript's inference engine. If a case needs unsupported
  candidate categories, file a follow-up instead of inventing a permissive shortcut.

## Decisions

- Treat `65` as its own sprint because the previous quick-wins run proved the fix is
  candidate-policy work, not a local union/removal tweak.
- Keep call-site inference and conditional `infer` candidate semantics separate.
  Conditional same-name covariant candidates can continue to union; this sprint is
  about ordinary generic call/new inference.
- Put the new acceptance corpus under `b65_inference_candidate_policy/` rather than
  a new milestone directory: this is a known-gap fix on the M10/M32 surface, not a
  new model milestone.
- Prefer safe over-reporting to any fallback that accepts an incompatible argument.

## Sequencing

| Order | WU | Rationale |
|---|---|---|
| 1 | WU1 corpus | The tsc matrix is the acceptance spec and guards against oversimplifying candidate priority. |
| 2 | WU2 provenance | The current vector-only model cannot support the policy without guesswork. |
| 3 | WU3 fixing | Once provenance exists, change the actual fix-then-check behavior. |
| 4 | WU4 integration | Audit the shared call/new/rest surfaces after the core policy changes. |
| 5 | WU5 review | Close only after independent false-negative review and official-suite audit. |

Parallelism: WU1 fixture authoring can split by probe family. WU2-WU4 should stay in
one implementation context because the candidate representation and fixing policy are
coupled. The independent review must be done by a different agent after WU3/WU4 and
before deleting backlog `65`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* -> ../decisions/NNNN ; new future work -> ../backlog/NN ;
     transient -> leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("-> ADR-0007"). -->
- **WU1 spec committed** - plan `03fd02c`, initial corpus `dfa72e7`,
  explorer extension `a9455f0`. Two read-only explorer subagents cross-checked
  scalar, constrained, widened-variable, tuple/array, rest, constructor, and
  structural-candidate probes against `tsc 6.0.3 --strict` and current typokat.
  The new `b65_inference_candidate_policy/` corpus is registered disabled and
  behavior-neutral; `cargo test conformance` passes with it disabled. Current
  typokat is clean on the core scalar/array/rest/constructor/structural b65
  false negatives, while tsc reports `TS2345`; constrained non-fresh object
  controls already report through the existing M24 clamp.
