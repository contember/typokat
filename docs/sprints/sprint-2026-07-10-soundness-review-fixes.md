<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME - shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU0 -> <sha>,
> WU1 -> <sha>, ... Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint - soundness review fixes (2026-07-10)

**Goal.** Close the verified whole-project review findings before adding feature
breadth, and replace the prose-only 1.0 direction with measurable, checked-in
completion criteria.

**Theme.** HEAD has M0-M33 but still contains verified false-negative risks across
statement checking, binding, overload handling, type normalization, relation, and
type-level evaluation. The same review found that the conformance and official-suite
ratchets can miss regressions and that the 1.0 roadmap is not yet executable. These
belong in one sprint because checker fixes are not complete until the tests, CI, and
release criteria can prove and preserve them.

## Refs re-verified at HEAD (2026-07-10)

The whole-project review supplied the code references below as verified HEAD inputs.
WU0 must reproduce each observable behavior before implementation. `✔` = confirmed
live by the review; `⚠` = baseline or process fact that WU5/WU7/WU8 must ratchet.

- ✔ M0-M33 are shipped - `README.md:11` and the enabled `MILESTONE_DIRS` rows in
  `tests/conformance.rs`.
- ✔ There was no active sprint before this planning change - verified from
  `git show 2e20fc6:docs/sprints/README.md` and
  `git show 2e20fc6:docs/INDEX.md`.
- ⚠ The committed official-suite scoreboard baseline is `334/1677` recall and must
  only move for audited, intended checker progress -
  `tooling/official-suite/scoreboard.txt:4`.
- ⚠ The roadmap has 38 active backlog items; WU7 must turn the 1.0 subset into a
  validated manifest rather than treating all 38 as one undifferentiated target -
  `docs/backlog/README.md` and the numbered files under `docs/backlog/`.
- ✔ Nested assignments are not checked through every expression/statement path -
  `src/check/checker/expr.rs`, `src/check/checker/statements.rs:66`.
- ✔ Function return inference can stop at the first return -
  `src/check/checker/statements.rs:154`.
- ✔ `for`, `for-in`, `for-of`, and `do` statements are skipped, and `throw`
  operands are not checked - `src/check/checker/statements.rs:106`.
- ✔ Type-only exports can leak into value lookup - `src/check/checker/mod.rs:495`.
- ✔ `switch` clauses already share one scope, but it is the surrounding scope rather
  than a switch-local lexical scope, so block-scoped declarations leak past the end
  of the switch - `src/binder/bind.rs:369`.
- ✔ Local function overloads do not follow the supported overload path -
  `src/check/checker/calls.rs:1207`.
- ✔ Intersection normalization mishandles `any & never` -
  `src/types/intern/operators.rs:111`.
- ✔ Relating a source intersection can lose nominal-member origin -
  `src/relate/relation/objects.rs:775`.
- ✔ Recursive mapped-type evaluation has an unguarded recursion path -
  `src/check/checker/eval/mapped.rs:453`.
- ✔ `keyof` over a string index signature omits part of the key domain -
  `src/check/checker/eval/keyof.rs:117`.
- ✔ Official-suite comparisons can ratchet counts without preserving diagnostic
  identity and can accept an incomplete corpus -
  `tooling/official-suite/tsofficial.py:580`,
  `tooling/official-suite/tsofficial.py:591`.
- ✔ Official-suite process/output handling can lose a failed or unparsed checker
  invocation - `tooling/official-suite/tsofficial.py:178`.
- ✔ The conformance marker parser does not reject every malformed marker -
  `tests/conformance.rs:321`.

## Work units

### WU0 - Disabled soundness corpus and pinned tsc probes (effort L)

- **Effort.** L.
- **Problem.** The verified findings span independent checker subsystems, but there
  is no single acceptance corpus that pins all verdicts before the implementation
  changes. Writing fixes first would make the tests describe the implementation
  instead of the required TypeScript behavior.
- **Verify first.** Reproduce every listed finding at HEAD with the smallest stable
  `.ts` probe. Cross-check each probe with `tsc 6.0.3 --strict --noEmit`, recording
  the expected accept/reject verdict, diagnostic code, and only stable message
  substrings. Confirm controls for adjacent already-supported behavior.
- **Scope.** Add disabled review-fix conformance corpora covering nested assignment
  expressions (`src/check/checker/expr.rs`, `src/check/checker/statements.rs:66`),
  complete return inference (`src/check/checker/statements.rs:154`), `for`, `for-in`,
  `for-of`, `do`, and `throw` operand checking (`src/check/checker/statements.rs:106`),
  type-only export/value separation (`src/check/checker/mod.rs:495`), the missing
  switch-local boundary that lets a case declaration resolve after the switch
  (`src/binder/bind.rs:369`), local overloads
  (`src/check/checker/calls.rs:1207`), `any & never`
  (`src/types/intern/operators.rs:111`), source-intersection nominal origin
  (`src/relate/relation/objects.rs:775`), recursive mapped types
  (`src/check/checker/eval/mapped.rs:453`), and string-index `keyof`
  (`src/check/checker/eval/keyof.rs:117`). Use a separate disabled project-shaped
  corpus for the cross-file export witness and register it in `PROJECT_DIRS`; keep
  the single-file witnesses in separate disabled corpora owned by WU1, WU2, and WU3.
  Also add a distinct deferred-ledger corpus for the known but currently unfixtured
  under-reports from backlogs `30`, `56`, `60`, `62`, `66`, and `67`; backlog `63`'s
  annotation-depth crash belongs to WU3. Only the WU1-WU3 corpora flip enabled as
  their implementations land; deferred-ledger fixtures stay disabled until their
  own future implementation WUs. Document the ownership and markers. Commit the
  complete fixture corpus on its own before WU1 begins.
- **Acceptance / witness.** With all new directories disabled, the existing
  conformance result is behavior-neutral. Enabling each at unmodified HEAD exposes
  its pinned finding. Every fixture has a recorded `tsc 6.0.3` witness and a nearby
  negative or positive control. The spec-only commit contains no checker
  implementation.
- **Touch points.** `tests/cases/`, `tests/cases/README.md`,
  `tests/conformance.rs`, scratch `tsc 6.0.3` probes, and the cited source paths as
  read-only orientation.

### WU1 - Expression, statement, and return checking (effort L)

- **Effort.** L.
- **Problem.** Nested assignments can escape checking, return inference can depend
  on the first visited return, several loop statements are skipped, and `throw`
  operands are not checked. These are silent false-negative families in ordinary
  function bodies - `src/check/checker/expr.rs`,
  `src/check/checker/statements.rs:66`, `src/check/checker/statements.rs:106`,
  `src/check/checker/statements.rs:154`.
- **Verify first.** Run the focused WU0 fixtures and inspect the existing expression
  dispatcher, binder/statement dispatchers, return-type collection, and flow-graph
  paths. Confirm which existing helpers already implement assignment and operand
  checking, and identify the smallest structural fallback that checks unsupported
  loop forms without claiming the precise narrowing deferred to backlog `51`.
- **Scope.** Check assignments wherever they occur as expressions, not only in a
  top-level statement shape. Infer a function return from the complete applicable
  return set rather than visitation order. Structurally bind and check `for`,
  `for-in`, `for-of`, and `do` children, including initializer, condition,
  incrementor, iteration target/source, and body, using declared/START-flow fallback
  where precise CFG semantics are not implemented. Check every `throw` operand.
  Preserve backlog `51` as the owner of precise loop narrowing, closures, and member
  paths; do not introduce a second flow/narrowing model.
- **Acceptance / witness.** The WU0 expression/statement fixtures pass and match the
  pinned `tsc 6.0.3` verdicts. Controls prove nested assignment errors are reported,
  return order does not change inference, every newly visited loop component is
  bound and checked at least against declared types, and `throw` operand errors are
  visible. Existing while-loop CFG, early-exit, assignment-narrowing, and
  function-boundary fixtures remain green; no acceptance claim is made for precise
  narrowing in the newly traversed loop forms. The WU1-owned corpus flips enabled in
  the WU1 implementation commit; unrelated WU0 corpora remain disabled.
- **Touch points.** `src/check/checker/expr.rs`,
  `src/check/checker/statements.rs`, `src/binder/bind.rs`,
  `src/check/checker/flowgraph/mod.rs`, `src/check/flow.rs`, focused binder/checker
  tests, backlog `51` documentation if its boundary needs clarification, and the WU0
  corpus.

### WU2 - Scope, export-space, and local-overload fixes (effort L)

- **Effort.** L.
- **Problem.** A type-only export can be resolved as a value. `switch` clauses already
  bind into one shared scope, but that scope is the switch's parent, so a block-scoped
  declaration from a case incorrectly resolves after the switch. Local overload
  declarations also bypass the supported overload behavior -
  `src/check/checker/mod.rs:495`, `src/binder/bind.rs:369`,
  `src/check/checker/calls.rs:1207`.
- **Verify first.** Run the focused WU0 fixtures, inspect value/type/namespace slot
  selection for exports, confirm that all switch clauses currently receive the same
  parent `ScopeId`, identify how the checker will enter one new switch-local scope,
  and trace local function declarations through the M33 grouping and call-resolution
  path. Confirm that the fixes can stay within existing multi-slot symbols and
  overload representation.
- **Scope.** Enforce export-space separation so a type-only export never supplies a
  runtime value. Create one lexical scope owned by the switch body, bind every clause
  into that same scope, make the checker enter it, and preserve explicit nested block
  scopes. Do not add duplicate-declaration diagnostics: object/member `TK2300` and
  block-scoped `TK2451`, including duplicate declarations across cases, remain owned
  by backlog `18`. Route representable local function overload declarations through
  the existing ordered overload and hidden implementation-signature machinery.
  Preserve declaration identity and avoid new binder or module boundaries.
- **Acceptance / witness.** WU0 fixtures prove type-only exports fail value lookup,
  a `let`/`const` declared in a case no longer resolves after the switch (`TK2304`),
  and local overload calls select only declared overload signatures. A binder-level
  positive control proves all clauses use the same new switch-local `ScopeId`; it
  does not expect `TK2451` or expand backlog `18`. Other controls preserve valid type
  exports, explicit nested blocks, top-level overloads, and multi-slot
  class/type/value behavior. The WU2-owned flat/project corpora flip enabled in the
  WU2 implementation commit; unrelated WU0 corpora remain disabled.
- **Touch points.** `src/check/checker/mod.rs`, `src/binder/bind.rs`, binder symbol
  and scope tests, `src/check/checker/calls.rs`, declaration/overload checker state,
  module fixtures, backlog `18` as the explicit duplicate-diagnostic boundary, and
  the WU0 corpus.

### WU3 - Intersection, keyof, and recursion safety fixes (effort L)

- **Effort.** L.
- **Problem.** `any & never` is normalized incorrectly, nominal origin can be lost
  while relating source intersections, recursive mapped types can recurse without
  the evaluator's sound cycle guard, and a string index signature produces an
  incomplete `keyof` domain. Supported type-annotation lowering also lacks a nesting
  budget and backlog `63` records a native stack overflow around deeply nested type
  literals - `src/types/intern/operators.rs:111`,
  `src/relate/relation/objects.rs:775`,
  `src/check/checker/eval/mapped.rs:453`,
  `src/check/checker/eval/keyof.rs:117`, `docs/backlog/63-review-parity-tail.md`.
- **Verify first.** Run the focused WU0 fixtures and unit probes around intersection
  normalization, private/protected declaring-class identity, evaluator recursion and
  memoization, and `keyof` index domains. Re-read the relation-cache, hash-consing,
  nominal-class, and recursion requirements in `docs/reference/invariants.md` before
  changing these paths.
- **Scope.** Make `never` remain the annihilator for source-level `any & never` while
  preserving deliberate cascade suppression for the distinct internal error type.
  Preserve the declaring-class origin required for nominal checks when a source
  member is obtained from an intersection. Give mapped-value replacement its own
  recursion-aware in-progress/memo context that preserves rewritten recursive
  identity; do not recurse unboundedly, substitute the error type, or cache an
  incomplete rewrite as final. Include the full pinned tsc key domain for string
  index signatures. Add a shared, graceful type-annotation nesting budget or
  equivalent iterative lowering boundary for the pinned depth witness; do not abort
  the process or silently degrade the annotation to a permissive type. Carry all
  identity-bearing metadata unchanged.
- **Acceptance / witness.** WU0 fixtures and focused unit tests match the pinned
  `tsc 6.0.3` verdicts. Nominal controls distinguish same-origin from unrelated
  private/protected members; repeated and reordered recursive queries are stable;
  `keyof` controls cover string, number, and mixed index signatures; relation results
  do not become query-order dependent. The pinned deep annotation produces a stable
  diagnostic or controlled parse/check failure rather than a native stack overflow.
  The WU3-owned corpus flips enabled in the WU3 implementation commit; the
  deferred-ledger corpus remains disabled.
- **Touch points.** `src/types/intern/operators.rs`, type interner unit tests,
  `src/relate/relation/objects.rs`, relation tests,
  `src/check/checker/eval/mapped.rs`, `src/check/checker/eval/keyof.rs`, evaluator
  and annotation-lowering code/tests, `docs/backlog/63-review-parity-tail.md`, and the
  WU0 corpus.

### WU4 - Independent adversarial review of WU1-WU3 (effort M)

- **Effort.** M.
- **Problem.** The implementation batches touch the checker, binder, interner,
  relation engine, and evaluator. A green authored corpus can still miss silent false
  negatives, order-dependent cache behavior, or a permissive unsupported path.
- **Verify first.** After each of WU1, WU2, and WU3, a reviewer independent from that
  implementer reads the spec commit, reviews the focused uncommitted working-tree
  diff, and builds fresh adversarial probes before the leader commits or the next
  implementation WU starts. The review must hunt dropped diagnostics first and
  cross-check every disputed verdict with `tsc 6.0.3 --strict --noEmit`.
- **Scope.** Review nested expression traversal, all new statement paths, return
  collection order, CFG joins, export slot selection, the switch-local boundary
  without `TK2451` scope expansion, local overload visibility, intersection
  normalization, nominal origin, mapped recursion, and
  `keyof`. Test query/source order and repeated evaluation where caches are involved.
  Route concrete FAIL repros back to the responsible implementation work unit and
  re-review soundness fixes. File valid out-of-scope discoveries as backlog items or
  documented divergences instead of silently widening this sprint.
- **Acceptance / witness.** WU4-A, WU4-B, and WU4-C each return PASS with the exact
  probes and commands used before the following implementation cluster begins. Any
  initial FAIL adds a regression fixture to the spec and commits that fixture alone
  before the corresponding fix; the implementation returns to its owning agent and
  receives a subsequent PASS. The leader then runs the full required gate and commits
  the reviewed implementation. Every review byproduct is either resolved in the
  owning WU or linked from a newly filed backlog/divergence record.
- **Touch points.** WU0-WU3 commits and diffs, scratch probes, affected unit and
  conformance tests, `docs/backlog/`, and `docs/reference/divergences.md` when a
  byproduct requires a durable record.

### WU5 - Conformance and official-suite ratchet hardening (effort L)

- **Effort.** L.
- **Problem.** The official-suite checker can compare aggregate counts while losing
  diagnostic identity, can accept an incomplete corpus, and can fail to preserve a
  failed, inconsistent, or unparsed checker result -
  `tooling/official-suite/tsofficial.py:580`,
  `tooling/official-suite/tsofficial.py:591`,
  `tooling/official-suite/tsofficial.py:178`. The conformance parser can accept
  malformed markers, and exact diagnostic columns are not locked by focused tests -
  `tests/conformance.rs:321`.
- **Verify first.** Add failing unit-level witnesses for equal-count diagnostic
  replacement, a scoreboard entry missing from the corpus, checker exits `0`/`1`
  inconsistent with parsed diagnostics, a signal/other exit, unparsed output,
  malformed markers, and incorrect start/end spans in compact and rich diagnostics.
  Confirm the current harness outcome for each before changing it.
- **Scope.** Ratchet official-suite results by stable diagnostic identity rather
  than aggregate counts. Require the checked corpus and committed scoreboard to be
  complete in both directions. Accept only documented checker exits `0` and `1`,
  validate their consistency with parsed diagnostics, and treat signals, other exits,
  or non-empty unparsed output as explicit harness failures with actionable context.
  Reject malformed conformance markers instead of ignoring or partially parsing
  them. Add focused span and compact/rich renderer tests for exact columns without
  changing the official-suite line-plus-code comparison contract or weakening
  existing code/message assertions.
- **Acceptance / witness.** Each pre-fix witness fails for the intended reason and
  passes after the fix. A same-count diagnostic swap is a regression, missing corpus
  coverage is rejected, process/output failures cannot be scored as success,
  malformed markers fail fast, and exact-column tests distinguish diagnostics that
  share a line and cover multiline, tabbed, UTF-8, and EOF spans. The existing corpus
  remains accepted without marker rewrites that hide real diagnostics.
- **Touch points.** `tooling/official-suite/tsofficial.py`, official-suite tests and
  the committed scoreboard/manifest contract, the reproducibly fetched gitignored
  corpus, `tests/conformance.rs`, conformance harness unit tests, and
  `tests/cases/README.md`, `src/span.rs`, `src/diagnostics/writer.rs`, and focused
  diagnostic tests.

### WU6 - Checked-in CI gates (effort M)

- **Effort.** M.
- **Problem.** The required quality gates are documented but are not all enforced by
  checked-in CI, so formatting, tests, clippy, release builds, or official-suite
  regressions can reach HEAD without a repository-owned failure signal.
- **Verify first.** Identify the repository's canonical Rust/TypeScript toolchain
  setup, official-suite invocation, generated/downloaded inputs, and cache needs.
  Run each intended command locally once before encoding it, without publishing or
  deploying anything.
- **Scope.** Add checked-in CI that runs `cargo fmt --check`, `cargo test`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and the
  official-suite `run --check` path against the committed scoreboard and a freshly
  fetched corpus at the pinned TypeScript `6.0.3` commit. Make fetch authentication,
  cache validation, dependencies, and pinning explicit. Keep jobs diagnosable and
  avoid a CI-only wrapper that changes local semantics.
- **Acceptance / witness.** A clean revision passes every required job. Focused
  temporary failure probes demonstrate that each gate fails the workflow when its
  command fails. The official check consumes the release binary and rejects the WU5
  regression cases. No CI step deploys, publishes, or mutates the committed
  scoreboard.
- **Touch points.** `.github/workflows/`, Rust/toolchain metadata if already owned by
  the repository, official-suite setup/scripts, and contributor command docs.

### WU7 - Executable 1.0 manifest and documentation truth (effort L)

- **Effort.** L.
- **Problem.** The roadmap names a 1.0 direction, but completion is not represented
  by one validated artifact with owners and witnesses. The pinned standard-library
  target, backlog classifications, ownership boundaries, and several public/test
  documents can therefore drift independently.
- **Verify first.** Audit current 1.0 claims across the backlog, root README, scope,
  divergences, test docs, and official-suite docs. Run a pinned TypeScript `6.0.3`
  `lib.d.ts` surface audit and record its reproducible inputs and classifications.
  Resolve the current meaning of backlog items `30`, `63`, `15`, `16`, and `38`
  before editing their status or ownership.
- **Scope.** Add one checked-in 1.0 manifest plus a validator executed by tests/CI.
  Every criterion must have a stable id, classification, owner, witness command or
  artifact, dependency/backlog links, and an explicit incomplete/complete state.
  Record the pinned TS `6.0.3` lib audit without implementing full `lib.d.ts`.
  Classify backlog `30` and `63` against the manifest; assign unambiguous ownership
  for `15` and `16`; reconcile the deferred families in `21`, `56`, `60`, `62`, `66`,
  and `67`; correct divergences, root README coverage claims, M29's `TK2305`/`TK2307`
  scope classification, and test documentation where the audit or shipped behavior
  disproves them. End with an
  explicit go/defer record for backlog `38`: approve a later spec-first sprint only
  when the audited lead time to `14` is long enough that its replaceable early
  real-world signal repays the temporary semantic surface. Record prerequisites,
  owner, witness, replacement path, and cost either way. Do not implement `38` here.
- **Acceptance / witness.** The manifest validator fails for duplicate/missing ids,
  unknown backlog/divergence links, missing owners or witnesses, inconsistent states,
  and an unpinned TypeScript lib audit. CI executes the validator. Items `30`, `63`,
  `15`, `16`, and `38` each have one non-contradictory disposition, and README,
  scope, divergence, backlog, and test docs agree with the manifest and actual
  shipped surface.
- **Touch points.** The new checked-in manifest and validator,
  `docs/backlog/README.md` and affected item files, `README.md`,
  `docs/reference/scope.md`, `docs/reference/divergences.md`,
  `tests/cases/README.md`, `tooling/official-suite/README.md`, pinned TS `6.0.3`
  audit artifacts, and CI from WU6.

### WU8 - Full verification, audited scoreboard, and sprint closure (effort M)

- **Effort.** M.
- **Problem.** The sprint is not complete until all fixes and ratchets pass together,
  official-suite movement is attributed to intended checker changes, and active
  planning/docs no longer describe pre-sprint behavior.
- **Verify first.** Review the commit map against WU0-WU7, confirm every WU
  acceptance witness exists, and diff official diagnostic identities against the
  committed `334/1677` baseline before changing any scoreboard artifact.
- **Scope.** Run the focused WU0 corpus, full conformance suite, unit tests, format
  check, clippy with warnings denied, release build, manifest validator, pinned tsc
  probes, and official-suite `run --check`. Update the committed official scoreboard
  only for audited intended progress caused by WU1-WU3; never refresh away a
  regression, harness error, missing corpus entry, or unexplained change. Reconcile
  public/docs claims, prepend the factual OUTCOME and commit map, move this sprint to
  `docs/archive/`, and remove its active registrations.
- **Acceptance / witness.** All checked-in and local gates pass with zero unexplained
  diagnostic-identity regressions and a complete official corpus. Any scoreboard
  delta is mapped to a WU fixture and review witness. The final OUTCOME reports actual
  numbers and commands only. WU1-WU3 corpora are enabled, the deferred-ledger corpus
  remains explicitly disabled with live backlog owners, the sprint is archived, and
  active sprint indexes are empty unless another sprint was independently registered.
- **Touch points.** All WU0-WU7 outputs, official-suite scoreboard/corpus artifacts,
  `README.md`, affected reference/backlog/test docs, `docs/sprints/README.md`,
  `docs/INDEX.md`, and `docs/archive/`.

## Out of scope (explicit)

- Implementing all existing backlog items; this sprint only closes the verified
  review findings and classifies work needed for measurable 1.0 completion.
- Full `lib.d.ts` support; WU7 audits the pinned TS `6.0.3` surface but does not load
  or implement it.
- Implementation of backlog item `38`; WU7 records only its explicit go/defer gate.
- Stable structural hashing, parallelism, and incrementality.
- Terminal sanitization.
- Benchmark pinning.
- Relation-cache optimization; correctness fixes must preserve the current cache
  invariant and may not use performance refactoring as their vehicle.
- The bytecode VM, which remains a profiling-gated evaluator optimization.

New findings outside this list must be filed with a reproducible witness and owner;
they do not silently expand this sprint.

## Decisions

- Soundness review fixes and their regression ratchets precede any new feature
  breadth.
- WU0 is a spec-only commit and must land before any implementation commit. Its
  fixtures remain disabled until their owning fixes are ready. WU1-WU3 flip only
  their owned corpora; the deferred-ledger corpus is not enabled by this sprint.
- `tsc 6.0.3 --strict --noEmit` is the pinned semantic oracle for this sprint. Where
  typokat deliberately over-reports, the divergence must be explicit and documented.
- WU1-WU3 reuse the existing flow graph, multi-slot symbol model, overload model,
  type store, evaluator recursion discipline, and relation cache. This sprint does
  not authorize an architecture or module-boundary change; if a finding cannot fit,
  stop and resolve the design explicitly.
- WU2 owns only the switch-local scope boundary and the after-switch `TK2304` witness.
  Duplicate declarations and `TK2451` remain in backlog `18`.
- Independent reviewers, not the corresponding WU1-WU3 implementers, own the three
  WU4 checkpoints. A relation or recursion FAIL requires a new regression witness and
  re-review after the fix.
- Every implementation work unit is delegated to a bounded subagent and remains
  uncommitted until its required review and leader-owned verification pass. WU5-WU7
  each receive a distinct read-only review proportionate to their trust boundary
  before the leader commits; WU8 remains the leader-owned closure gate.
- The official-suite ratchet is diagnostic-identity based. Aggregate count stability
  is insufficient evidence of no regression.
- The 1.0 plan is one checked-in manifest enforced by a validator/CI, not a second
  prose roadmap. Existing docs explain it and link to it rather than duplicating
  mutable status.
- Backlog `38` has a hard lead-time/value decision gate in WU7 and no implementation
  budget in this sprint. A go decision schedules a later spec-first sprint; it does
  not widen WU8.
- Scoreboard changes belong only in WU8 after independent review and the full gate.
  The committed `334/1677` recall baseline is never rewritten merely to make
  `run --check` pass.

## Sequencing and commit boundaries

| Order | Work unit | Required commit boundary |
|---|---|---|
| 1 | WU0 | One corpus/docs spec commit; no checker code. |
| 2 | WU1 | Implementation subagent leaves one focused expression/statement/return diff uncommitted. |
| 3 | WU4-A | Independent WU1 review; after PASS, leader verifies and creates the WU1 implementation commit. |
| 4 | WU2 | Implementation subagent leaves one focused binder/export/local-overload diff uncommitted. |
| 5 | WU4-B | Independent WU2 review; after PASS, leader verifies and creates the WU2 implementation commit. |
| 6 | WU3 | Implementation subagent leaves one focused type/relation/evaluator diff uncommitted. |
| 7 | WU4-C | Independent WU3 review; after PASS, leader verifies and creates the WU3 implementation commit. Durable byproducts get separate backlog/divergence commits. |
| 8 | WU5 | One harness-hardening commit, split only if Rust conformance and Python official-suite changes are independently reviewable. |
| 9 | WU6 | One checked-in CI commit. |
| 10 | WU7 | One manifest/docs-truth commit, with generated audit data separated only if repository convention requires it. |
| 11 | WU8 | One audited ratchet/docs/closure commit after every gate passes. |

WU1-WU3 begin only after the WU0 spec commit and execute sequentially, each followed
by its WU4 review checkpoint, leader verification, and commit before the next cluster
starts. WU5-WU7 may be prepared independently only after WU4-C PASS, but WU6 must
consume the final WU5 commands and the WU7 validator. WU8 is strictly last. Every
commit uses explicit paths and follows the repository's prevailing commit-message
convention.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* -> ../decisions/NNNN ; new future work -> ../backlog/NN ;
     transient -> leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("-> ADR-0007"). -->

- **2026-07-10 - Plan registered.** No work unit has been executed; the scoreboard
  remains at the committed `334/1677` baseline.
