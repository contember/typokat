# Sprint — soundness tail quick wins (2026-07-08)

**Goal.** Close a batch of remaining known silent-false-negative families that are
small or low-coupling enough to ship before the next signature-shape/model sprint.

**Theme.** This sprint deliberately stays on the soundness tail before the `24`+`39`
signature migration. Each WU is a leader-verified known-gap item whose fix should not
require changing the function/type representation; `65` is stretch because it may expose
broader inference-policy work.

## Refs re-verified at HEAD (2026-07-08)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `readonly` type operators are not lowerable except `keyof`, so
  `readonly (infer U)[]` degrades before the array/infer path can collect `U` —
  `src/check/checker/annotations.rs:103`.
- ✔ Function signature lowering still rejects rest/optional/default parameters, so this
  sprint must not rely on `(...args: T[])` or `b?: T` signatures — `src/check/checker/annotations.rs:1031`.
- ✔ `fix_params` substitutes a constraint and immediately gates on
  `contains_deferred_keyof` without first demand-evaluating the substituted node —
  `src/check/infer.rs:283`.
- ✔ `check_type_argument_constraints` already has the evaluate-then-gate discipline to
  mirror in `fix_params` — `src/check/checker/calls.rs:51`.
- ✔ `infer_expr` has no `TSAsExpression` / `TSTypeAssertion` arm; cast expressions fall
  through the out-of-subset path and therefore vanish at assignment/call sites —
  `src/check/checker/expr.rs:26`.
- ✔ `infer_initializer` contextualizes only fresh object/array literals and otherwise
  delegates to `infer_expr`, so a cast RHS currently bypasses the normal annotation
  relation by returning `None` — `src/check/checker/expr.rs:287`.
- ✔ The statement checker still drops unhandled statements in `_ => {}`, with no
  labeled-statement arm — `src/check/checker/statements.rs:97`.
- ✔ The flow builder targets unlabeled `break`/`continue` only through the innermost
  break/loop stacks; label operands are ignored today — `src/check/checker/flowgraph.rs:115`.
- ✔ Project-mode fill drains only `pass.diagnostics` per module before body checking;
  `override_checks` stay global until the first body-phase `emit_pending_checks` —
  `src/check/checker/mod.rs:611`.
- ✔ `collect_list_export` records empty slots for `export { ghost }` instead of
  validating that the local name resolved — `src/check/checker/mod.rs:740`.

## Work units

### WU1 — b64: `readonly (infer U)[]` binder/lowering gap (effort S)

- **Problem.** A conditional infer binder under `readonly` array syntax raises a spurious
  `TK2304`, degrades the alias to error, and masks downstream assignment errors.
- **Verify first.** Re-run probes for `readonly (infer U)[]`, readonly tuples, and the
  mutable `(infer U)[]` control against typokat and `tsc 6.0.3 --strict`.
- **Scope.** Make readonly array/tuple type syntax transparent for the existing lowerer
  and infer-binder collection path; audit `TSTypeOperator` traversal so this fix stays
  local to `readonly`, not a broad `keyof` rewrite.
- **Acceptance / witness.** New `b64_readonly_infer_binder/` corpus: readonly array
  extraction reports the downstream `TK2322`; mutable control stays clean/erroring as
  before; unsupported readonly forms do not become permissive.
- **Touch points.** `src/check/checker/annotations.rs`, conditional-infer fixtures,
  `tests/conformance.rs`, `tests/cases/README.md`.

### WU2 — b34: evaluate deferred `keyof` before `fix_params` gate (effort S)

- **Problem.** Generic calls with `K extends keyof T` can skip a concrete constraint
  violation because the substituted `keyof` is gated before evaluation.
- **Verify first.** Pin `f({ a: 1, b: 2 }, "c")` vs `tsc 6.0.3 --strict`; include a
  free-parameter shape that must remain gated.
- **Scope.** In `fix_params`, evaluate the substituted constraint through the shared
  conditional evaluator before `contains_deferred_keyof`; leave truly deferred
  constraints gated.
- **Acceptance / witness.** New `b34_fix_params_keyof/` corpus reports `TK2345` for
  concrete bad keys and keeps existing M24/M28 constraint fixtures green.
- **Touch points.** `src/check/infer.rs`, `src/check/checker/eval.rs`, existing
  `m10_inference/` and `m24_generic_constraints/` coverage.

### WU3 — b33: `as` expressions participate in assignability (effort M)

- **Problem.** Cast expressions currently return no expression type, so initializer and
  call-argument checks can disappear instead of relating the asserted type to the target.
- **Verify first.** Pin primitive/object cast RHS declarations, call arguments, legal
  narrowing/upcast controls, and `as const` behavior against `tsc 6.0.3 --strict`.
- **Scope.** Type `TSAsExpression` / `TSTypeAssertion` as the asserted type while still
  walking the source expression for nested diagnostics; route the asserted type through
  the normal assignment/call relation. Cast-validity `TS2352` is out of scope unless it
  falls out cheaply without widening the sprint.
- **Acceptance / witness.** New `b33_as_cast_assignability/` corpus reports the declared
  annotation/call-site errors (`TK2322`, `TK2741`/object missing-property equivalent,
  `TK2345`) and keeps legal casts clean.
- **Touch points.** `src/check/checker/expr.rs`, `src/check/checker/assignment.rs`,
  declaration initializer path, tests/cases docs.

### WU4 — b54: labeled statements are checked and flow-aware (effort M)

- **Problem.** `foo: { ... }` bodies are invisible to both the statement checker and flow
  builder; labeled `break`/`continue` operands are ignored.
- **Verify first.** Pin labeled block, labeled `while`, `break outer`, and
  `continue outer` narrowing probes vs `tsc 6.0.3 --strict`.
- **Scope.** Treat labels as transparent for checking; add label-aware flow targets for
  `break`/`continue` without changing the loop fixpoint invariant.
- **Acceptance / witness.** New `b54_labeled_statements/` corpus catches assignments
  inside labels and preserves loop narrowing behavior after labeled exits/continues.
- **Touch points.** `src/check/checker/statements.rs`,
  `src/check/checker/flowgraph.rs`, possibly binder scope lookups for labeled blocks.

### WU5 — b59: M29 diagnostics hygiene and export-list validation (effort M)

- **Problem.** Project-mode fill-phase override checks can drain into the wrong module,
  and `export { ghost }` is accepted when nobody imports it.
- **Verify first.** Build two-file probes for cross-file override attribution and a
  single-file `export { ghost }` project fixture; compare positions/codes with `tsc`.
- **Scope.** Drain or tag pending checks per module during fill; validate list-export
  locals at collection time so the export site gets `TK2304`.
- **Acceptance / witness.** New `b59_modules_hygiene/` project corpus reports the
  override diagnostic in the derived module and reports `export { ghost }` without an
  importer.
- **Touch points.** `src/check/checker/mod.rs`, M29 project conformance harness,
  `tests/cases/README.md`.

### WU6 — b65: multi-argument inference fix-then-check (stretch, effort L)

- **Problem.** Same-`T` arguments can be unioned into a too-wide inferred type, dropping
  per-argument `TK2345`.
- **Verify first.** Re-check scalar, tuple/array-plus-scalar, contravariant, and
  genuinely-common-typed controls vs `tsc 6.0.3 --strict`.
- **Scope.** Adopt a fix-then-check discipline for multi-source covariant candidates
  only if it stays local; if it touches inference policy broadly, stop and file a
  dedicated sprint plan.
- **Acceptance / witness.** `m10_inference/inference_multi.ts` no longer pins the
  permissive divergence; new corpus catches the dropped `TK2345` while preserving valid
  common-type calls.
- **Touch points.** `src/check/infer.rs`, `docs/reference/divergences.md`,
  `m10_inference/` coverage.

## Out of scope (explicit)

- Track-A signature-shape work: `24` rest elements and `39` optional/default parameters.
- Larger relation/freshness parity items: `56`, `60`, `62`.
- Class/value aliasing follow-ups: `21`, `22`, `32`, `66`.
- `67` utility alias constraints, blocked by `24`.
- Cast-validity diagnostics (`TS2352`) unless WU3 can add them without broadening the
  expression model.

## Decisions

- Stay soundness-first for one more sprint; defer `24`+`39` to the next model sprint so
  the signature representation changes happen together.
- Execute WU1→WU5 sequentially by default. `65` is stretch and requires a stop/replan if
  the implementation cannot stay local to inference candidate fixing.
- Follow the dev method exactly for every WU: leader-written corpus/spec commit, worker
  implementation subagent, independent adversarial review subagent, leader verification
  (`cargo test`, `cargo clippy --all-targets -- -D warnings`, focused typokat/tsc probes,
  official-suite `run --check` where module/inference behavior could affect the
  scoreboard), then implementation commit.

## Sequencing

| Order | WU | Rationale |
|---|---|---|
| 1 | b64 | Smallest isolated lowering/binder gap; good warm-up and unlocks readonly infer coverage. |
| 2 | b34 | Small inference-gate fix using an existing evaluate-then-gate precedent. |
| 3 | b33 | Broader expression typing gap, but still localized. |
| 4 | b54 | Flow-sensitive, so run after the small non-flow fixes. |
| 5 | b59 | Project-mode hygiene; keep after single-file work to reduce simultaneous harness churn. |
| 6 | b65 stretch | Potentially policy-heavy; only run if the earlier WUs finish cleanly. |

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
- **WU1 shipped** — spec `8376eb5`; implementation uses a real internal `Readonly`
  wrapper for `readonly T[]` / `readonly [..]`, not transparent erasure. Review loop:
  round 1 FAIL caught readonly sources matching mutable infer patterns and readonly
  returns flowing to mutable arrays; round 2 FAIL caught read/indexed access returning
  error and masking downstream `TK2322`; focused round 3 PASS. Verification:
  `cargo test conformance`, `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  and `tsc 6.0.3 --strict` over the b64 corpus. Deferred safe over-report: contextual
  fresh literals against readonly tuple targets remain outside this WU.
- **WU2 shipped** — spec `54af4ba`; implementation demand-evaluates substituted
  inference constraints before the deferred-`keyof` gate, and keeps exhausted
  evaluation from becoming an accepted error-typed constraint. Review loop: round 1
  FAIL caught budget exhaustion being accepted and concrete union `keyof` staying
  gated; round 2 FAIL caught nested concrete `keyof` inside object constraints;
  round 3 FAIL caught `Pick<A | B, "shared">` degrading the value to error type;
  round 4 FAIL caught string-index-covered union keys and official-suite
  `intersectionTypeInference2` over-report; final focused review PASS. Verification:
  `cargo test conformance`, `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release`, official-suite `run --check` (0 regressions, one
  `intersectionTypeInference2` progress), and `tsc 6.0.3 --strict` over the b34 corpus.
  Backlog 35 is narrowed: concrete object-union common keys are implemented, while
  `K = never` and template-literal key sources remain deferred.
