# Sprint — soundness FN fixes (2026-07-07)

**Goal.** Kill the five HIGH silent-false-negative families found by the 2026-07-07
cross-cutting soundness review (backlog `53` `55` `57` `58` `61`).

**Theme.** Every item is a leader-verified dropped-error class at HEAD `5450070`. They are
independent surfaces, executed **sequentially** (one WU at a time, per the user's
direction), each through the full dev-method loop: spec corpus committed first → Opus
implementation subagent → independent Opus adversarial review → fix loop → leader commit.

## Refs re-verified at HEAD (2026-07-07)

All five findings were re-verified by the leader today against the release binary at HEAD
and `tsc 6.0.3 --noEmit --strict --target es2020` (probe transcripts in the review
session; code locations cited by the reviewers and spot-read):

- ✔ Template memo poisoning — `eval_template` has no exhausted gate and
  `finish_template_with_holes` inserts into the memo directly (`src/check/checker/eval.rs:920-948`,
  `:980-984`); control file errors, poisoned file goes silent.
- ✔ Project scope collision — `fn_scopes`/`block_scopes` keyed by span start
  (`src/binder/bind.rs:546`, `:567`, `:429`), shared `BindState` across modules
  (`bind.rs:187-238`); two offset-aligned files check clean where tsc errors.
- ✔ Class field initializers — `check_class_member_bodies` walks initializers via
  `infer_expr` only (`src/check/checker/classes.rs:918-931`); `n: number = "x"` silent.
- ✔ CFG assignment loss — cursor restored to `pre` after logical/conditional
  (`src/check/checker/flowgraph.rs:379-408`) and switch (`:220-221`, `:210-213`);
  while-test assignments orphaned (`:284-287`); sequence exprs unmodeled (`:370-371`).
- ✔ Inference pairing — like-kind arms only (`src/check/infer.rs:442-472`);
  `Elem<[1,2]>` binds `U = unknown`; `h<T>(t: [T, T])` with `[1,2]` infers `unknown`.

## Work units

### WU1 — b55: template memo poisoning under exhausted budget (effort S)

- **Problem.** A template hole evaluated after the shared TK2589 budget is exhausted
  resolves to error and is memoized durably; the pass-wide memo + hash-consing then
  silently degrade every later annotation interning the same template node.
- **Verify first.** Re-run the poison probe + control at WU start (binary rebuilt).
- **Scope.** Gate `eval_template` like the other node kinds; route its memoization
  through `SetMemo` (which refuses to commit when exhausted). No behavior change
  anywhere else.
- **Acceptance / witness.** `b55_template_memo/` corpus: poisoned file reports both
  TK2589 and the downstream TK2322; exhaustion-then-reuse pinned for conditional /
  instantiation / mapped / keyof (regression net); m27/m28 corpora unchanged.
- **Touch points.** `src/check/checker/eval.rs`.

### WU2 — b58: project-mode scope-key collision (effort M)

- **Problem.** Span-start-keyed `fn_scopes`/`block_scopes` collide across files in the
  shared project `BindState`; the checker descends into the wrong file's scope —
  layout-dependent dropped errors or spurious TK2304.
- **Verify first.** Re-run the collision probes (aligned offsets, same-named/absent
  bindings, arrow + block variants).
- **Scope.** Key the maps by module (module id + span, or a global node id), including
  `reference_flow` for safety. Extend the conformance harness's project path to run
  `b58_project_scopes/` (generalize the `m29_modules` special case).
- **Acceptance / witness.** `b58_project_scopes/` project fixtures: offset-aligned
  functions/arrows/blocks across two files report exactly tsc's diagnostics (both the
  silent-clean and spurious-TK2304 layouts); m29 corpus + single-file corpora unchanged.
- **Touch points.** `src/binder/bind.rs`, consumers in `src/check/checker/calls.rs`,
  `statements.rs`, `mod.rs`/`expr.rs`; `tests/conformance.rs`.

### WU3 — b61: class field initializers unchecked (effort M)

- **Problem.** Field initializers get no assignability/excess/contextual checking against
  the declared annotation.
- **Verify first.** Re-run the field-initializer probe (primitive, excess, tuple).
- **Scope.** Check each annotated field initializer like a variable-declaration
  initializer: TK2322 assignability, TK2353 excess, M30 contextual typing (object /
  array / tuple), instance + static fields; `readonly` fields stay initializable in
  their declaration; unannotated fields keep inference behavior.
- **Acceptance / witness.** `b61_field_initializers/` corpus vs tsc; m11/m13/m14/m30
  corpora unchanged.
- **Touch points.** `src/check/checker/classes.rs` (reuse the declaration-initializer
  path from `statements.rs`/`assignment.rs`).

### WU4 — b53: CFG assignment loss on pre-state restore (effort L)

- **Problem.** Four surfaces restore/bypass the flow cursor and drop Assignment nodes:
  `&&`/`||` RHS, ternary arms, switch clause bodies (rejoin + fallthrough + clause
  `break`), while-test assignments, sequence expressions (also unchecked in
  `infer_expr`).
- **Verify first.** Re-run the four probe families.
- **Scope.** Join real branch-end cursors instead of restoring `pre` (logical /
  conditional / switch); antecede while-condition nodes with the post-test cursor;
  model sequence expressions in the flow builder and `infer_expr`. The loop-fixpoint
  discipline (invariants §1) must not change.
- **Acceptance / witness.** `b53_cfg_assignments/` corpus (all four families + loop
  fixpoint regression pins) vs tsc; m23 corpus unchanged.
- **Touch points.** `src/check/checker/flowgraph.rs`, `src/check/checker/expr.rs`.

### WU5 — b57: Tuple↔Array inference pairings (effort M)

- **Problem.** No cross-kind candidate arms: tuple source vs `(infer U)[]` pattern binds
  `U = unknown` (wrong evaluation, accepts everything); fresh `[1,2]` argument vs
  `[T, T]` parameter infers `unknown` (spurious FP).
- **Verify first.** Re-run both probes; pin tsc's exact results for the spec shapes
  (element-union inference) before writing markers.
- **Scope.** Tuple-source vs Array-pattern inference arm (element union, mirroring the
  relation's tuple→array covariance) in both the evaluator's `infer` walker and
  call-site inference; the Array-literal-vs-Tuple-parameter side via contextual tuple
  typing of fresh array literals before inference (M30 ordering) OR a positional arm —
  decide by tsc probing at spec time, document the choice in the corpus README entry.
- **Acceptance / witness.** `b57_tuple_array_infer/` corpus vs tsc; m10/m25 corpora
  unchanged.
- **Touch points.** `src/check/infer.rs`, `src/check/checker/calls.rs`.

## Out of scope (explicit)

- The MED/LOW review findings (`54` `56` `59` `60` `62` `63`) — stay in the backlog.
- Any model-completeness work (track A) — separate milestones.
- `reference_flow` re-keying beyond what WU2 needs for safety.

## Decisions

- Sequential execution (user direction), WU order 1→5 (smallest/sharpest first; WU4 is
  the largest and lands after the two independent M-sized fixes).
- Implementation and adversarial review subagents run on **Opus** (user direction).
- Each WU ships as its own pair of commits (spec, then implementation) so a regression
  bisects to one WU.

## Sequencing

WU1 → WU2 → WU3 → WU4 → WU5, strictly sequential. Each WU: spec commit → impl agent →
review agent → fix loop → leader verify (`cargo test`, `clippy`, spot-run fixtures,
official-suite `run --check`) → commit → next.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. -->
