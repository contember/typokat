<!--
On close, prepend the final result block here, then `git mv` this file to ../archive/:

> **FINAL RESULT — PROCEED|STOP YYYY-MM-DD.** <bounded feasibility result and why.>
> Commit map: WU0 → <sha>, WU1 → <sha>, … Verification: <exact commands + receipts>.
> Backlog: 114 <rescope/delete>; 113 <keep/delete/rescope>; 115/116 edges updated.
> Production lifecycle: unchanged. Deferred: <honest notes>.
-->

# Sprint — project-local default-library index feasibility (2026-08-22)

**Goal.** Decide whether a store-independent symbolic library index plus project-owned demand
publication is feasible enough to replace `FrozenLibraryBase`, using a bounded
global-augmentation-first prototype and reproducible evidence without changing the production
library lifecycle.

**Theme.** The current sparse collision machinery is a consequence of publishing library semantics
before the user project's global declarations are known. This sprint tests the causal alternative:
share only immutable symbolic library facts, then let each project form and publish its own complete
global declaration groups. It consumes only the baseline-measurement part of backlog 113; it does
not first optimize or delete sparse replay.

## Decision boundary

This is a killable architecture spike with exactly two outcomes:

- **PROCEED.** The bounded prototype satisfies every semantic, ownership, and determinism gate
  below. Keep production unchanged, retain the evidence, and rescope backlog 114 to a full-profile
  implementation/cutover plan with its own performance thresholds and superseding ADR.
- **STOP.** Any hard gate fails. Remove the prototype, retain the minimal evidence and reason, and
  return to backlog 113 as the bounded way to simplify the existing frozen-provider lifecycle.

Neither outcome may delete `FrozenLibraryBase`, change a public driver route, claim full-profile
performance, or weaken an existing collision/augmentation witness in this sprint.

## Refs re-verified at HEAD (`28058cb`, tree `73bac707`, 2026-08-22)

- ✔ `FrozenLibraryBase` is an already-published semantic universe. It owns the runtime store,
  optional collision plan, root-name census, frozen prefixes, identity, and retained private replay
  sources — `crates/typokat-library/src/base.rs:101-116`.
- ✔ Provider-backed projects select `Shared`, `Private`, or `CompleteSourceFallback`; a private
  route performs collision preflight and attempts `fork_sparse_collision_epoch` before failing
  closed — `crates/typokat-library/src/base.rs:118-152,4088-4145`.
- ✔ `LibraryBaseProvider` compiles and publishes that base once through `OnceLock`; sharing is a
  repeated-process optimization, not the source/profile authority —
  `crates/typokat-library/src/provider.rs:63-73,139-152,249-265`.
- ✔ Standalone `check_project_once` already compiles one library-plus-project universe without
  acquiring the process-wide base, while provider-backed `check_project` acquires it first —
  `crates/typokat-driver/src/driver.rs:210-252,430-445`.
- ✔ The current binder checkpoint cannot serve as the symbolic index: it owns a complete `Binder`,
  is opaque, and is deliberately neither `Clone` nor caller-constructible —
  `crates/typokat-binder/src/binder/bind.rs:244-271,320-328`.
- ✔ The existing `DeclaredTypeRecipe` is not store-independent: its nodes and mapper embed
  universe-local `TypeId` and `TypeParamId` values —
  `crates/typokat-types/src/types/repr.rs:990-1018`.
- ✔ Legal `declare global` fragments already target one project-wide compilation-global scope,
  but value-bearing augmentation remains an explicit safe-direction gap under backlog 82 —
  `docs/reference/architecture.md:189-200` and
  `docs/backlog/82-declare-global-value-space.md`.
- ✔ The current repository has no authenticated repeated-consumer comparison that proves the
  frozen semantic base is worth its cross-cutting lifecycle. The only obvious in-repository
  repeated consumer is the official-suite batch —
  `docs/backlog/113-measure-sparse-collision-lifecycle.md`.
- ⚠ Backlog 114 previously waited for all of 113 and 115. That ordering optimized the current
  lifecycle before testing its cause. This sprint instead takes a read-only/current-route baseline
  from 113. Backlog 115 remains useful before production integration but is not required to decide
  whether the symbolic/materialization model works.

## Fixed feasibility contract

The contract is frozen before prototype implementation. A result visible during WU2-WU4 may not
relax it.

### Semantic gate

For the locked slice and corpus, the prototype must match the ordinary complete-source oracle on
normalized diagnostics, incomplete records, exit status, source order, declaration-group terminal
state, selected type displays, global-object keys, and stable source-site identities. A dropped or
added diagnostic, false-clean global surface, partial terminal, or order-dependent result selects
**STOP** immediately.

Backlog 82 behavior is parity, not new scope: value-bearing `declare global` forms must produce the
same exact outcome as the project-route oracle. The prototype must not claim support beyond that
observable result merely because it has a new materializer.

### Ownership gate

The shared experimental artifact may contain only immutable, store-independent facts: exact
profile/source identity, stable source sites, names and slots, declaration groups, dependency/SCC
edges, lexical-event order, and symbolic lowering recipes for the admitted slice. It must contain
no `TypeId`, `TypeParamId`, `DeclaredRecipeId`, `Store`, `Interner`, binder checkpoint, semantic
terminal, relation/flow cache entry, `CollisionReplayPlan`, or owner-site replay row.

Every prototype project owns its complete binder view, `Interner`/`Store`, semantic terminals,
global object, relation/flow caches, and publication state. It may share source bytes and the
symbolic index only.

### Publication and augmentation gate

All library and user fragments of one declaration group join before that group first becomes
observable. Publication uses an explicit project-local state machine:

`Unplanned -> Reserved -> Resolving(SCC) -> Ready | Unavailable`

Recursive groups expose one complete terminal or one typed unavailable terminal. No provisional
identity or type may enter relation, flow, diagnostic, bridge, or global-object caches. Reversing
source input order or library demand order must not change the terminal or output.

### Hard STOP criteria

Stop the design rather than inventing a compatibility layer if any of these is required:

1. clone, mutate, or overlay a frozen binder or semantic prefix;
2. reuse sparse affected-owner replacement as the materialization mechanism;
3. place universe-local type/parameter/recipe IDs in the shared index;
4. retain a second near-complete AST or a second semantic publication authority;
5. publish a library group before all known user augmentation fragments join it;
6. make diagnostics, events, identities, or global-object shape depend on demand/input order; or
7. keep the old and experimental lifecycle both as permanent production routes.

Performance and reachability observations may select **STOP** only after semantic and ownership
gates pass. The bounded slice cannot select production cutover or establish full-profile speed.

## Locked feasibility slice

WU0 freezes the exact source/group manifest before implementation. It must include real vendored
profile declarations rather than a hand-written substitute:

- the `Array<T>` / `ReadonlyArray<T>` groups and their admitted transitive type dependencies;
- their constructor/value bridge needed to observe ordinary array expressions and global lookup;
- one recursive interface dependency SCC;
- one namespace/value group already admitted by the checker; and
- global-object construction for the selected roots.

Synthetic user files may exercise the slice, but the library declarations and their source sites
must come from the pinned TypeScript 6.0.3 profile. If the exact transitive closure is too large for
a bounded spike, WU0 records that fact and reduces by a named declaration-form boundary, never by
silently dropping dependencies.

## Work units

### WU0 — freeze the acceptance corpus and symbolic boundary (effort M)

- **Problem.** A prototype written before its parity corpus can choose only the declaration forms
  that make the design look viable. The current collision tests prove the shipped route, not the
  absence of frozen semantics in a new route.
- **Verify first.** Inventory the selected real library groups, their exact source sites, slots,
  dependencies, SCCs, global-object contributions, and complete-source oracle outputs. Confirm the
  existing Array/global augmentation, `declare global`, explicit `globalThis`, randomized
  file-order, and complete-source parity witnesses still pass at HEAD.
- **Scope.** Land fixture/spec code only. Pin clean use, direct `Array<T>` merge, module `declare
  global` merge, unique global type, explicit global-object contribution, cross-file
  consume-before-augment and augment-before-consume, reverse demand order, recursive SCC, and
  backlog-82 value-form parity. Add a forbidden-content contract for the shared index and a
  source/group manifest for the locked slice. No candidate implementation lands in this commit.
- **Acceptance / witness.** The new tests fail only because the experimental index/materializer is
  absent, while all existing production-route and complete-source oracle tests remain green. The
  manifest is deterministic under reversed source enumeration and names every excluded dependency.
- **Touch points.** Focused fixtures/specs under `crates/typokat-library/src/`, existing
  private-combined/collision oracle helpers, and a small test-only feasibility manifest.

### WU1 — capture the current lifecycle baseline without simplifying it (effort S)

- **Problem.** A later full-profile decision needs current route frequency and cost, but executing
  all of backlog 113 first would invest in a lifecycle this spike may replace.
- **Verify first.** Authenticate HEAD/tree, release binary, pinned profile, official-suite corpus
  and scoreboard, allocator/environment, and an enforced CPU lease. Confirm instrumentation cannot
  alter official-batch stdout/stderr or route selection.
- **Scope.** Add the narrowest removable measurement seam that records base initialization,
  shared/private/fallback frame counts, per-route time, and peak worker RSS for one pinned complete
  official-suite discovery run. Record complete-source-once comparison receipts already available
  from the production standalone route. Do not build the 113 candidate, change collision routing,
  or delete sparse code.
- **Acceptance / witness.** One immutable baseline artifact identifies binary, commit, tree,
  corpus, profile, scoreboard and output hashes; reports every frame exactly once; and leaves the
  complete official-suite scoreboard unchanged. Benchmark numbers are valid only under
  `cpu-lease run -n 2 --no-smt -- ...`.
- **Touch points.** `crates/typokat-library/src/provider.rs`, official-batch test-only metrics,
  `tooling/official-suite/`, and sprint evidence stored with the eventual archive.

### WU2 — build a store-independent symbolic index for the locked slice (effort L)

- **Problem.** Current binder checkpoints and declared recipes embed project-universe state, so
  neither can be relabelled as a lazy index without preserving the frozen architecture.
- **Verify first.** Freeze a forbidden-type/import census for the experimental shared artifact and
  prove its IDs are derived from exact profile/source/group identity rather than allocation order.
- **Scope.** Parse and inventory the real packaged sources into immutable symbolic declarations,
  groups, dependencies/SCCs, slots, source/event order, and the minimum lowering recipe algebra for
  the locked slice. Keep this behind an experimental/test-only boundary. Reuse the canonical source
  declaration inventory and ordinary lowering semantics where possible; do not create a second AST
  walker or copy production checker logic into tooling.
- **Acceptance / witness.** Repeated index builds, reversed source enumeration, and a fresh process
  produce byte-identical symbolic manifests. The forbidden-content test proves no universe-local
  semantic ID/state crossed the boundary. Independent review confirms the artifact is not a replay
  plan under different names.
- **Touch points.** Default-library profile/source inventory, binder declaration/group metadata,
  checker lowering entry points, and a bounded experimental index module.

### WU3 — materialize project-owned groups on demand (effort L)

- **Problem.** The experiment is meaningful only if a fresh project can turn symbolic groups into
  ordinary semantics without inheriting a frozen prefix or introducing a second publisher.
- **Verify first.** Construct two empty project runtimes from the same index and prove every binder,
  store, identity table, cache, terminal, and global object is independently owned before demand.
- **Scope.** Implement the explicit project-local publication state machine for the admitted group
  forms. Reserve the complete selected dependency SCC before lowering; translate symbolic generic
  and nominal identities into project-local identities once; publish through the checker's existing
  semantic authority; and cache only final `Ready`/`Unavailable` products locally. Add `force_all`
  for the locked slice as an oracle/control, not a hidden eager fallback.
- **Acceptance / witness.** Two projects demanding the same symbolic group receive structurally
  equivalent but universe-local semantics; repeated demand is pointer/ID stable within one project;
  recursion terminates atomically; and no relation/flow/diagnostic query can observe a resolving
  group. `force_all` matches the locked complete-source census and semantic probes.
- **Touch points.** Checker library compilation/publication, binder project construction, type
  interning, class/interface/namespace state, bridge/global-object construction, and focused specs.

### WU4 — prove global augmentation before ordinary fast-path claims (effort L)

- **Problem.** A lazy library that handles only collision-free lookup recreates the frozen-base
  split. Global extensions are the feasibility test, not an edge case after the fast path works.
- **Verify first.** For each locked case, capture the complete declaration group before demand and
  prove all library plus user fragments are known. Fail closed if routing/publication begins first.
- **Scope.** Route the locked prototype corpus through one project-owned materializer with no
  collision classifier. Join direct global and legal `declare global` type fragments before group
  publication; rebuild selected global-object/bridge products from final terminals; preserve the
  exact project-route oracle for backlog-82 value forms. Exercise consume-before-augment,
  augment-before-consume, reverse file order, reverse demand order, and two isolated projects with
  different augmentations.
- **Acceptance / witness.** Every ordering matches the complete-source oracle exactly. Augmentation
  from project A never appears in project B. `Array<T>`, its constructor/bridge, and the selected
  global-object view all refer to one project-local merged group; no stale unaugmented terminal is
  observable or retained in a cache.
- **Touch points.** Experimental materializer, compilation-global binder/group join, global-object
  construction, complete-source oracle adapters, and augmentation fixtures.

### WU5 — evaluate reachability, cost, and architecture evidence (effort M)

- **Problem.** Semantic success alone does not show whether the approach can plausibly replace a
  repeated-process optimization. Conversely, bounded timing cannot justify production cutover.
- **Verify first.** Freeze candidate commit/tree, symbolic manifest, corpus, oracle hashes, release
  build, allocator/environment, and enforced no-SMT lease. Remove debug tracing from timed code.
- **Scope.** Measure index construction, per-project binder/runtime creation, demanded/published
  groups, dependency closure, semantic rows, local cache entries, wall time, and peak RSS for
  `force_all` and each locked demand pattern. Compare only within the exact bounded slice and report
  raw counts alongside timing. Audit shared-artifact ownership and the complete code path against
  every hard STOP criterion.
- **Acceptance / witness.** A deterministic evaluator emits exactly `PROCEED` or `STOP`, names every
  failed clause, and refuses unauthenticated/incomplete runs. An independent reviewer reproduces
  the verdict from raw evidence and hunts hidden eager fallback, shared semantic state, stale global
  terminals, and order-dependent output.
- **Touch points.** Test-only measurement/evaluator tooling, sprint evidence, symbolic ownership
  audit, and no production routing change.

### WU6 — close the feasibility decision and restore a single roadmap (effort S)

- **Problem.** A prototype that remains beside production without a decision creates the fourth
  lifecycle backlog 114 explicitly forbids.
- **Verify first.** Confirm the WU5 evidence/review verdict and list every experimental production
  touch point, temporary metric, public symbol, feature/flag, and generated artifact.
- **Scope.** On **STOP**, remove all prototype code and temporary instrumentation, retain only the
  minimal archived evidence/decision, delete or close backlog 114, and unblock backlog 113. On
  **PROCEED**, keep only bounded internal substrate that has independent value and full tests;
  otherwise remove the prototype and retain its specification/evidence. Rescope backlog 114 to the
  full-profile implementation, make backlog 115 a production-integration prerequisite if still
  required, and mark backlog 113 as obsolete or fallback-only. In both outcomes, leave production
  behavior unchanged and update all indexes/dependency edges.
- **Acceptance / witness.** No undocumented experimental route or metric remains; public API and
  production route attestations are unchanged; docs lint adds no failure against the authenticated
  pre-sprint baseline; the sprint has a closure header and moves to `docs/archive/`; and an
  independent closure review returns PASS.
- **Touch points.** This sprint, backlogs 113-116, docs indexes, any retained evidence, and the
  exact experimental files inventoried before cleanup.

## Out of scope (explicit)

- Production deletion or mutation of `FrozenLibraryBase`, provider routes, sparse replay, collision
  fallback, or complete-source standalone routing. This sprint only decides feasibility.
- Full 82-file semantic materialization and production performance acceptance. A PROCEED outcome
  must schedule those with fixed gates before cutover.
- Implementing backlog [`82`](../backlog/82-declare-global-value-space.md). Its current explicit
  project-route behavior must remain exact.
- The general lazy source-value/type resolver in backlog
  [`76`](../backlog/76-lazy-value-type-resolution.md); this sprint owns library declaration-group
  materialization only.
- The production parse-once integration in backlog
  [`115`](../backlog/115-prepared-project-single-parse.md). The prototype may consume a bounded
  prepared test product but may not broaden frontend architecture.
- Public session/API cleanup in backlog
  [`116`](../backlog/116-library-session-lifecycle-boundary.md), parallel type universes (`16`),
  incrementality (`17`), snapshots, persistent disk caches, packages, or alternate library profiles.

## Decisions

- **Test the cause before simplifying the workaround.** Backlog 114 feasibility now precedes the
  full DELETE/RETAIN work in backlog 113. Only 113's baseline observations are folded into WU1.
- **Pinned source profile survives every outcome.** The experiment questions the frozen semantic
  product, not the vendored TypeScript 6.0.3 sources or deterministic profile identity.
- **Global augmentation is the first semantic gate.** A collision-free-only lazy route is not an
  accepted intermediate architecture.
- **One publisher, project-owned state.** Symbolic facts may be shared; semantic identities,
  terminals, caches, bridges, and global-object products may not.
- **No production claim from a slice.** PROCEED means the architecture earned a full-profile
  implementation attempt, not that `FrozenLibraryBase` is approved for deletion.

## Sequencing

1. WU0 fixture/spec commit lands independently and receives adversarial review.
2. WU1 records the current baseline without changing routing.
3. WU2 builds only the symbolic boundary; review may stop the sprint immediately on hidden
   universe-local state or replay semantics.
4. WU3 implements project-local publication for the locked slice.
5. WU4 attacks global augmentation and ordering before any performance conclusion.
6. WU5 measures/evaluates only after semantic and ownership gates pass.
7. WU6 removes temporary state, records PROCEED/STOP, repairs backlog dependencies, and archives.

No parallel writer may touch the experimental symbolic/materialization boundary, library compiler,
binder group publication, global-object construction, provider/driver routing, or benchmark
protocol during WU2-WU5. Read-only independent review starts only from a frozen commit/evidence set.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- **2026-08-22 — sprint replaced before execution.** The prepared backlog-113 sparse lifecycle
  sprint was deleted before WU0. Reassessment identified frozen pre-publication of the library
  global universe as the cause of the collision lifecycle and made global augmentation the first
  feasibility gate. No sparse candidate, benchmark series, or production change from that sprint
  was executed.
- **2026-08-22 — docs-lint baseline authenticated.** Before the plan commit, the repository-wide
  agent-docs lint reported 21 pre-existing failures: twenty broken historical links under
  `docs/archive/` or `docs/decisions/`, plus the existing `docs/AGENTS.md` root-file policy finding.
  None names a changed sprint/backlog/index file. WU6 therefore requires zero new failures rather
  than broad unrelated archive cleanup.
- **2026-08-22 — WU0 spec drafted independently.** The acceptance corpus locks ten project shapes,
  both file orders, two demand orders, complete-source parity, the backlog-82 project-route tail,
  and real declaration fragments from pinned `lib.es5.d.ts` plus `lib.es2015.reflect.d.ts`. The
  library slice is the pinned source profile, never a handwritten semantic substitute. Candidate
  code remains absent until the spec is committed and reviewed.
- **2026-08-22 — WU0 independent review FAIL; remediation active.** The first corpus version pinned
  correct `tsc 6.0.3` truth and source offsets but did not executable-authenticate the manifest or
  candidate boundary. It also failed to force the Array/ConcatArray cycle, observe two-project
  isolation, preserve native Array members beside augmentations, or pin the exact backlog-82
  receipt. The revised spec adds those witnesses and will not commit until a second review passes.
- **2026-08-22 — WU0 oracle corrected from executable receipts.** The project-summary route is the
  relevant complete-source oracle. It reports the value-bearing `declare global` pair clean and
  preserves the existing `Reflect.ownKeys` false-clean, while `tsc 6.0.3` reports `TS2322` for the
  latter. The frozen spec records those observable results instead of the earlier inferred
  incomplete/diagnostic outcomes. Oracle receipt: 14 runs, digest
  `e8c41e6645c31e3d4aa084a484b5b1399cfd2c6b6451cd1cc135ae4a24ae0afa`, production tree
  `1061e8ae0e95060560bfc39c39cb18cb6bbb5202`, and release binary SHA-256
  `cbadd5c963c9e262418d1b267449e549ca67971a9367cbdc45ac7acf30d3093c`.
- **2026-08-22 — WU0 independent review PASS.** After three adversarial remediation rounds, the
  reviewer reproduced the 14-run oracle and expected RED candidate exit, then found no remaining
  blocker. The final contract independently authenticates complete library group records and user
  sites, carries the final fragment census on reservation, batches both project creation orders,
  reverses library/source/demand order, and checks project-local ownership plus atomic SCC
  publication. Existing production witnesses also passed: the complete-source `declare global`
  oracle, Array full-scan seed, script global-object routing, and unique cross-module global type.
- **2026-08-22 — WU1 lifecycle baseline PASS.** Removable nonce-authenticated sidecar metrics landed
  in `58b5b1a`; the immutable receipt landed in `a877c7e` with SHA-256
  `950d5d7b38da7e7b5a0bc87da7ed423968b151ebf731c451021216c1d1880a18`. The authenticated
  `cpu-lease run -n 2 --no-smt --` full discovery covered all 687 worker frames: 395 private, 247
  shared, 42 user-parse-rejected, three complete-source fallbacks, and zero preflight rejections.
  Initialization was 1.540 s compile, 11.263 ms publication, and 1.551 s total; peak worker RSS was
  144,109,568 bytes. The scoreboard ratchet remained `0/0/0/0`, pre/post commit/tree, binary,
  corpus, profile, scoreboard, frame-source, and CPU-lease identities matched, and the exact
  complete-source-once witness passed under the same lease. Independent review reproduced all
  counts, timing totals, hashes, route coverage, and the passing ratchet from the frozen evidence.
