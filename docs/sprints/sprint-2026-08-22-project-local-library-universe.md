<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU0 → <sha>,
> WU1 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> 113, 115, 116, and 121. Deferred: <honest notes>.
-->

# Sprint — project-local default-library universe (2026-08-22)

**Goal.** Replace the frozen semantic default-library base, collision preflight, and sparse replay
with one project-owned declaration and semantic universe that is sound under global augmentation
and demonstrably faster and leaner than the pinned native `tsgo` comparator.

**Theme.** Typokat is a research checker, not a compatibility wrapper around an architecture that
has already exposed its wrong ownership boundary. The default library may share immutable source
and declaration facts, but every binder identity, semantic identity, `TypeId`, terminal, cache,
global-object product, and diagnostic state belongs to the project whose complete global declaration
set gives it meaning. This sprint delivers that boundary end to end. A frozen semantic fast path,
collision classifier, sparse replay, or permanent hybrid is not an acceptable shipped outcome.

## Delivery contract

The target lifecycle is:

```text
validated packaged sources
        -> one immutable, AST-free declaration product
        -> project-local eager declaration binding and global merge
        -> project-local demand-driven semantic publication
        -> one project-local checker session and result
```

“Demand-driven” applies only after declaration discovery and binding. The implementation must know
and merge all library fragments, script globals, `declare global` fragments, `globalThis`
contributions, namespaces, and user declarations before any affected semantic terminal becomes
observable. Per-name lazy source loading is forbidden.

There is one production publisher: the checker `Pass`. The shared product is construction input,
not a semantic snapshot, binder checkpoint, replay recipe, cache, or alternate publisher. The old
provider remains only as a frozen control until the candidate passes the cutover gates; it is then
deleted in the same sprint.

The sprint has no performance-only fallback to sparse replay. If the candidate misses a gate,
profile and improve the project-local representation, binder, scheduling, or query granularity.
If a sound store-independent product or single-publisher implementation proves impossible, stop at
the design boundary, retain the failed evidence, and request a new architecture decision. Do not
ship a hybrid as a workaround.

## Refs re-verified at HEAD (`0ae9ddd`, 2026-08-22)

- ✔ `FrozenLibraryBase` owns a complete semantic runtime, replay plan, root-name census, frozen
  prefixes, identity, and retained private sources; its routed result has three semantic
  lifecycles — `crates/typokat-library/src/base.rs:101-152`.
- ✔ `LibraryBaseProvider` publishes that base once through `OnceLock` and returns cloned
  `Arc<FrozenLibraryBase>` handles — `crates/typokat-library/src/provider.rs:63-73,139-153,249-264`.
- ✔ One runtime state currently couples the interner, binder, published terminals, declaration
  types, semantic identities, checker metadata, ID counters, replay index, and private epoch —
  `crates/typokat-check/src/check/checker/library_compiler/runtime_model.rs:3-15`.
- ✔ Ordinary and sparse projects fork different layered views over that runtime; both preserve the
  frozen semantic prefix —
  `crates/typokat-check/src/check/checker/library_compiler/runtime_state.rs:101-141`.
- ✔ The standalone complete-source route already parses, binds, and publishes the packaged library
  plus one project without retaining a replay plan —
  `crates/typokat-check/src/check/checker/library_compiler/complete_source.rs:27-100`.
- ✔ The canonical declaration inventory is the sole source-declaration discovery authority, but
  its live rows still carry AST- and binder-local identities. `DeclarationSite` contains `ScopeId`,
  while `DeclId`, `TypeGroupId`, and `ValueStorageId` are dense universe-local IDs —
  `crates/typokat-binder/src/binder/declaration.rs:41-110` and
  [`ADR-0022`](../decisions/0022-use-one-binder-authenticated-declaration-inventory.md).
- ✔ `LibraryBinderCheckpoint` owns a complete `Binder`, is opaque, and is deliberately neither
  cloneable nor caller-constructible; it cannot be the shared declaration product —
  `crates/typokat-binder/src/binder/bind.rs:244-271`.
- ✔ Existing declared-type recipes are not store-independent: they embed `TypeId`, `TypeParamId`,
  and `DeclaredRecipeId` values — `crates/typokat-types/src/types/repr.rs:988-1018`.
- ✔ Cross-run stable type hashes remain a zero-digest placeholder and cannot authenticate or
  transport semantic identities — `crates/typokat-types/src/types/hash.rs:440-451`.
- ✔ The 2026-08-22 feasibility spike proved a 4,964-byte store-independent index for a real
  two-source/12-group slice, but it stopped before augmentation and performance gates. Making
  `Reflect` Ready exposed a false-clean oracle and activated 44 sealed DOM record owners on an
  Array collision; the prototype was removed —
  `docs/archive/sprint-2026-08-22-project-local-library-index-feasibility.md:405-428`.
- ✔ The authenticated repeated-provider receipt covers 687 official-suite frames: 395 private,
  247 shared, 42 parse-rejected, and three complete-source fallbacks. Initialization was 1.551 s
  and peak RSS 144,109,568 bytes. The receipt SHA-256 is
  `950d5d7b38da7e7b5a0bc87da7ed423968b151ebf731c451021216c1d1880a18` —
  `docs/archive/project-local-library-index-feasibility-2026-08-22/lifecycle-baseline.json`.
- ✔ The current cold benchmark already authenticates exact source inventories, diagnostics,
  binaries, timing, RSS, and the pinned native comparator over `fast-clean`, `fast-errors`,
  `collision`, and `fanout` — `tooling/full-lib-bench/README.md`.
- ⚠ The local SOTA note correctly rejects a *growing shared interner* and per-name lazy stdlib
  loading, but its rejection predates this project-owned semantic design. The retained lesson is
  eager declaration assembly with project-local semantic state, not frozen semantic reuse —
  `docs/ideas/sota-checker-lessons.md:45-64` and
  `docs/ideas/phpstan-architecture-lessons.md:25-38`.

## Fixed invariants

These conditions may not be relaxed after candidate results are visible.

### Semantic ownership

- The shared product may contain exact source/profile identities, owned strings, declaration kinds,
  stable source sites, module classification, slot/group facts, symbolic type syntax, dependency
  edges/SCCs, and deterministic lexical order.
- It may contain no AST node/reference, allocator lifetime, `Binder`, binder checkpoint, `Store`,
  `Interner`, `TypeId`, `TypeParamId`, `DeclaredRecipeId`, `SymbolId`, `ScopeId`, `DeclId`,
  `TypeGroupId`, `ValueStorageId`, class ID, semantic terminal, global-object product, event row,
  relation/evaluator/flow cache entry, replay owner, fingerprint baseline, or mutable cell.
- Every project allocates all binder and semantic identities locally. No numeric semantic identity
  is compared or transported between projects or between typokat and `tsgo`.
- Structural equality may remain integer equality only inside one project-local universe.

### Binding and publication

- Library and user declaration discovery completes before semantic publication.
- Every same-name global declaration group is assembled in canonical source order before its first
  query. Reverse user input order and reverse demand order must produce the same normalized result.
- A demand reserves the complete dependency SCC before lowering any member. The observable state is
  `Unresolved -> Resolving(SCC) -> Ready | Unavailable`; provisional results enter no durable cache.
- Only the existing checker `Pass` may publish semantic state or diagnostics. The shared product,
  binder, session, provider, and driver never patch terminals.
- Library validation roots affected by a user augmentation must be derived from the final
  project-local dependency graph. A baseline fingerprint copied from an unaugmented universe is not
  evidence.

### Product and lifecycle

- OXC programs remain inside their owning worker and allocator lifetime. Provider state is AST-free.
- User sources are parsed once per project. There is no separate collision-preflight parser or
  routing AST walk.
- Standalone and repeated checks use the same project-session semantic pipeline. They may differ in
  immutable product lifetime, never in semantic ownership or publication order.
- No production route branches on a library-name collision. There is no sparse, fallback, or
  collision-free semantic mode after cutover.
- The pinned TypeScript 6.0.3 source profile, package verification, exact source order, and library
  census remain binding.

## Work units

### WU0 — freeze the target corpus and decision contract (effort L)

- **Problem.** The stopped slice used a false-clean `Reflect` oracle and therefore mixed a semantic
  feature gap with the architecture experiment. Current collision tests prove sparse containment,
  not the target lifecycle.
- **Verify first.** Re-run real `tsc 6.0.3 --strict --target es2025 --noEmit` and the current
  complete-source route over the exact symbol/Reflect, Array/ReadonlyArray, collision, fanout,
  declaration-merge, `declare global`, `globalThis`, namespace/value, class/namespace, module, parse
  failure, and two-project isolation witnesses. Record every current mismatch before creating a
  candidate.
- **Scope.** Land a fixture/spec-only commit. Enable no candidate production path. Freeze:
  - the ordinary `symbol` callable namespace case and exact 44-owner DOM movement;
  - direct and module `declare global` augmentation in both file orders;
  - script globals and explicit `globalThis` contributions;
  - legal and illegal interface/class/function/namespace merges;
  - recursive generic group SCCs, bridge identities, and global-object keys;
  - collision-free controls, parse errors, source re-exports, and project isolation;
  - reverse source order, reverse demand order, and deterministic fresh-process receipts;
  - a forbidden-shared-content contract for the future declaration product.
  Write the proposed superseding ADR that fixes the target ownership and cutover rules without
  claiming that the implementation has shipped.
- **Acceptance / witness.** Every expected `tsc` outcome and current typokat divergence is explicit;
  existing production gates remain green; candidate-only tests fail solely because the new product
  and session do not exist. An independent adversarial review hunts false negatives and verifies
  the `tsc` baselines.
- **Touch points.** `tests/cases/`, library collision/combined-universe specs,
  `tests/wu6_complete_source_cli.rs`, official-suite fixtures, a new ADR, and this sprint.

### WU1 — establish the correct symbol and library-record truth (effort L)

- **Problem.** Ordinary `symbol` annotations withhold the `Reflect` terminal today. Enabling them
  changes 44 library-owned records, so the current frozen baseline cannot serve as the target
  oracle.
- **Verify first.** Run only the WU0 symbol/Reflect and 44-owner fixtures through `tsc`, the current
  complete-source compiler, and the disabled backlog-121 case. Confirm the exact lowering authority
  and the first semantic cause of every activated record.
- **Scope.** Lower `TSSymbolKeyword` through the ordinary type-syntax authority and project-local
  `well_known().symbol` identity. Resolve the resulting library model gaps in the complete-source
  compiler core. Do not teach sparse replay about the movement, copy fingerprints, bless new
  incompletes, or add a namespace-specific shim. Enable the backlog-121 conformance fixture only
  when the complete-source oracle is correct.
- **Acceptance / witness.** `Reflect` is Ready with the exact `Reflect.ownKeys` surface; the fixture
  emits the expected `TK2322`/`TK2345`; the pinned library census is reviewed and exact; the 44
  owners have the same clean meaning as `tsc`; ordinary complete-source and cold CLI gates pass.
  Independent review traces the symbol identity and searches for a hidden sparse-only fix.
- **Touch points.** Callable namespace lowering, `WellKnownTypes`, library records/census,
  `tests/cases/b121_symbol_namespace_surface/`, complete-source specs, and backlog 121.

### WU2 — build the full-profile immutable declaration product (effort XL)

- **Problem.** The canonical manifest borrows AST/binder identities, the binder checkpoint owns a
  complete binder, and existing type recipes embed store-local IDs. None is safe process-wide
  construction input.
- **Verify first.** Inventory every declaration/type-syntax form, source site, module classification,
  declaration group, namespace/value attachment, dependency edge, and lexical event required by all
  82 packaged files. Compare the inventory with ADR-0022 and the current complete-source binder and
  library census. Freeze counts and canonical ordering in a spec commit before implementation.
- **Scope.** Introduce one owned `LibraryDeclarationProduct` (name not binding), constructed once
  from the canonical declaration inventory while its AST is alive. It stores symbolic references
  by stable product-local keys and owned syntax recipes sufficient for later binder and semantic
  lowering. It must not retain a second near-complete syntax tree. Add deterministic canonical
  rendering/hash only as a test/audit witness; runtime correctness must not depend on serialization.
  Make invalid or unsupported full-profile syntax a typed initialization failure.
- **Acceptance / witness.** The product covers all 82 files and exact source/profile identity;
  repeated builds, reversed enumeration, and a fresh process produce the same digest and census.
  Compile-time/private-field tests and an explicit walker prove the forbidden shared types are
  absent. Product creation builds the canonical declaration inventory once and retains no AST or
  allocator. A negative control for each forbidden ownership class fires. Independent review must
  reject any renamed binder checkpoint, replay plan, semantic snapshot, or second AST.
- **Touch points.** `typokat-binder` declaration inventory, a small store-independent product module,
  `typokat-library` profile/provider initialization, owned symbolic type syntax, and product specs.

### WU3 — assemble one eager project-local declaration universe (effort XL)

- **Problem.** Global and ambient meaning cannot be resolved correctly until library and user
  fragments share one binder. The existing provider instead publishes library identities first and
  classifies later writes as shared or colliding.
- **Verify first.** Freeze complete-source binder projections for all WU0 projects: scopes, slots,
  groups, fragments, namespaces, global augmentations, module edges, global-object contributors,
  source ownership, and canonical order. Add one-parse and no-semantic-allocation witnesses.
- **Scope.** Introduce a frontend-owned `PreparedProject` that retains parsed user programs only
  inside one worker. Instantiate a fresh binder from the immutable library product, then bind the
  prepared user programs into it before semantic construction. Allocate all IDs locally. Remove
  collision concepts from the candidate path. Build final global groups, namespace/value
  attachments, bridge lookup identities, and global-object contribution inventory without
  materializing semantic types.
- **Acceptance / witness.** Every normalized binder projection matches the complete-source oracle in
  both input orders. Different projects receive different numeric identities but equivalent
  structure; augmentations never leak. Each user source parses exactly once. Candidate project
  construction allocates no `TypeId`, semantic terminal, event, or semantic cache entry. Full
  82-file binding work, wall time, allocations, and peak live bytes are recorded before WU4.
- **Touch points.** `typokat-frontend`, canonical declaration inventory, binder builder/namespace
  tables, global augmentation, project driver preparation, and backlog 115.

### WU4 — publish project-local semantics on demand (effort XL)

- **Problem.** An eager project-local binder fixes ownership, but eagerly compiling the complete
  semantic library for every project would discard the repeated-work advantage. A demand-driven
  implementation must remain atomic across recursive groups and diagnostic dependencies.
- **Verify first.** Freeze semantic demand roots and dependency SCCs for WU0, the complete packaged
  library census, and representative official-suite frames. Measure which owners the current
  complete-source route constructs and which user queries actually consume.
- **Scope.** Add the project-local terminal state machine and symbolic-to-local lowering. Reserve an
  entire dependency SCC before resolving it; allocate type parameters, classes, values, bridges,
  and global-object products in deterministic project-local order. Cache only final Ready or typed
  Unavailable results. Route user checking, library validation roots, evaluator/projection,
  substitution, relation, flow, and diagnostics through the same coordinator. Provide a test-only
  `force_all` control that is never reachable from the ordinary session path.
- **Acceptance / witness.** Demand and source order do not affect normalized results or stable source
  ownership. Repeated demand is ID-stable inside one session. Recursive SCCs become atomically Ready
  or Unavailable. No resolving value enters a durable cache. Ordinary cases perform strictly less
  semantic owner work than `force_all`; the exact counts are reported, not inferred from timing.
  `force_all` matches the complete-source library census and semantic probes. Known-broken controls
  catch an omitted edge, premature terminal, provisional cache write, and stale global-object
  product.
- **Touch points.** Checker type-group/class/value publication, query coordination, library semantic
  lowering, bridges/global object, event ownership, evaluator/relation/flow caches, and focused
  specs.

### WU5 — unify standalone and repeated checks behind `CheckerSession` (effort XL)

- **Problem.** Driver and library crates currently expose low-level frozen, routed-private, fallback,
  and binder-continuation states. Keeping those beside the candidate would make two permanent
  semantic authorities.
- **Verify first.** Inventory every public and documentation-hidden lifecycle API, caller, route
  receipt, worker boundary, parser failure order, module report order, package/profile probe, and
  test injection seam. Freeze API compatibility decisions for the pre-1.0 crate.
- **Scope.** Add one high-level session owner. Provider initialization validates sources and builds
  only the immutable declaration product. Every check creates one project-local session and drops
  all mutable state afterward. Route `check_source`, `check_files`, `check_project`, standalone CLI,
  official batch, and test injection through the same semantic pipeline. During migration, a
  test-only shadow harness may execute the old route and compare normalized results; no public flag
  or production call-order switch may select it.
- **Acceptance / witness.** All public operations preserve parse/panic precedence, report order,
  project notices, diagnostics, incompletes, and exit status. Concurrent sessions share the exact
  product `Arc` and no mutable state. There is one source parse, binder, semantic publisher, and
  event domain per project. Low-level lifecycle types cease to cross crate/public boundaries.
  Independent API/architecture review finds no second production route.
- **Touch points.** `typokat-library` provider/public exports, `typokat-driver`, frontend worker,
  checker entry points, CLI/library-info, official batch protocol, and backlog 116.

### WU6 — prove superiority over `tsgo` and optimize the correct model (effort XL)

- **Problem.** Cold CLI evidence exists, but there is no apples-to-apples repeated isolated-project
  comparator for the new session lifecycle. Comparing a typokat batch with repeated `tsgo` process
  startup would overstate the result.
- **Verify first.** Freeze the candidate/control commits, exact binaries, TypeScript 6.0.3 semantic
  oracle, staged native comparator identity, 82-file inventory, workloads, CPU lease, affinity,
  environment, sampling schedule, and current authenticated 687-frame baseline. Declare all gates
  before the first candidate timing run.
- **Scope.** Extend the authoritative benchmark with two panels:
  1. the existing cold `fast-clean`, `fast-errors`, `collision`, and `fanout` process comparison;
  2. a pinned in-process `tsgo` batch runner built from the authenticated upstream source, creating
     one isolated Program/checker per frame and sharing only the immutable parse/source products its
     real architecture permits.
  Authenticate the runner source, binary, exact flags, file inventory, per-frame isolation, output,
  and memory. Add 1-, 32-, and 687-project panels. Instrument typokat product initialization,
  project binding, semantic owners materialized, checking, teardown, allocations, and peak RSS.
  Profile and optimize only the project-local product/binder/query path; do not revive frozen
  semantics or sparse replay.
- **Acceptance / witness.** Before timing, typokat, `tsc`, and the pinned `tsgo` comparator agree on
  every admitted semantic/control oracle. Under `cpu-lease run -n 2 --no-smt --`:
  - every cold row has typokat/tsgo median speedup, p95 speedup, and one-sided 95% bootstrap lower
    bound strictly greater than `1.00`;
  - the 32- and 687-project isolated batch panels satisfy the same three strict speed gates;
  - typokat median peak RSS is no greater than `tsgo` on every binding panel and every sample remains
    below 512 MiB;
  - the candidate also beats the authenticated current typokat provider baseline on total 687-frame
    wall time and peak RSS;
  - 1/2/32-session concurrency is deterministic, memory-bounded, and contains no cross-project
    identity or cache leak.
  Raw evidence is retained with binary/source hashes. An independent reviewer recomputes every
  verdict and inspects the comparator for an unfair lifecycle.
- **Touch points.** `tooling/full-lib-bench`, a pinned `tsgo` batch comparator/collector, official
  suite batch tooling, session metrics, and immutable evidence under `docs/archive/`.

### WU7 — cut over and vertically delete the frozen/sparse lifecycle (effort XL)

- **Problem.** A successful candidate left beside the old provider would preserve the architecture
  knot and make future features support two ownership models.
- **Verify first.** Freeze the complete candidate/control parity and benchmark evidence. Enumerate
  every frozen/sparse/collision production type, field, method, test, route string, metric, source
  registry, replay plan, override table, and public export. Identify general layered containers that
  still have non-library consumers before deleting them.
- **Scope.** Make `CheckerSession` the sole production route. Supersede ADR-0011's frozen semantic
  base, ADR-0013/0014's private replay/checkpoint boundaries, ADR-0020's sparse epochs, and
  ADR-0021's split semantic lifecycle while preserving ADR-0017's exact source-compiled profile.
  Delete `FrozenLibraryBase`, collision preflight/routing, `CollisionReplayPlan`,
  `PrivateCollisionEpoch`, sparse overrides, replay source retention, record-fingerprint replay
  validation, complete-source fallback automata, and lifecycle-only permits/metrics/tests. Delete
  `fork_sparse_delta` implementations and sparse container support only where no other live consumer
  remains. Update architecture, invariants, public docs, route probes, package tests, and examples.
- **Acceptance / witness.** Production contains one library/project semantic lifecycle and no
  collision-name branch. `rg` finds no unexplained frozen/sparse/replay production residue. The
  exact WU0 corpus, full unit/workspace suite, clippy, ordinary `580/696` conformance, complete
  shared/cold parity replacement, official-suite ratchet, 44-owner witness, library census, package
  verification, and WU6 benchmark evidence all pass at the same reviewed tree. Backlogs 113, 115,
  116, and 121 are deleted; the sprint is archived with commit map and exact measurements.
- **Touch points.** Library/checker/binder layered storage, provider/driver/CLI, all lifecycle specs,
  ADRs, `docs/reference/`, backlog/index files, and final archived evidence.

## Out of scope (explicit)

- Per-name or per-file lazy loading of `lib.d.ts`. Declaration discovery and global merging are
  intentionally eager.
- A shared growing semantic interner, cross-project `TypeId`, semantic snapshot, persisted semantic
  cache, or serialized binder.
- General incrementality/Salsa adoption from backlog [`17`](../backlog/17-incrementality.md).
  The session query coordinator may establish compatible boundaries but must not turn this sprint
  into the IDE incremental engine.
- Parallel cross-file type universes from backlog [`16`](../backlog/16-parallelism-type-universe.md).
  WU6 tests concurrent isolated sessions; it does not implement Stage 2 parallel checking within
  one project.
- Alternate `--lib`, target, host, package, or TypeScript profiles. The exact pinned ES2025 full-host
  profile remains the product.
- General checker completeness unrelated to a load-bearing full-profile declaration or semantic
  dependency. New unrelated gaps graduate to backlog rather than expanding this sprint.
- The active [`as const` sprint](sprint-2026-08-17-as-const.md). Its WIP remains preserved and
  paused while this lifecycle sprint owns overlapping checker seams.

## Decisions

- **Fix ownership, not collision handling.** The project-local universe is the selected production
  target. DELETE/RETAIN optimization of sparse replay is superseded.
- **Share declarations, never meaning.** The process-wide product ends before binder and semantic
  identity allocation.
- **Bind eagerly, evaluate on demand.** Ambient/global declaration groups are complete before any
  query; semantic SCCs are project-local and lazy.
- **One publisher and one lifecycle.** Shadow comparison is test-only and temporary. No feature flag
  or permanent hybrid ships.
- **Correct the oracle before architecture claims.** Symbol/Reflect and the 44 DOM owners land in
  the complete-source semantic authority before the new product consumes them.
- **“Better than tsgo” is a binding gate.** It means strict semantic/control agreement, strict cold
  and repeated wall-time wins with confidence, and no greater median peak RSS under authenticated
  comparable lifecycles.
- **Performance failures optimize the target model.** They do not authorize frozen semantics,
  sparse replay, or an unfair comparator.

## Sequencing

| Order | Unit | Parallelism |
|---:|---|---|
| 1 | WU0 target corpus/ADR | Leader-owned spec commit; independent adversarial review before implementation. |
| 2 | WU1 semantic truth | One implementation owner; independent `tsc`/false-negative review. |
| 3 | WU2 declaration product | One product owner; benchmark-comparator design may proceed read-only in parallel. |
| 4 | WU3 project binder | Starts only after WU2 product is frozen; frontend/binder writer territory is exclusive. |
| 5 | WU4 lazy semantics | Starts after WU3 binder projections pass; checker publication territory is exclusive. |
| 6 | WU5 session integration | Route cutover remains shadow-only until WU6. |
| 7 | WU6 benchmark/optimization | Comparator tooling may be implemented in parallel with late WU4/WU5 only from frozen interfaces; timing waits for a reviewed candidate commit. |
| 8 | WU7 deletion/closure | Serial after every semantic and performance gate passes. |

Each WU follows the mandatory loop: fixture/spec commit first, implementation through a subagent
with explicit write territory, narrow verification, independent adversarial review against real
`tsc`, integration, then an atomic explicit-path commit. CPU-heavy gates use `cpu-lease`; benchmark
numbers are invalid without `--no-smt`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
