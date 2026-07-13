# Sprint — pre-lib model and soundness hardening (2026-07-12)

> **OUTCOME — shipped 2026-07-13.** Backlogs `22`, `56`, `77`, and `70` are closed;
> `13` remains the deliberate ADR-0001 DEFER/no-VM decision. **Commit map:** plan `96b6198`;
> WU0 `94a5b9a`/`e576e74`/`7db797e`/`e38afb5`/`81d1cc0`/`576885b`; WU1
> `351e0da`/`8db4c22`/`e72749d`; WU2 `6c3c4f7`; WU3 `38acd79`; WU4
> `597913a`/`9b9d976`/`0ed8688`/`a1bcc59`; WU5 `45ac183`; WU6 corpus-integrity
> hardening `7d6617a`, lifecycle correction `8f52bd8`, and pinned Git transport `51887d4`.
> Manifest v3 records 17 configured roots, 874 tests, 531 baselines, and persistent ref
> `refs/typokat/pinned/050880ce59e30b356b686bd3144efe24f875ebc8`, verified to resolve to that
> exact commit. Its post-fetch report has 368 in-scope / 506 out-of-scope tests, 324 matched
> diagnostics out of 1,352 expected, and zero regressions, progress, or missing corpus/scoreboard
> entries. All 62 harness tests, `cargo fmt --check`, manifest/surface/divergence tests, and
> `cargo build --release` passed. This closure commit archives the sprint and redirects its
> lifecycle/index links; backlog `43` remains the next direct `lib.d.ts` prerequisite.

**Goal.** Close backlogs `22`, `56`, `77`, `70`, and the `13` profiling gate so the
next sprint can focus on namespaces/declaration merging (`43`) with one remaining
direct `lib.d.ts` prerequisite and three known silent-false-negative families removed.

**Theme.** This is the bounded hardening batch before the XL namespace milestone:
finish the freshly enabled persistent-signature surface (`70`/`77`), repair two
independent dropped-error paths (`22`/`56`), and make the already-instrumented VM
decision (`13`). Each item remains independently specified, reviewed, committed, and
closable; the batch does not authorize a VM or namespace/type-container redesign.

## Refs re-verified at HEAD (2026-07-12)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ Method and call signatures explicitly discard `this_param`; overload lowering turns
  any overload set containing one into a `never` property —
  `src/check/checker/annotations/signatures.rs:34-42`, `:55-87`, `:99-108`.
- ✔ B41's persistent generic call/construct signatures are the current HEAD immediately
  preceding this sprint; call signatures are ordered identity-bearing children of
  `ObjectType`, propagated by hashing, substitution, and relation.
- ✔ `infer_new` recognizes class metadata only for a direct identifier callee; every
  parenthesized or aliased form falls through to public construct signatures and loses
  abstract/constructor-accessibility facts — `src/check/checker/calls.rs:950-1030`.
- ✔ Evaluator re-entry returns the error type, while `SetMemo` suppresses durable writes
  only for budget exhaustion; no cycle-taint prevents an ancestor from memoizing an
  error-derived result — `src/check/checker/eval/mod.rs:200-215`, `:297-323`,
  `src/check/checker/eval/instantiation.rs:8-21`, `:48-75`.
- ✔ Disabled ledger witnesses already pin the core `56` and `77` false negatives, but
  the shared `sr_deferred_ledger` directory cannot be enabled item-by-item —
  `tests/conformance.rs:128-130`.
- ✔ The synthetic benchmark harness already has ordinary application-style families,
  a type-level-heavy family, release-binary timing, memory sampling, and tsgo comparison;
  backlog `13` still requires profiler evidence that separates evaluator dispatch from
  relation/instantiation/allocation — `tooling/bench/README.md:1-20`, `:22-53`.
- ✔ The namespace symbol slot remains unused and explicitly post-MVP; backlog `43` is an
  XL binder/type-container/merge milestone rather than an honest parallel filler —
  `src/binder/symbol.rs:47-51`.
- ✔ The separate real-project preview sprint remains paused at its zero-threshold public
  witness gate; this sprint does not modify its contract —
  `docs/sprints/sprint-2026-07-12-real-project-preview.md`.

## Work units

### WU0 — isolated acceptance specs and profiling protocol (effort L)

- **Problem.** `56` and `77` share a permanently disabled ledger with unrelated backlog
  owners, while `22` and `70` lack independently enableable focused corpora. Backlog `13`
  has a benchmark harness but no committed profiling protocol/result artifact.
- **Verify first.** Re-run every proposed verdict at old HEAD against both current
  typokat and `tsc 6.0.3 --strict`; prove each disabled directory fails only for its own
  missing behavior and leaves clean controls green.
- **Scope.** Commit one behavior-neutral spec per semantic item: dedicated `b22_*`,
  `b56_*`, `b77_*`, and `b70_*` directories, each registered disabled. Move/strengthen
  the existing `56`/`77` witnesses rather than duplicating them and update every durable
  witness link. For `13`, commit a reproducible profiling protocol or only the smallest
  benchmark extension needed to distinguish dispatch from other hot paths.
- **Acceptance / witness.** Every spec commit leaves `cargo test` green; flipping only
  its directory to enabled at old HEAD fails on the intended missing diagnostic/verdict.
- **Touch points.** `tests/cases/b{22,56,70,77}_*/`, `tests/conformance.rs`,
  `tests/cases/README.md`, `docs/reference/divergences.md`,
  `docs/backlog/completion-1.0.toml`, optionally `tooling/bench/`.

### WU1 — preserve class construction facts through admitted callee forms (`22`, effort S/M)

- **Problem.** Parenthesized and single-step const-aliased class constructors bypass
  `TK2511` and `TK2673`/`TK2674`, even though direct construction diagnoses them.
- **Verify first.** Cross-check direct/parenthesized/aliased public, abstract, private,
  protected, inherited, and generic constructor forms against tsc; trace whether the
  originating class `DeclId` can be recovered without general alias-flow analysis.
- **Scope.** Make the backlog's parenthesized and const-alias forms retain the same
  class-keyed checks and instance/constructor behavior as the direct form. Do not grant
  arbitrary structural construct signatures nominal class restrictions.
- **Stop gate.** Stop and ask if exact const-alias support requires general flow-sensitive
  alias analysis or a new static-side metadata ownership model.
- **Acceptance / witness.** Enable the focused corpus; exact accessibility/abstract codes
  match tsc, public/generic controls stay clean, and existing class/new corpora retain
  their identities.
- **Touch points.** `src/check/checker/calls.rs`, class/value declaration provenance only
  if the verify-first trace proves it necessary, focused tests.

### WU2 — cycle diagnostics without evaluator memo poisoning (`56`, effort M)

- **Problem.** Direct and mutual instantiation cycles resolve silently to the any-like
  error type, and ancestors can durably memoize the cycle-derived value.
- **Verify first.** Trace the task/value stack for direct and mutual cycles in both query
  orders and prove which `SetMemo` frames currently survive re-entry.
- **Scope.** Surface `TK2589` at the owning evaluation demand and propagate a
  non-memoizable cycle-taint through every ancestor that depended on the re-entry.
  Preserve legal recursive/deferred types and the existing budget-exhaustion discipline.
- **Stop gate.** Stop if diagnostic provenance cannot remain local to the demand site or
  if the only apparent fix is a global memo clear/error-as-success workaround.
- **Acceptance / witness.** Enable the focused corpus: direct + mutual cycles diagnose;
  cycle-then-legitimate-reuse, legitimate-reuse-then-cycle, and repeated-query controls
  prove no cache poisoning or order-dependent false negative.
- **Touch points.** `src/check/checker/eval/mod.rs`, `eval/instantiation.rs`, evaluator
  tests and focused conformance corpus.

### WU3 — infer `ReturnType` from object call signatures (`77`, effort M)

- **Problem.** Callable objects and overload sets satisfy the shipped `ReturnType`
  constraint, but conditional `infer R` ignores `ObjectType.call_signatures` and yields
  the error type, dropping result obligations.
- **Verify first.** Trace the represented ordered call-signature list through conditional
  candidate collection after WU2 and cross-check tsc's last-overload return rule.
- **Scope.** Feed represented object call signatures into the existing conditional-infer
  path, preserving source order, direct-function behavior, mixed properties, and nested
  use. Keep the shipped non-callable constraint unchanged.
- **Stop gate.** Stop if the fix needs a new variance/candidate-reduction policy owned by
  backlog `68`, signature sorting/canonicalization, or a relation-cache policy change.
- **Acceptance / witness.** Enable the focused corpus: single callable objects and
  overloads infer exact returns; wrong assignments diagnose; optional/rest/generic and
  direct-function controls retain tsc-parity verdicts.
- **Touch points.** Conditional inference/evaluation under `src/check/`, represented
  object call signatures, focused M25/M28/B41 controls.

### WU4 — explicit `this` signature slot and contextual `ThisType<T>` (`70`, effort L)

- **Problem.** Explicit receiver parameters and contextual `ThisType<T>` are discarded,
  making core `Function.apply/call/bind` and `ObjectConstructor.defineProperty`-shaped
  declarations silently permissive and blocking a sound `lib.es5.d.ts` load.
- **Verify first.** Produce a representation parity matrix for the new non-positional
  slot across structural hash/equality, interning, substitution, traversal/evaluation,
  relation, inference, display, and arity. Pin receiver variance and contextual lexical
  boundaries against tsc before changing `FunctionType`.
- **Scope.** Add one identity-bearing, substitution-aware explicit `this` slot that does
  not count toward positional arity and relates contravariantly. Lower it through free,
  method, object call-signature, generic, and overload paths. Implement contextual
  `ThisType<T>` for object-literal methods with scope restoration at every boundary.
- **Stop gate.** Stop and present a design if the distinct slot cannot compose with the
  B41 representation/cache model, or if contextual `ThisType` requires a broader object
  contextual-typing architecture change.
- **Acceptance / witness.** Enable the focused corpus: direct/generic/bind-shaped
  receivers, arity, assignment variance, object call signatures, and contextual object
  methods match tsc; generic-signature and recursive-relation regression suites stay
  green with no type-identity/order dependence.
- **Touch points.** `src/types/`, signature lowering under `src/check/checker/`,
  `src/relate/`, contextual object literal checking, diagnostics rendering/traversal,
  focused and B41 regression corpora.

### WU5 — post-evaluator profiling decision (`13`, effort S gate)

- **Problem.** The VM is deliberately deferred, but checker 1.0 still requires recorded
  measurements and an explicit go/defer decision under ADR-0001's trigger.
- **Verify first.** Build the release binary at the final semantic HEAD; validate the
  ordinary and type-level-heavy corpora and capture reproducible timing/memory baselines.
- **Scope.** Profile at least one ordinary and one deliberately type-level-heavy corpus
  with symbols/instrumentation sufficient to distinguish evaluator dispatch from
  relation, instantiation, allocation, parsing, and rendering. Record commands, inputs,
  machine/tool versions, measurements, and the decision.
- **Stop gate.** If interpreter dispatch is not the reproducible dominant hot spot, close
  `13` with **no VM**. If it is dominant, file/approve a separate VM sprint; do not build
  a VM inside this sprint.
- **Acceptance / witness.** Reproducible report plus a decision that satisfies ADR-0001
  and the manifest criterion; benchmark harness tests/validation stay green.
- **Touch points.** `tooling/bench/`, profiling report/decision docs, manifest/backlog
  closure. Checker/evaluator implementation is out of scope.

### WU6 — independent item reviews, final audit, and closure (effort L)

- **Problem.** Parallel agent throughput can hide cross-item coupling unless every
  semantic diff retains an independent review boundary and final combined audit.
- **Verify first.** For each item, a reviewer different from its Terra implementer starts
  from the committed spec and exact implementation diff, reruns fresh tsc
  probes, and hunts false negatives, false positives, cache/order dependence, and
  diagnostic identity drift.
- **Scope.** Remediate every FAIL through the original implementation agent, re-review
  cache/relation/type-identity changes, then run all final gates. Close each manifest
  criterion and divergence owner, delete the five shipped backlog files, archive this
  sprint with commit map/numbers, and leave `43` explicitly next.
- **Acceptance / witness.** Per-item PASS; `cargo fmt --check`; `cargo test`;
  `cargo clippy --all-targets -- -D warnings`; `cargo build --release`; focused CLI
  probes; benchmark validation; and a freshly fetched official-suite identity ratchet
  all pass without lost matched diagnostics.
- **Touch points.** Focused diffs/probes, completion manifest, divergence ledger,
  backlog/sprint/archive indexes, public/reference docs only where behavior changed.

## Out of scope (explicit)

- Namespace binding, qualified type containers, and declaration merging — backlog
  [`43`](../backlog/43-namespaces-declaration-merging.md), the immediately following
  dedicated XL sprint and the remaining direct blocker of `14` after `70`.
- Full `lib.d.ts` loading (`14`), resolver breadth (`15`), parallel identity (`16`), and
  incrementality (`17`).
- Any bytecode VM implementation. Backlog `13` owns only the evidence-backed decision;
  a positive trigger requires a separately approved sprint under ADR-0001.
- General alias-flow analysis or arbitrary constructor provenance beyond backlog `22`'s
  admitted parenthesized/single-step const-alias forms.
- Contravariant multi-candidate intersection policy (`68`) or broader conditional infer
  parity while implementing `77`.
- `this`-based flow narrowing, implicit-`this` diagnostics, decorators, or general
  contextual object-model expansion beyond explicit receiver slots and `ThisType<T>`.
- Resuming or relaxing the paused real-project preview (`72`).

## Decisions

- This is one larger sprint with **five independently closable items**, not one combined
  implementation diff. Each item keeps its own spec commit, implementation commit, and
  independent review evidence.
- Terra agents implement and independently review. The leader owns fixture specs,
  integration, verification, and commits; implementation agents never commit.
- Only one agent mutates source at a time in the shared worktree. Other Terra capacity
  pipelines read-only trace/probe preparation and subsequent independent review.
- `56` precedes `77` because both touch conditional evaluation and cycle/memo discipline;
  `77` must be reviewed on the fixed evaluator state.
- `70` lands after the bounded fixes so its cross-cutting identity migration is isolated
  at a clean HEAD and receives a dedicated review. `13` profiles the final semantic state.
- `43` is not a stretch goal. Its namespace/merge architecture and overlap with `70`
  would erase a credible integration/review boundary; it becomes the next sprint.
- Stop gates are real outcomes: when one fires, preserve the acceptance witness, record
  the evidence, rescope the backlog, and ask before changing architecture or scope.

## Sequencing

| Order | Integrating work | Pipelined Terra work | Gate |
| --- | --- | --- | --- |
| 1 | WU0 specs, one atomic commit per item | Read-only code traces and tsc matrices | All specs disabled and behavior-neutral |
| 2 | WU1 `22` | WU2 review probes / WU4 representation trace | Independent `22` PASS + leader commit |
| 3 | WU2 `56` | WU3 evaluator trace / WU2 adversarial probes | No memo poisoning; independent PASS |
| 4 | WU3 `77` | WU4 corpus/probe strengthening | No `68` policy expansion; independent PASS |
| 5 | WU4 `70` | WU5 profiling setup / WU4 adversarial probes | Identity/cache matrix + independent PASS |
| 6 | WU5 `13` | Final cross-item audit preparation | Recorded go/defer decision; no in-sprint VM |
| 7 | WU6 closure | Independent combined regression audit | Full gates + official-suite ratchet |

The leader never stages blanket paths and commits only the exact files belonging to the
current item. A reviewer never reviews its own implementation. A failed review returns to
the original Terra implementer and repeats the relevant independent gate before integration.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-12 — Scope approved by the user after a three-Terra read-only deliberation.
  Two independent proposals converged on `22`/`56`/`77`/`70`/`13` with `43` deferred;
  the YAGNI critic's valid review-bottleneck objection is addressed by one-writer
  integration, per-item commits/reviews, stop gates, and pipelined read-only agent work.
- 2026-07-12 — WU1 review round 1 FAIL: parenthesized generic class values were
  incorrectly grouped with generic aliases, leaving abstract/private generic construction
  silent. Parentheses are transparent; a spec amendment now requires direct generic
  behavior through parentheses. Generic `const` aliases remain the separate stop-gated
  boundary because they require alias-level generic substitution.
- 2026-07-12 — WU1 remediation review PASS: direct generic classes now retain explicit
  and inferred construction plus class facts through arbitrary parentheses; non-generic
  one-step const aliases preserve lexical accessibility and declaring-class messages.
  The genuine generic-alias remainder graduated to backlog `78` with an isolated corpus.
- 2026-07-12 — WU2 independent review PASS: genuine evaluator re-entry taints every
  active memo ancestor, remains distinct from budget exhaustion and ordinary error
  values, emits one `TK2589` per demand, and leaves terminating siblings/query order
  exact. Constraint evaluation conservatively retains written constraints on a cycle.
- 2026-07-12 — WU3 independent review PASS: conditional-only Object→Function inference
  consumes exactly the last represented call signature; forward/reverse overloads,
  optional/rest calls, nested extraction, zero-signature controls, relation, and raw
  call-site inference retain their tsc-equivalent verdicts. Backlog `68` is untouched.
- 2026-07-12 — Before WU4, the surface ledger's polymorphic `this` type annotation/name
  rows moved from `70` to `75`: they are distinct from explicit receiver parameters and
  the contextual `ThisType<T>` marker, so implementing them here would expand scope.
- 2026-07-12 — WU4 checkpoint 2 verify-first amended the corpus: clean receiver controls
  no longer call `number.toFixed()` (which requires full `lib.d.ts`); direct typed
  assignments prove the receiver, and the string `ThisType` case now expects `TK2322`.
- 2026-07-13 — WU4 closed. Spec amendment `0ed8688` strengthened the enabled B70 acceptance
  corpus. The implementation adds an identity-bearing, substitution-aware receiver slot through
  free/method/object-call/overload lowering, relation/cache traversal, generic call inference,
  diagnostics, contextual `ThisType<T>`, and trusted `OmitThisParameter` evaluation. Review round
  one found the generic `OmitThisParameter` guard must use effective constraints (never defaults);
  the follow-up representation review required a receiver-only recursive generic relation witness
  and explicit documentation of the pre-existing union-callability gap (backlog `19`). Both final
  independent reviews passed. Evidence: `cargo fmt --check`; `cargo test` (326 passed, including
  enabled B70 conformance); `cargo clippy --all-targets -- -D warnings`; and `git diff --check`.
- 2026-07-13 — WU5 closed **DEFER / no VM** under ADR-0001. At `a1bcc59`, the committed protocol
  generated and preflight-validated 100k-line `flow` and `typelevel` corpora, then collected ten
  fresh-process timings and three peak-RSS samples per tool/corpus. `samply` was attempted three
  times for each corpus at 1000 Hz/25 iterations, but all six attempts failed before target start
  because `kernel.perf_event_paranoid=4`; no privilege change was made. With evaluator-dispatch
  self-time unavailable, the strict GO predicate cannot be proved, so the durable result is DEFER.
  Raw measurements, commands, hashes, host/tool versions, and limitations are in
  [`backlog-13-profiling-gate.md`](../archive/backlog-13-profiling-gate.md).
- 2026-07-13 — WU6 closed. Local Rust and benchmark gates passed; a fresh official fetch exposed
  corpus-integrity holes, hardened in `7d6617a` with independent offline reviews PASS/PASS.
  The closure transport replaces the unreliable API/raw path with a marked `full-blob-v1` bare
  Git cache and manifest v3. One non-interactive, shallow, exact-revision fetch populated 874
  tests (531 baseline); all 62 harness unit tests and `cargo build --release` passed, and
  `tsofficial.py run --check` reported zero regressions, progress, missing corpus, and missing
  scoreboard entries (324 matched diagnostics across 1,352 checked expectations).
