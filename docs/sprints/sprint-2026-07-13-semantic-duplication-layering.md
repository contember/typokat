# Sprint — semantic duplication and layering (2026-07-13)

**Goal.** Remove approved duplication at narrow semantic seams and eliminate only measured repeated
execution, without merging policy-bearing checker machines or changing observable TypeScript
semantics.

**Theme.** Similar-looking code is not automatically the same operation. This sprint first records
which effects each path owns, then shares only neutral mechanics. Code deduplication is accepted on
behavioral equivalence; execution optimizations need separate operation-count or timing evidence.

**Prerequisite satisfied.** The rewrite/hotpath sprint's WU7 and WU8 are complete and archived; its
three evaluator walkers are now independently hardened and are a boundary this sprint preserves,
not an active blocker —
[`../archive/sprint-2026-07-13-rewrite-hotpath-hardening.md`](../archive/sprint-2026-07-13-rewrite-hotpath-hardening.md).

## Refs re-verified at HEAD (2026-07-13, `24881c7`)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The archived rewrite/hotpath outcome records WU7/WU8 complete, independently reviewed, and
  ratcheted; it explicitly keeps `InferRewrite`, `InferenceConstraintEvaluator`, and
  `MappedRewrite` as three private machines — `docs/archive/sprint-2026-07-13-rewrite-hotpath-hardening.md:3-15`,
  `:202-236`.
- ⚠ Class signatures are lowered during `collect_class_own_members`, then member bodies call the
  ordinary function reserve/fill machine and re-lower signatures. Static methods subsequently
  delete duplicate signature-level `TK2302` diagnostics by span while retaining body diagnostics —
  `src/check/checker/classes/mod.rs:363-657`, `src/check/checker/classes/members.rs:83-173`.
- ✔ Class member meaning is spread across deliberate, differently ordered walks: overload grouping,
  parameter-property construction, member-kind classification, abstract completeness, override
  spans, and body checking — `src/check/checker/classes/mod.rs:379-657`, `:680-760`, `:883-1014`,
  `src/check/checker/classes/inheritance.rs:121-155`.
- ⚠ `tests/manifest.rs` and `tests/surface.rs` each implement the same string/array TOML subset, but
  their table names, result models, and unknown-header cursor behavior are not byte-for-byte
  identical — `tests/manifest.rs:26-152`, `tests/surface.rs:34-156`.
- ⚠ Parameter loops repeat name/annotation/optional/default/rest shaping across strict signature,
  permissive signature, contextual arrow, and body-binding paths. Their effects differ: strict
  failure, error recovery, contextual fallback, declaration publication, destructuring access
  checks, and initializer checking — `src/check/checker/annotations/functions.rs:196-261`,
  `src/check/checker/annotations/signatures.rs:186-222`, `src/check/checker/calls.rs:1834-1895`,
  `:1949-2050`.
- ✔ Single-file and project checking duplicate prelude parse/reserve/fill/check/handoff code while
  intentionally diverging afterward in module setup and scheduling — `src/check/checker/mod.rs:80-202`,
  `:274-430`. Intrinsic marker seeding already has one helper for project mode at `:686-706`, while
  single-file mode still spells the same table inline at `:152-169`.
- ✔ Call and construct overload selection duplicate the same speculative loop, first-constraint
  retention, arity aggregation, committed rebuild, and final diagnostic choice. Receiver semantics
  are the intentional difference — `src/check/checker/calls.rs:451-544`, `:1430-1521`.
- ⚠ Candidate selection currently rebuilds the winning candidate after a speculative match and the
  archived counters prove contextual rewalks, but speculative inference also touches diagnostics,
  type-parameter allocation, interning/evaluation, and relation work. No reuse boundary was approved
  by that measurement — `src/check/checker/calls.rs:482-528`, `:1460-1506`, archived sprint WU4c.
- ✔ Conditional `infer` freshens and rewrites both the extends operand and true branch before knowing
  whether the branch is selected — `src/check/checker/eval/extends.rs:527-577`. Inference constraints
  are demand-evaluated independently while fixing signature parameters — `src/check/infer/mod.rs:719-790`.
- ✔ Mapped replacement and constraint evaluation expose test-only counts but have intentionally
  different cycle, memo, pending, budget, and identity policies. The binding invariant forbids a
  generic walker that obscures those boundaries — `src/check/checker/eval/mapped.rs:3-223`,
  `:712-1068`, `src/check/checker/eval/extends.rs:38-129`,
  `docs/reference/invariants.md:23-33`.
- ✔ The mandatory build loop remains spec → delegated implementation → different independent
  adversarial reviewer → leader verification/commit — `docs/reference/dev-method.md:20-50`.

## Semantic operation matrix

The matrix is the review contract. A shared implementation may cover only the **neutral seam**;
the listed policy/effects stay with their caller.

| Surface | Duplication class | Neutral seam permitted | Policy/effects that must stay local | Proof before sharing |
|---|---|---|---|---|
| Class members | code duplication **and** repeated signature lowering | reserved, lifetime-free member callable surface consumed by the body pass | class/static type-param barriers, overload source order and implementation hiding, parameter properties, visibility/nominal metadata, initializer/body timing, diagnostic replay | class characterization corpus + exact diagnostic identity/order/cardinality |
| Test TOML | code duplication only | quoted string/array value parser and, only if equivalent, a parameterized table scanner | schema names, validation rules, paths, error aggregation and cursor recovery | both existing fixture suites retain exact accept/reject/error behavior |
| Parameters | code duplication; some callers also re-lower | pure parameter shape/enumeration helper | strict vs recovery policy, contextual target, `DeclTypes` writes, destructuring access, default-expression check timing | direct shape tests + function/arrow/method/constructor corpus |
| Prelude bootstrap | code duplication only per public entry path | parse/reserve/fill/check and lifetime-free handoff for one trusted prelude | single-file vs project module construction, import/export placeholders, module scheduling, user diagnostic channels | single/project prelude, shadowing, intrinsic-marker and declaration-index tests |
| Call/construct selector | code duplication **and** repeated candidate execution | common ordered selection control flow with an explicit receiver adapter | call receiver trial/final diagnostics, type-argument syntax source, constraint/arity/no-overload order, speculative vs committed effects | shared-selector parity corpus and effect ledger; no reuse in this WU |
| Conditional `infer` | repeated execution, not a helper-extraction target | defer true-branch rewrite until a matched branch demands it | fresh binder allocation/order, extends rewrite and relation, memo/cycle/budget behavior | false-heavy operation counter plus true/false semantic controls |
| Mapped/constraint walkers | similar structure and possible repeated execution | measurement helpers/counters only | all walker traversal, cycle, memo, pending, exhaustion, identity and cleanup policy | reviewed counter placement; no shared walker |

## Performance and equivalence gates

- **Code-duplication WUs (WU1-WU5):** no speedup claim is required. Diagnostics, incomplete records,
  obligations, returned `TypeId` structure, declaration publication, source order, and test-only
  operation counts must be unchanged unless the WU's spec explicitly says otherwise. Relevant
  release microcorpora may regress by no more than 2%; a noisy timing result is not evidence of
  regression when deterministic operation counts are unchanged.
- **Repeated-execution WUs (WU6-WU7):** select an optimization only if it removes at least 20% of
  the directly relevant deterministic operations at both 10k and 100k scale, or improves the
  isolated high-resolution median by at least 10% over five fresh-process repetitions. Every
  paired control must show no more than 2% regression in its deterministic operation count (or the
  same high-resolution timing metric when no direct count exists).
- Graph construction, parsing, and fixture generation happen outside timed regions. Record raw
  samples, median/range, environment, revision, exact command, and counter formulas in the run log.
  If a candidate misses its gate, record it and ship no production optimization.

## Work units

### WU0 — characterization corpus and speculative-effect ledger (effort L; spec first)

- **Problem.** A deduplication can preserve types while changing diagnostic cardinality, binder
  lifetime, source order, or speculative side effects. Those are not fully pinned together today.
- **Verify first.** Run focused existing class/call/prelude/evaluator tests and compare new syntax
  probes with `tsc 6.0.3 --strict`. Inventory every mutation performed by speculative candidate
  construction and trial checking.
- **Scope.** Commit behavior-neutral acceptance fixtures before production changes. Pin generic
  method binder scope; exact static `TK2302` cardinality and span; unresolved parameter/return/default
  diagnostics; method and constructor overload order plus implementation-signature hiding;
  constructor parameter properties; current externally visible omitted-return/`void` behavior;
  call/construct constraint, arity, receiver, and no-overload ordering; single/project prelude
  shadowing and intrinsic identity; conditional false/true `infer` paths. Add direct tests where
  display text cannot prove identity or ordering. Record a candidate-effect ledger covering at least
  diagnostics, incomplete records, obligations, type-parameter allocation, interner growth,
  conditional memo/evaluator state, relation queries, contextual AST walks, and test counters.
- **Acceptance / witness.** One separately committed spec change precedes WU1-WU7 implementation;
  current behavior is fully characterized, strict-`tsc` differences are classified, and no
  unowned false negative is accepted. The ledger names which effects are rollback-safe, replayed,
  monotonic, or unknown; any unknown forbids candidate reuse.
- **Review.** An agent independent of later implementation reviews the corpus for false-negative
  blind spots and cross-checks the syntax probes with strict `tsc`.
- **Touch points.** `tests/cases/sr_semantic_duplication/`, focused existing test modules,
  `tests/conformance.rs`, `tests/cases/README.md`, and this sprint's run log. No production source.

### WU1 — reserve class-member callable surfaces once (effort L)

- **Problem.** Class fill builds callable member signatures, but the body pass calls
  `infer_function` and lowers them again; static methods then filter duplicate `TK2302` records.
- **Verify first.** Trace each WU0 method/constructor through fill and body checking and record the
  exact signature-lowering count, source order, overload visibility, type-param frame, and diagnostic
  replay point.
- **Scope.** Introduce the smallest class-member reserved-surface representation needed to carry the
  already-lowered binder frame, receiver, parameters, declared return, and signature records into
  body checking. Consume it without rebuilding the public member type. Preserve separate field and
  accessor behavior; do not redesign class lowering or ordinary function hoisting.
- **Stop gate.** Stop if one surface cannot preserve static class-type-param barriers, generic method
  binder identity, overload implementation hiding, constructor parameter-property ownership, or
  source-position diagnostics without a new module boundary or suppression filter.
- **Acceptance / witness.** WU0 class fixtures retain exact diagnostics and external types; each
  method/constructor signature is lowered once; static body-local `TK2302` remains while duplicate
  signature `TK2302` no longer needs post-hoc deletion; class relation/nominal metadata is unchanged.
- **Review.** Different adversarial reviewer hunts dropped errors across static/instance generic
  methods, overloads, defaults, unresolved annotations, constructors and parameter properties.
- **Touch points.** `src/check/checker/classes/{mod,members}.rs`, the existing function-surface seam
  in `src/check/checker/{calls,context}.rs`, and WU0 tests only.

### WU2 — extract the test-only TOML subset helper (effort S)

- **Problem.** Two integration validators maintain nearly identical value and table parsing.
- **Verify first.** Diff parser behavior for unknown headers, keys before a header, duplicates,
  malformed values, multi-error aggregation, strings and arrays across both fixture suites.
- **Scope.** Extract only behavior proven identical into a test-support module. Keep manifest/surface
  schema models and validators local. If table scanning differs, share `Value`, `Table`, and
  `parse_value` only rather than adding policy flags to force a larger abstraction.
- **Acceptance / witness.** `cargo test --test manifest` and `cargo test --test surface` preserve all
  positive/negative fixture results and error substrings; no production dependency or general TOML
  parser is introduced.
- **Review.** Lightweight independent diff review confirms the helper did not weaken either schema.
- **Touch points.** `tests/manifest.rs`, `tests/surface.rs`, and a narrow `tests/support/` helper.

### WU3 — share neutral parameter shape mechanics (effort M)

- **Problem.** Four parameter-lowering loops repeat AST enumeration and shape construction while
  mixing it with intentionally different checker effects.
- **Verify first.** Build a caller matrix for ordinary/rest, optional/defaulted, missing/unresolved
  annotation, destructuring, contextual target, and declaration binding. Record which caller emits,
  recovers, binds, or checks executable defaults.
- **Scope.** Share only pure enumeration/name/shape mechanics (including one
  `parameter_from_shape`). Leave annotation lowering and every mutable checker effect at explicit
  caller sites. Keep strict `Option` failure distinct from recovery to the error type.
- **Stop gate.** Stop if the helper requires policy booleans/closures that obscure diagnostic,
  binding, contextual, or initializer timing; smaller duplication is preferable to a policy engine.
- **Acceptance / witness.** WU0 parameter/class corpus and existing signature tests retain exact
  behavior; no cast/`any` workaround; generated `ParameterType` order and flags are identical.
- **Review.** Independent reviewer compares all four callers against the operation matrix.
- **Touch points.** `src/check/checker/annotations/{functions,signatures}.rs`,
  `src/check/checker/calls.rs`, and focused tests.

### WU4 — centralize trusted prelude bootstrap (effort M)

- **Problem.** Single-file and project entry paths duplicate the trusted prelude lifecycle and have
  already drifted on intrinsic seeding shape.
- **Verify first.** Snapshot prelude type-declaration indices, `DeclTypes`, next type-param id,
  intrinsic markers, clean diagnostics, and user shadowing for both entry paths.
- **Scope.** Extract one bounded bootstrap/handoff routine for parse, reserve, fill, flow, check,
  lifetime-free type-param placeholders, resolved types, values, next id, and intrinsic seeding.
  Keep project binder construction, import/export placeholders, per-module fill, and user-pass
  scheduling outside it.
- **Stop gate.** Stop if the helper requires moving an OXC AST across lifetimes/threads, changing the
  run-local type universe, or creating a general compilation-unit abstraction.
- **Acceptance / witness.** WU0 prelude tests pass in single/project modes; declaration indices,
  shadowing, values and marker ids are identical; prelude diagnostics remain internal and clean.
- **Review.** Independent reviewer audits slot shadowing and handoff identity, not just test green.
- **Touch points.** `src/check/checker/mod.rs` and existing prelude/utility tests only.

### WU5 — unify call/construct selection control flow (effort L)

- **Problem.** Call and construct selectors duplicate a large ordered control-flow machine and can
  drift in failure precedence, yet call receivers are a real semantic difference.
- **Verify first.** Use WU0 to compare single-signature, overload match, constraint failure, mixed
  arity/mismatch, explicit/defaulted generic argument, contextual callback, and receiver cases.
- **Scope.** Extract one ordered selector kernel over an explicit request that supplies type
  arguments and receiver semantics. Keep signature discovery and final return handling in the call
  and construct wrappers. Preserve speculative build → trial → committed rebuild exactly.
- **Hard stop.** No candidate, contextual-result, diagnostic, relation, or inferred-map reuse in this
  WU. Any reuse proposal stops until WU0's effect ledger proves complete rollback/replay semantics
  and receives separate approval.
- **Acceptance / witness.** WU0 selector diagnostics, source order, receiver checks and operation
  counters are unchanged; the call and construct wrappers both exercise the shared kernel; no cache
  key or relation behavior changes.
- **Review.** Independent adversarial reviewer focuses on first failure, diagnostic cardinality,
  speculative leaks, callback rewalks, receiver absence/presence, and false negatives.
- **Touch points.** `src/check/checker/calls.rs` and narrow call measurement/tests.

### WU6 — lazily rewrite conditional-`infer` true branches (effort M)

- **Problem.** `run_extends_test` rewrites the true branch before the extends test, even when the
  false branch wins.
- **Verify first.** Add deterministic counts for infer-rewrite visits on false-heavy and true-heavy
  conditional corpora. Confirm the caller never observes the returned true branch on a failed test,
  and pin next-type-param allocation plus memo/cycle/budget outcomes.
- **Scope.** Defer only true-branch infer rewriting and final substitution until a successful extends
  test demands the branch. Keep fresh binder allocation, extends rewrite, candidate collection,
  relation, and selected-branch semantics unchanged.
- **Performance gate.** The false-heavy corpus must clear the repeated-execution gate; true-heavy and
  no-`infer` controls must not regress beyond the control threshold.
- **Stop gate.** Stop if laziness changes fresh binder ids visible to later work, evaluator memo
  entries, cycle/exhaustion behavior, or diagnostic ordering.
- **Acceptance / witness.** Strict-`tsc`-cross-checked WU0 cases are identical; false branches perform
  no true-branch rewrite visits; true branches retain exact inferred types and failure behavior.
- **Review.** Independent evaluator reviewer checks binder scope, cycle paths, unchanged identity and
  counter placement.
- **Touch points.** `src/check/checker/eval/{extends,instantiation,tests}.rs`, WU0 fixtures, and
  test-only measurement fields.

### WU7 — mapped/constraint execution measurements only (effort M)

- **Problem.** Similar private walker code and repeated visits are visible, but neither code shape nor
  raw `TypeId` repetition proves a sound reuse boundary.
- **Entry condition.** WU6 is committed and independently reviewed so this unit measures one settled
  `extends.rs` implementation and does not overlap edits to its shared tests.
- **Verify first.** Reuse the archived WU4b baselines, then add paired mapped-property fanout and
  inference-constraint shared-DAG/cycle/exhaustion corpora at 10k and 100k scale.
- **Scope.** Measure root calls, child visits, memo hits/inserts, re-entries, pending evaluations,
  identity returns, re-interns, exhaustion and per-property mapped contexts. Keep counters
  `cfg(test)` and construction outside timing. This WU selects or rejects future candidates; it does
  not implement one.
- **Hard stops.** No universal `TypeId` walker; no global/durable `TypeId → TypeId` cache; no cache
  shared across mapped properties or inference-constraint evaluations; no transfer of SCC taint,
  pending, exhaustion, budget, or partial-cycle policy between walkers.
- **Acceptance / witness.** Reviewed raw counters and controls explain repeated work without claiming
  an optimization. Any future candidate must name its complete context key and cleanup boundary,
  clear the performance gate, and receive explicit approval as a new WU/backlog item.
- **Review.** Independent reviewer validates every counter against the actual production edge and
  checks that instrumentation does not create a second traversal.
- **Touch points.** Test-only counters/microcorpora in `src/check/checker/eval/{mapped,extends,tests}.rs`
  and this sprint run log. No production behavior change.

### WU8 — independent final review, ratchet, and closure (effort L)

- **Problem.** Individually plausible deduplications can compose into reordered diagnostics,
  binder leakage, or a false-clean path.
- **Verify first.** A reviewer independent of WU1-WU7 starts from WU0 and audits the whole diff
  against the semantic operation matrix and binding invariants.
- **Scope.** Hunt false negatives and order/cardinality drift across class members, prelude
  shadowing, parameters, call/construct selection and conditional infer; validate all measurement
  claims and stop gates. Return concrete PASS/FAIL repros; failures go back to the responsible
  implementation agent and are re-reviewed.
- **Acceptance / witness.** `cargo fmt --check`; focused test crates; `cargo test`; `cargo clippy
  --all-targets -- -D warnings`; `cargo build --release`; selected release measurement corpora; and a
  fresh pinned official-suite `run --check` all pass with zero regressions/missing entries. Record
  commit map, raw measurements, reviews and deferrals in the outcome section, then archive and update
  indexes.
- **Touch points.** Read-only whole sprint diff, focused regression fixtures for confirmed failures,
  official-suite ratchet, sprint outcome/archive, and affected living reference docs only if current
  architecture wording changed.

## Out of scope (explicit)

- A universal type visitor/rewriter or shared evaluator task machine. The three hardened walkers'
  different policies are binding architecture, not cleanup debt.
- Global relation, substitution, evaluator, inference, mapped, or constraint caches keyed only by
  `TypeId`; cross-run caching, stable structural hashing, and incrementality remain separately owned.
- Candidate-local reuse, speculative transactions, or rollback machinery without the completed WU0
  effect ledger and separate explicit approval.
- Changes to relation cache keys/cycle policy/reason chains, CFG narrowing, class nominal identity,
  public TypeScript coverage, diagnostic codes, or deliberate divergence policy.
- A general TOML crate/dependency, a general compilation-unit/prelude framework, a bytecode VM, or
  cosmetic extraction whose only result is fewer lines.
- Optimizing mapped/constraint work in WU7. Measurement may propose follow-up ownership; it does not
  authorize implementation.

## Decisions

1. Classify every candidate as **code duplication**, **repeated execution**, or both. Line-count
   reduction uses equivalence gates; performance claims use measurement gates.
2. Share neutral data/mechanics, not semantic policy. If an abstraction needs many mode flags or
   mutable callbacks, retain local code and document parity instead.
3. The archived rewrite/hotpath sprint is closed. Its WU7/WU8 result is the entry baseline and the
   reason mapped/constraint walkers remain separate; there is no active-WU dependency.
4. Candidate selection may share control flow, but its speculative and committed executions remain
   distinct until a complete effect ledger proves reuse sound and the user separately approves it.
5. A raw repeated `TypeId` is never sufficient cache identity. Binder scope, substitutions,
   mapped-property value, pending evaluation, budget/exhaustion, cycle state, and cleanup lifetime
   must be explicit where applicable.
6. Every checker-affecting WU starts from a separate spec commit and ends with review by an agent who
   did not implement it. Review hunts false negatives first and cross-checks fresh syntax probes with
   `tsc 6.0.3 --strict`.

## Sequencing

| Order | Unit | Gate |
|---:|---|---|
| 1 | WU0 | Behavior-neutral spec commit and reviewed effect ledger before production edits. |
| 2 | WU1 | Class reservation first; it removes the most fragile diagnostic-suppression seam. |
| 3 | WU2 and WU3 | May run independently after WU0; one writer per disjoint file set. |
| 4 | WU4 | After WU1 so the bootstrap helper consumes the settled surface behavior. |
| 5 | WU5 | After WU0 ledger; selector sharing only, with candidate reuse forbidden. |
| 6 | WU6 | Gated optimization lands and receives independent review before shared evaluator files move again. |
| 7 | WU7 | Measurement-only follow-up starts from committed WU6; no overlapping `extends.rs`/test edits. |
| 8 | WU8 | Independent whole-diff review, full gates, ratchet, outcome and archive. |

Each production WU is an atomic implementation commit separate from WU0's spec commit. The leader
verifies and commits; implementation and adversarial review use different agents. No failing
performance candidate is folded into a neighboring cleanup commit.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-13 — Plan grounded at clean HEAD `24881c7`. The rewrite/hotpath sprint is archived with
  WU7/WU8 complete; this sprint treats its private-walker boundary and measurements as shipped input.
