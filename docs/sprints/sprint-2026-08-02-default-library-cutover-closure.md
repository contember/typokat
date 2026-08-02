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
  current CI job executes and passes. Any scoreboard movement is cause-classified and separately
  specified, never smoothed by rebaseline.
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
- Namespace binder refactoring; its active sprint remains gated on this cutover closing.

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
WU3 path. WU3 → WU4 → WU5 → WU6 → WU7 are serial gates. No authoritative timing begins before
semantic, production-acceptance, official-suite, differential, package, and CI gates are green.

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
