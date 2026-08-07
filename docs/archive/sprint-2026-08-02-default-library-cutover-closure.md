# OUTCOME — SHIPPED (2026-08-07)

The exact pinned TypeScript 6.0.3 ES2025 full-host library is the production default on the public
driver, CLI, and batch routes. The production prelude fallback is deleted. The B14 corpus,
project-global merge routes, package gate, official-suite ratchet, differential gates, and local
closure gate are green. WU7 ended in independent **PASS** with zero unresolved HIGH or MEDIUM
findings after GitHub Actions run `31118462286`, attempt 3, verified exact head
`d1aa6d4c5f99dd5b95260b6d90203af45a24300a`; all eight push-applicable jobs passed and the scheduled
truth job was correctly skipped for the push event. The preceding WU5 remote gate was run
`31114984298`, with all eight push-applicable jobs green.

The authoritative fresh-process evidence is
`tooling/full-lib-bench/evidence/candidate-d1aa6d4.json`, SHA-256
`a120f159bb6bb68253dd6df80d03c1e035bf69a860947d79c2cdb781e82dda7a`. Its claim is limited to the
four approved rows: `fast-clean`, `fast-errors`, `collision`, and `fanout`. Every recorded
window/row cell exceeded the 1.25 engineering target on median, p95 ratio, and bootstrap lower
bound. It is not a general checker-performance claim. No production, build, collector, or contract
change followed that measurement, so the evidence remains valid at closure. Backlog `14` is
archived; resolver breadth, cross-file parallel identity, incrementality, and independently owned
semantic/parity tails remain open.

# Sprint — default-library cutover closure (2026-08-02)

**Goal.** Ship the exact pinned TypeScript 6.0.3 ES2025 full-host library through the ordinary
production CLI, with sound project-global merging, no production prelude fallback, a complete B14
corpus, and an authoritative fresh-process performance result whose claim is stated no more broadly
than its evidence.

**Theme.** The superseded
[`2026-07-21 sprint`](../archive/sprint-2026-07-21-full-lib-performance-cutover.md) proved the
source-backed library, immutable base, user delta, collision classifier, and sparse private epoch,
but its historical plan no longer described the live work. This closure sprint starts from the
actual nine conformance differences and first decides one remaining architecture question: keep the
library-provenance freeze and finish its merge route, or move the freeze boundary after project
declaration publication. The decision is evidence-gated; neither path may be implemented by
assumption.

## Refs re-verified at HEAD and the shared worktree (2026-08-02)

`✔` = confirmed live · `⚠` = live worktree delta or unresolved decision.

- ✔ The implementation baseline before this docs-only transition is `3008175`. The default-library
  profile is still the pinned 82-file TypeScript 6.0.3 ES2025 full-host closure
  (`tooling/full-lib-bench/contract.toml`). ADR-0017 still forbids a shipped semantic snapshot:
  every fresh process compiles the vendored sources.
- ✔ Backlog `102` is not an unimplemented M-sized blocker. `ScopeGraph::declare` returns a typed
  `FrozenScopeWrite`; binder value, function, import, declaration-list, type-group, type-slot, and
  namespace writes record `FrozenPrefixWriteSite`. Both B102 corpora are enabled and produced no
  difference in the retirement conformance run. Closure still requires reconciling the stale
  backlog document with those executable witnesses.
- ✔ `5790fbc` landed source-native sparse collision epochs. B103 flat and project corpora are
  enabled and produced no difference in the retirement conformance run. This proves the committed
  corpus on the current worktree, not production-route or benchmark closure.
- ✔ The current conformance result is **9 differences across 574 files**: one missing `TK2684` in
  `this_utility_shadow`, one obsolete RegExp incomplete marker, five global-value-publication
  differences, and two local-`Array`-heritage differences. B102 and B103 are absent from the
  failure list.
- ⚠ B14 is partially admitted: 10 of 14 flat fixtures and 2 of 12 project fixtures run through
  `ENABLED_FIXTURES` / `ENABLED_PROJECT_FIXTURES`; the directories remain disabled as wholes.
- ⚠ `tests/wu7_parse_routing.rs` contains an uncommitted, independently specified RED extension.
  A recoverable Oxc TS1063 parse produces four failing public-route tests because the recovered AST
  reaches semantic checking; three pre-existing parser-panic tests remain green.
- ⚠ The shared worktree contains an uncommitted two-file `declare global` patch. Its focused WU5
  forward/reverse, B103 `Date`/`Partial`, and control fixtures were exact, but the full conformance
  run still exposes global value publication and local `Array` heritage. The patch is parked until
  WU2 decides the phase boundary.
- ⚠ WU7 plumbing is present only as an uncommitted worktree delta: provider-backed public routes,
  `library-info`, batch protocol, test-only prelude isolation, and production `prelude.ts` deletion.
  HEAD has not made the atomic cutover and no full gate has accepted that delta.
- ✔ `tooling/full-lib-bench/contract.toml` gates four full-library rows: `fast-clean`,
  `fast-errors`, `collision`, and `fanout`. It does not prove general multi-file, generic-heavy, or
  flow-heavy performance; the checker-scaling sprint owns those families.
- ⚠ Local `main` is 480 commits ahead of `origin/main`. GitHub has run the sole CI workflow twice;
  the newest run failed before the differential and current library-package jobs existed. Remote CI
  publication requires explicit user authorization and is a closure gate, not assumed evidence.

## Work units

### WU0 — retire the stale delivery plan (effort S)

- **Problem.** The 2026-07-21 sprint is a historical run log with stale work-unit status, yet an
  active sprint has higher documentation precedence than ADRs and reference docs.
- **Verify first.** Re-run the exact conformance entry point and inventory every reference to the
  old sprint before moving it.
- **Scope.** Archive the old sprint with an explicit incomplete outcome; create this plan; refresh
  sprint, archive, index, backlog, reference, and historical links without touching production code.
- **Acceptance / witness.** The old active path has no live references; every new/moved relative
  link resolves; `git diff --check` passes; the docs-only commit stages exactly the intended files.
  The repository-wide docs lint retains its pre-existing baseline of 20 historical links to deleted
  backlog files plus the stray `docs/AGENTS.md`; WU0 must introduce no additional finding.
- **Touch points.** `docs/archive/`, `docs/sprints/`, `docs/INDEX.md`, and direct references.

### WU1 — stop recoverable syntax before semantics (effort S)

- **Problem.** Parser panics route to ordinary parse output, but a recoverable Oxc parse diagnostic
  continues through collision preflight into the semantic checker over a recovered AST.
- **Verify first.** Commit the existing four-test RED extension in
  `tests/wu7_parse_routing.rs` by itself. Preserve its `check_source`, `check_files`,
  `check_project`, CLI, order, cardinality, filename, no-semantic-diagnostic, and no-incomplete
  witnesses.
- **Scope.** Reject `parsed.panicked || !parsed.diagnostics.is_empty()` before census or semantic
  continuation, retain the typed `UserParseRejected` route, and preserve ordinary parser output.
- **Acceptance / witness.** Seven focused tests pass; valid shared/private controls do not move;
  independent adversarial review confirms no recovered AST reaches semantics.
- **Touch points.** `tests/wu7_parse_routing.rs`, `crates/typokat-frontend/src/frontend.rs`,
  `crates/typokat-library/src/{collision_preflight.rs,base.rs}`, and the exact driver match.

### WU2 — decide the freeze boundary (effort S, hard stop gate)

- **Problem.** The current design freezes library-provenance rows before user project declarations,
  then restores TypeScript merge semantics through overlays and sparse replay. An alternative may
  complete all library and project-global declaration publication before freezing, eliminating
  semantic replay while retaining a read-only checking phase. It is unknown whether typokat already
  has that phase boundary or would need a new publication architecture.
- **Verify first.** Trace the real pipeline from `ProjectBinderBuilder` collect/finalize through
  `reserve_type_decls`, class/type-group publication, validation, and `freeze_as_base`. Inventory
  which cross-file meanings depend on initializer/body inference, which state mutates during body
  checking, and what library sharing the official-suite batch would lose. Also classify every live
  B102/B103 shape by whether it is project-global/ambient.
- **Scope.** Read-only phase audit first. Only if the required barrier already exists, build one
  bounded experimental path outside production routing that binds and publishes the merged project
  environment before freeze. Do not modify ADR-0020 or delete current machinery during the probe.
- **Acceptance / witness.** Produce a falsifiable table covering phase availability, semantic
  equivalence on B102/B103 in both file orders, fast-clean/collision/fanout work, batch sharing, and
  expected deletable code. The probe must fit one working day.
- **Stop / decision.** If this requires a new declaration-publication model, invalidates previously
  published types, or loses library sharing without measured compensation, retain ADR-0020 and
  finish the current route. If it is predominantly an ordering change and the witnesses hold,
  write and accept a superseding ADR before production implementation. No third path or indefinite
  dual implementation.
- **Touch points.** Read-only across binder/checker/library/driver; any experiment must be isolated
  and removable.

### WU3 — close project-global semantics and the B14 corpus (effort L)

- **Problem.** Nine conformance differences remain and 14 B14 projects/fixtures are still gated.
- **Verify first.** Preserve the four current failure clusters. Cross-check each marker change with
  `tsc 6.0.3 --strict`; do not mass-rebaseline or treat an error type as success.
- **Scope.** Implement exactly one WU2-selected architecture. Under ADR-0020, independently review
  the parked `declare global` patch and extend lexical augmentation semantics only through new RED
  whole-body/value/nested witnesses. Under a superseding design, replace rather than layer another
  route. Fix local `Array` heritage and adjudicate the `this`/RegExp rows. Enable the remaining B14
  fixtures spec-first as their causes close.
- **Acceptance / witness.** Entire conformance corpus green with both B14 directories enabled as
  wholes; B102/B103 and module-scope controls exact in both file orders; no new incomplete channel.
- **Touch points.** Determined by WU2; expected binder declaration/namespace, checker publication,
  and conformance corpus paths.

### WU4 — complete and unify the production cutover (effort L)

- **Problem.** Commit `f2e5bc2` started this work unit early: the public driver, CLI,
  `library-info`, and official batch now use and attest the full-library provider, while the root
  facade still exposes the raw checker, `prelude.ts` remains compiled into that route, and the
  conformance harness deliberately uses it for `FixtureBase::Prelude`. The attestation is truthful
  for the driver consumers that emit it, but it is not evidence that the repository has one type
  universe. The planned atomic boundary has already been crossed and must not remain implicit.
- **Verify first.** Re-audit `check_source`, `check_files`, `check_project`, CLI exit behavior,
  `library-info`, batch supervision, raw checker exports, parse failures, and provider lifecycle.
  Treat `production-default-library` as a route attestation, not a repository-wide single-universe
  claim, until the source-inventory guard passes.
- **Scope.** Finish the cutover in one post-WU3 commit: delete production `prelude.ts`; move its
  required fixture support behind the existing test-only feature; remove raw checker exports from
  the root production facade; remove `FixtureBase` and route the conformance corpus through the
  public driver; finish warning cleanup and user-facing docs. Re-verify the already-landed provider,
  `library-info`, batch, and typed infrastructure behavior without reimplementing them.
- **Acceptance / witness.** Production acceptance is executable and green; the provider probe says
  `production-default-library`; the source-inventory guard proves that no production source or
  public facade references the retired prelude/raw route; conformance has no alternate base;
  malformed input remains exit 1, semantic incomplete remains exit 3, and infrastructure failure
  remains typed and fail-closed.
- **Touch points.** Existing WU7 worktree files in driver/frontend/library/check/root CLI/tests.

### WU5 — semantic, cross-tool, package, and CI gates (effort L)

- **Problem.** Local Cargo gates alone cannot prove the cutover; the branch has never exercised its
  current official-suite, differential, or package workflows remotely.
- **Verify first.** Build a pre-change comparison binary and a known-broken negative-control binary
  before touching inference/contextual results.
- **Scope.** Run full Cargo unit/integration/conformance, fmt, clippy, package verification,
  official-suite ratchet, randomized differential fuzz over multiple seeds against the pre-change
  binary, committed repro ratchet, and the live negative control. With user authorization, publish
  the exact committed branch to CI and require all current jobs green.
- **Acceptance / witness.** Zero unexplained semantic regression; the negative control fires; every
  CI job applicable to the pushed branch or its pull request executes and passes. The scheduled-only
  tsc truth-mode remains scheduled evidence and is never reported as executed by a push. Any
  scoreboard movement is cause-classified and separately specified, never smoothed by rebaseline.
- **Touch points.** Test/tooling outputs and only the source changes their failures justify.

### WU6 — authoritative performance claim (effort L)

- **Problem.** Preliminary 260 ms vs 289.6 ms runs are not WU8 evidence, and the four full-library
  rows cannot establish a universal checker-performance claim.
- **Verify first.** Freeze the exact release commit, binaries, package/profile identities, semantic
  oracles, route incidence, warm-sharing result, memory gate, and CPU lease. Confirm the benchmark
  negative/perturbation controls fail when deliberately broken.
- **Scope.** State the claim before timing. By default, WU8 proves only that the production
  full-library cutover is faster than the pinned native comparator on every approved contract row;
  multi-file/generics/flow remain owned by the scaling sprint unless the user explicitly broadens
  the contract first. Run the authoritative interleaved distributions without changing semantics.
- **Acceptance / witness.** Every row exceeds the binding `>1.00` threshold with its confidence and
  p95 conditions; engineering target remains 1.25×; raw evidence and binary identities are
  committed. A failing row stops closure and cannot be relabelled out of scope.
- **Touch points.** `tooling/full-lib-bench/` and evidence only after WU5 is green.

### WU6A — replace the failed standalone lifecycle (effort L)

- **Problem.** The first authoritative WU6 run is NO-GO because a one-project CLI process builds
  the shared base and then takes complete-source fallback. The non-authoritative probe provides
  `PROMISING` evidence sufficient to specify a one-publication candidate, but its example discards
  project import dependencies and is not a production route.
- **Verify first.** Pin the probe's `m29_modules/basic_named` false-`TK2304` result as a negative
  control. Trace one production entry from parse and dependency ordering through the library binder
  checkpoint and semantic publication. Reject any candidate that acquires the provider, constructs
  replay state, parses a packaged library source twice, or binds user imports with an empty
  dependency list.
- **Scope.** Implement ADR-0021 behind the real project frontend. A distinct `check_project_once`
  entry point gives ordinary standalone CLI checks one complete library-plus-project source
  publication. Existing driver APIs and the official batch retain the shared provider. Add distinct
  route attestations; keep parse-first routing, original report order, the exact fixed profile,
  library-ledger completion and census coverage, and the 256 MiB worker. Do not delete ADR-0020
  machinery or promote the test-only example.
- **Acceptance / witness.** Spec-first tests cover imports in both input orders, dependency cycles
  and missing modules, every B102/B103 shape in both orders, parser diagnostic/panic precedence,
  exit-3 incomplete output, mixed output, original report order, exactly one cold source compile,
  provider non-initialization, and one-base isolated batch reuse. Cross-route output is exact on the
  complete conformance corpus; the known broken import probe fires; full WU5 gates are rerun before
  WU6 is repeated.
- **Stop / falsifier.** Stop if project-correct complete-source checking requires a second semantic
  publication, a second library parse, a cache/snapshot, or loses any all-row `>1.00` probe gate.
- **Touch points.** `crates/typokat-frontend`, `crates/typokat-check`,
  `crates/typokat-library`, `crates/typokat-driver`, `src/main.rs`, production acceptance tests,
  and the benchmark route contract.

### WU7 — independent closure review (effort L)

- **Problem.** Base/delta identity, project-global merging, route selection, parse ordering, cache
  soundness, atomic cutover, and benchmark scope can each yield a locally green but false result.
- **Verify first.** Give a fresh reviewer the selected/superseded ADR, archived sprint, this sprint,
  complete commit range, raw gate outputs, and benchmark evidence.
- **Scope.** Adversarially hunt false negatives, cross-project contamination, order dependence,
  hidden prelude/test routes, error-type success, benchmark asymmetry, and any claim wider than its
  corpus.
- **Acceptance / witness.** Independent PASS with zero unresolved HIGH findings; all WU5 gates and
  WU6 evidence reproduced. Then archive this sprint with a shipped outcome, close backlog `14`, and
  refresh reference docs.
- **Touch points.** Read-only review first; remediation follows spec-first as separate commits.

## Out of scope (explicit)

- General multi-file, generic-heavy, and flow-heavy optimization beyond preserving their existing
  guards; owned by the checker-scaling sprint.
- Persistent semantic snapshots or cold-start caches; ADR-0017 remains binding unless superseded by
  a separately evidenced decision.
- Stage-2 parallel export identity, stable structural hashes, incremental checking, and language
  service work.
- Namespace binder refactoring; its active sprint was gated on this cutover closing and is now
  unblocked, subject to its required fresh-HEAD re-verification.

## Decisions

- The old sprint is historical evidence, not an active contract.
- No more collision-route implementation lands before WU2 chooses the freeze boundary.
- `f2e5bc2` landed the provider-backed driver and route attestation before WU3 closed. This is a
  partial WU4 cutover, not an atomic or complete one. No further WU4 implementation lands until
  project-global semantics and full conformance close WU3.
- WU8's default claim is the four-row full-library cutover claim, not universal checker speed.
- A WU2 pivot requires a superseding ADR; an experiment alone cannot overrule ADR-0020.

## Sequencing

WU0 → WU1. WU2 may run read-only alongside WU1 implementation/review. WU2 then selects exactly one
WU3 path. WU3 → WU4 → WU5 → WU6 (initial NO-GO) → WU6A → WU5 rerun → WU6 rerun → WU7 are serial
gates. No authoritative timing begins before semantic, production-acceptance, official-suite,
differential, package, and CI gates are green. A bounded non-authoritative remediation probe may run
after a WU6 NO-GO, but it cannot support a product claim.

## Run log

- **2026-08-02 — WU0 retirement baseline.** `cargo test --test conformance -- --nocapture` under a
  two-vCPU lease completed 574 files with 9 differences in four cause clusters. The old sprint was
  archived explicitly incomplete; no production or test worktree file was included in the docs
  transition. The agent-docs lint reported its existing 21 hard findings (20 historical deleted-
  backlog links and the stray `docs/AGENTS.md`); no finding targets the new or moved sprint.
- **2026-08-02 — WU1 shipped.** Commits `914b66d` and `556f084` pin recoverable Oxc diagnostics,
  parser panics, all three public check shapes, the CLI, file ordering, and valid shared/private
  controls. Commit `f2e5bc2` rejects every parser diagnostic or panic before collision census and
  semantic continuation, then reparses only to return canonical ordinary parser output. The
  implementation also makes provider initialization and driver infrastructure distinct typed failures
  and, more broadly than WU1 required, switches the public driver, CLI, `library-info`, and official
  batch to the full-library provider. It does not complete WU4's prelude deletion or unified
  conformance cleanup. The isolated candidate
  passed all seven parse-routing tests, all 25 collision-preflight tests, provider lifecycle 5/5,
  CLI fault routing 5/5, the unchanged ES5 readiness and library-owned-record oracles, and
  `cargo check --all-targets`. Independent adversarial review passed with no high or medium finding
  and confirmed no dependency on parked WU3 or local-`Array` work. The boundary deliberately follows
  Oxc parser diagnostics: `tsc` classifies the recoverable TS1063 witness differently and is not the
  WU1 diagnostic-phase oracle.
- **2026-08-02 — WU4 boundary correction.** A supervisor audit found that `f2e5bc2` had begun WU4
  while its commit title and the run log described only a minimal WU1 dependency. HEAD's
  `production-default-library` value is truthful for each emitter: the CLI, `library-info`, and
  official batch all acquire `LibraryBaseProvider` through the public driver. HEAD is nevertheless
  mixed: `src/lib.rs` still re-exports raw checker functions that bootstrap `prelude.ts`, and
  conformance intentionally calls their test-support equivalents for `FixtureBase::Prelude` rows.
  The existing WU4 source-inventory acceptance remains RED and is the binding single-universe gate.
  WU4 is therefore restated as completion of an already-started cutover, and further WU4 work is
  paused behind WU3.
- **2026-08-02 — WU2 retained ADR-0020.** Two independent read-only audits found a project binder
  barrier but no complete semantic-publication barrier before body checking. Unannotated variable,
  function, namespace-group, and class-field meanings still depend on initializer or body inference;
  the frozen runtime is extracted only after those effects complete. The existing complete-combined
  source compiler is already the correctness probe for a library-plus-user universe, but it starts
  from a new intrinsic-only interner and cannot preserve the process-wide library `TypeId`s or the
  persistent official-suite batch's shared semantic base. Preserving only unaffected rows while
  replacing the affected closure is ADR-0020 itself. A later freeze would therefore require a new
  publication architecture or lose sharing, so the hard stop fired: no experimental path or
  superseding ADR is warranted, and WU3 proceeds through the sparse collision epoch.
- **2026-08-03 — WU3 shipped.** Commit `49e9c0d` pinned the global-publication review failures;
  `781f1b8` and `5910ed3` corrected their scope without rebaselining, and `e91f294` retired a stale
  driver oracle. Independent review rejected eager prefill, raw checker access, and a dotted-name
  panic. Commit `2974719` instead publishes legal global contributors incrementally in source order.
  The resulting gates passed 824 checker tests, 105+1 binder tests, and all 625 conformance files.
  Randomized differential testing found zero differences across 1,200 cases; its `412f321` negative
  control fired on 92 of 400 cases.
- **2026-08-03 — WU4 shipped.** Commit `b5988e0` removed the production prelude, raw root facade,
  and `FixtureBase`; retained fixture support is test-only, while conformance now uses the public
  driver. It also aligned the living docs and workspace inventory and removed the release-build
  warning. Source inventory passed 6/6, parse routing 7/7, provider lifecycle 5/5, full conformance,
  warning-free release build, production acceptance, and strict ReasonChain/profile parser tests.
  The `.d.ts` exit correction from 0 to 3 records backlog 15's stale oracle: unsupported semantics
  fail closed rather than succeeding silently.
- **2026-08-05 — WU5 official-suite ratchet audited.** The committed scoreboard had been stale
  since `e3f622e`. The full 874-file run changed two aggregate headers and 175 test rows; the old
  ratchet reported 48 regression events and 112 progress events, with three rows carrying both.
  Three independent read-only audits assigned every changed test row to one exclusive cause family:
  23 option-variant and 22 stable-variant oracle corrections (`c926873`), 9 parser-first routes
  (`f2e5bc2`), 14 BOM line-fidelity rows (`02dc7bd`), 5 RegExp-oracle retirements (`bf1d0a7`),
  33 full-library cutover/status rows, 1 mapped-binder unresolved row (backlog 63), 20 flow/type-guard
  rows, 16 Object/type-relation rows, 5 union/callable rows, 23 other library/member-surface rows,
  and 4 current-WU5 rows. The exact pre-WU5 report changed only those last four, all progress:
  `intersectionTypeInference3.ts` became in-scope and clean, while three string-interface files each
  lost one false positive. The 53 `IN`-to-`IN` rows are byte-identical to the pre-WU5 binary.
  Two historical rows lose five matched tsc diagnostics, but neither is unowned: the pinned
  `flow/unassigned-redundant-guard-cascade` under-report belongs to backlog 47 and
  `narrowing/assigned-or-never-alternate` belongs to backlog 51. The retained-unsupported audit lost
  no diagnostic or incomplete identity and found no error-type success channel. The scoreboard was
  saved with ordinary `--save`, never `--rebaseline`; a fresh full `--check` then reported zero
  regressions, zero progress, and complete corpus/scoreboard membership.
- **2026-08-05 — WU5 driver coverage and remote CI closed.** The WU6 harness exposed a driver
  assembly bug that could turn missing project results into false-clean reports. Commits `b3ec832`,
  `ca5d1e2`, and `e26d856` pin the coverage, ordering, and route failure classes and make every
  missing or misindexed result fail closed. Commit `6de4977` separates the shared-route witness from
  lifecycle tracing. GitHub Actions run `31009015017` passed every job; the later probe-parser fix
  at `4694880` also passed every job in run `31026499352`.
- **2026-08-05 — WU6 authoritative run stopped NO-GO.** Commit `2d3a73f` retains the raw evidence.
  Across its three windows, fast-clean and fast-errors remained faster than tsgo at about
  `1.05x`–`1.08x`, but collision and fanout were only `0.56x`–`0.58x`; p95 failed in the same
  direction. Both private rows ultimately took the full-source fallback. The binding all-row gate
  therefore stopped closure without changing the contract or relabelling either row.
- **2026-08-05 — complete-source remediation probe passed conditionally.** Commits `51171fc` through
  `4694880` specify, implement, harden, and independently review a non-authoritative one-pass probe.
  Its validated evidence (SHA-256
  `a2f1f0be310ec56952e42476a095ac7956407faa1fedceabfba5a9060c22c5ad`) was `PROMISING`: all four
  one-pass/tsgo median, p95, and bootstrap lower bounds exceeded `1.00`, with median speedups
  `1.1457`, `1.1484`, `1.1591`, and `1.1364` and 84 MiB median RSS. The probe itself is not
  project-correct: `m29_modules/basic_named` proves that it discards import dependencies and emits
  false `TK2304` diagnostics. The accepted direction is therefore the lifecycle measured by the
  probe behind the real production frontend, never promotion of the example. →
  [`ADR-0021`](../decisions/0021-use-complete-source-compilation-for-standalone-cli-checks.md).
- **2026-08-06 — post-promotion WU6A falsifier passed.** Commits `e086f59` and `2b373cf` pin and
  remove only an unrequested test-only serialization of the complete 559-record product census;
  the semantic compile and the production census witness remain intact. A controlled direct run
  fell from about 0.75 s to 0.29 s. The retained collector then ran from exact HEAD `2b373cf` after
  separate `cpu-lease run -n 2 -- cargo build --release --bin typokat` and
  `cpu-lease run -n 2 -- cargo build --release --example one_pass_probe` builds, each serialized by
  `/tmp/typokat-perf.lock`. The exact collector was `cpu-lease run -n 4 --no-smt -- python3
  tooling/one-pass-probe/one_pass_probe.py run --production target/release/typokat --one-pass
  target/release/examples/one_pass_probe --tsgo tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc
  --output /tmp/typokat-one-pass-probe-2b373cf.json`, under the same lock. Raw evidence has SHA-256
  `d6132774fcd81edb3d8d07b2fe1df4f348ec69b00cc67668c0418f58636bc078`; production, one-pass,
  and tsgo binary SHA-256 identities are respectively `cc756f26ff4dac30eaed0d66865b2447f2608db901d98e38434fb8074b9b262e`,
  `aaf2a0010b368b375bf712befdaa735f295fb46fdbc1413f7a6c330f6d3f4bce`, and
  `4f2de678286401759b3fb4475bafe35b8f32b4b3a07d92642bbf37eadc9b34a4`. Every all-row O/T
  median, p95 ratio, and bootstrap lower bound passed: fast-clean `1.2475/1.1770/1.2277`,
  fast-errors `1.2326/1.2895/1.2150`, collision `1.2286/1.2959/1.2073`, and fanout
  `1.2674/1.3430/1.2543`. Production/tsgo was also above 1.00 throughout, but remains
  non-authoritative evidence. The unchanged historical collector correctly stored
  `NOT-PROMISING` because post-promotion production is 3–5% faster than its discarded test
  example, so O/P no longer answers a release decision. The probe is retired unchanged; its raw
  verdict was not relabelled, while WU6A's all-row falsifier is PASS. Authoritative WU6 remains
  mandatory.
- **2026-08-06 — WU5 exact rerun passed at `176423c`; first authoritative retry exposed a
  collector bug.** All local gates were green. GitHub Actions run `31114984298` passed all eight
  push-relevant jobs; scheduled tsc truth mode was skipped by event and is not claimed as executed.
  The first authoritative retry completed semantic, control, and timing work, then stopped before
  verdict because the collector treated GNU `time`'s nonzero-status message as compiler stderr. It
  wrote no JSON; its partial assets were retained.
- **2026-08-06 — memory parser hardened spec-first; exact-commit CI blocked externally.** Commit
  `b2d1bfb` pinned the parser contract, adversarial boundary commit `23bbfc4` established RED, and
  `d1aa6d4` implemented the correction. All 62 Python tests passed, independent review passed, and
  a live GNU `time` memory probe passed. Exact-`d1aa6d4` GitHub Actions run `31118462286` did not
  provide code evidence: setup-stage jobs hit `Service Unavailable`, and both overall attempts were
  then invalidated/cancelled during the documented GitHub Actions critical outage. The user
  explicitly approved running WU6 before remote CI recovered while retaining exact-`d1aa6d4`
  remote CI as a closure gate.
- **2026-08-06 — authoritative four-row WU6 rerun returned GO.** From a clean `d1aa6d4`, the
  trusted collector ran under `flock -w 3600 /tmp/typokat-perf.lock` and
  `cpu-lease run -n 4 --no-smt`:
  `python3 tooling/full-lib-bench/full_lib_bench.py run --typokat target/release/typokat --tsgo
  tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc --output
  tooling/full-lib-bench/evidence/candidate-d1aa6d4.json --window-label
  2026-08-06-complete-source-retry-window-1 --window-label
  2026-08-06-complete-source-retry-window-2 --window-label
  2026-08-06-complete-source-retry-window-3`. Commit `f70e587` retains the GO artifact at
  `tooling/full-lib-bench/evidence/candidate-d1aa6d4.json`, SHA-256
  `a120f159bb6bb68253dd6df80d03c1e035bf69a860947d79c2cdb781e82dda7a`. The attested SHA-256
  identities are typokat `87a5c653815a5667a4461a3e4a62683f3bdcef873a5a250828cd86127f0d23b1`,
  comparator `4f2de678286401759b3fb4475bafe35b8f32b4b3a07d92642bbf37eadc9b34a4`, profile
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`, and contract
  `7162d237cdfaf55dae562979ed76df6567b172db23165eb586d0912a6a974acb`. Across exactly the four
  approved rows (`fast-clean`, `fast-errors`, `collision`, `fanout`), the weakest median speedup was
  `1.277993`, weakest p95 ratio `1.256933`, and weakest bootstrap lower bound `1.268481`; all 12
  window/row cells exceeded `1.25` on all three metrics (36 metric values). Typokat median RSS was
  about 75 MiB versus about 98–100 MiB for tsgo, with worst RSS-max ratio `0.74562`. The evidence
  inspector passed at measured parent
  `d1aa6d4` under the identical lease. This is only the four-row full-library cutover claim, not a
  general checker-performance claim.
- **2026-08-06 — WU7 independent review is CONDITIONAL PASS.** A fresh review at `f70e587` found
  zero HIGH or MEDIUM issues and confirmed that the user-approved out-of-order performance evidence
  remains valid. Only exact-`d1aa6d4` remote CI remains as the technical closure gate; any
  production, build, collector, or contract fix invalidates the evidence and requires a rerun.
  Closure-candidate commits `91bfaf7` and `760f46c` passed the focused 22/22 checks, formatting and
  diff validation; docs lint remains unchanged at 20 historical-link findings. Lifecycle closure
  still follows the remote gate; this is not final WU7 PASS or an OUTCOME.
- **2026-08-06 — local closure candidate validated at `365db4`.** Full `cargo test`, formatting,
  clippy with `-D warnings`, the release build, the `one_pass_probe` example, and the clean package
  gate all passed. The 874-case official-suite ratchet reported zero regressions and zero progress;
  differential repros were green, and the differential self-check completed 472/472 cases before
  its fixed time budget. The exact-`d1aa6d4` remote-CI outage remains the closure gate.
- **2026-08-06 — divergence-owner closure audit graduated at `8a16b10`.** Stale backlog-`14`
  aggregates were split among `48`, `51`, `75`, and new backlogs `107`/`108`. The disabled B51
  assignment-target evaluation-order spec pins tsc 6.0.3 to exactly `TS2339`; current typokat is
  silent. Independent review passed with zero HIGH, MEDIUM, or LOW findings. No scoreboard was
  rebaselined, and production, build, collector, contract, and active benchmark evidence remain
  unchanged. Focused manifest/divergences/readiness/surface checks passed 22/22, conformance marker
  parsers passed 2/2, formatting and diff checks were clean, and docs lint remained unchanged at 20
  historical-link findings. The exact-`d1aa6d4` remote-CI outage remains the sole technical closure
  gate; WU7 is not yet final PASS and no OUTCOME is declared.
- **2026-08-07 — exact remote gate and WU7 closed.** GitHub Actions run `31118462286`, attempt 3,
  verified exact head `d1aa6d4c5f99dd5b95260b6d90203af45a24300a`: all eight push-applicable
  jobs passed and the scheduled truth job was correctly skipped for the push event. The independent
  reviewer then returned final PASS with zero unresolved HIGH or MEDIUM findings. No production,
  build, collector, or contract change followed the authoritative measurement; lifecycle closure
  archives this sprint and backlog `14` without widening the four-row performance claim.
