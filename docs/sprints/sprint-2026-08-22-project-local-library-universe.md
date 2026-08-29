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
  `Reflect` Ready exposed a false-clean oracle. The removed prototype also reported 44 changed DOM
  record owners without retaining their names; WU0 could not reproduce that count under either a
  consistent symbol fix or a controlled old-base/new-candidate split, so it is not an oracle —
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
  - the ordinary `symbol` callable namespace case and the exact named library census before any fix;
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

- **Problem.** Ordinary `symbol` annotations withhold the `Reflect` terminal today. The stopped
  prototype reported a 44-owner movement but retained no named evidence, and a WU0 reconstruction
  did not reproduce it. A historical count cannot serve as the target oracle.
- **Verify first.** Run the WU0 symbol/Reflect fixtures through `tsc`, the current complete-source
  compiler, and the disabled backlog-121 case. Pin the complete named library census before the
  change, then derive any actual named delta without assuming its size or direction. Confirm the
  exact lowering authority and the first semantic cause of every real changed record.
- **Scope.** Lower `TSSymbolKeyword` through the ordinary type-syntax authority and project-local
  `well_known().symbol` identity. Resolve the resulting library model gaps in the complete-source
  compiler core. Do not teach sparse replay about the movement, copy fingerprints, bless new
  incompletes, or add a namespace-specific shim. Enable the backlog-121 conformance fixture only
  when the complete-source oracle is correct.
- **Acceptance / witness.** `Reflect` is Ready with the exact `Reflect.ownKeys` surface; the fixture
  emits the expected `TK2322`/`TK2345`; the pinned library census is reviewed and exact; the named
  census has a reviewed before/after delta (including an explicit empty delta if unchanged);
  ordinary complete-source and cold CLI gates pass.
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
- **Verify first.** Derive admitted product groups, resolved product dependency edges, publication
  SCCs, explicit demand roots, visit order, and the exact materialized-owner closure independently
  from the immutable declaration product plus the final project-local binder map. The legacy
  complete-source route is only a temporary test-only black-box shadow for normalized observable
  output; its replay owners, root consumers, statement owners, general SCC graph, and raw record
  ledger are not candidate authority. Before semantic lowering starts, amend the immutable product
  with one canonical qualified-path row for every Reference and Query, including single-segment
  paths, and every nonempty import qualifier, plus one exact source site per ordered segment.
  Preserve the binder's full typed resolution or failure cause and exact failing segment ordinal.
  Derive sites from the authenticated AST and outcomes from the live binder during product
  extraction; never infer sites from a recipe start plus name length, re-resolve outcomes later
  from product/local binder topology, or copy either from candidate or legacy semantic output.
- **Scope.** Add the project-local terminal state machine and symbolic-to-local lowering. Reserve an
  entire dependency SCC before resolving it; allocate type parameters, classes, values, bridges,
  and global-object products in deterministic project-local order. Cache only final Ready or typed
  Unavailable results. Route user checking, library validation roots, evaluator/projection,
  substitution, relation, flow, and diagnostics through the same coordinator. Provide a test-only
  `force_all` control that is never reachable from the ordinary session path. The candidate entry
  lives in the dedicated checker-owned `project_local/` tree and receives only the immutable
  declaration product, prepared project-local binder/program inputs, and request controls. The
  library provider and complete-source shadow are outside that capability boundary.
- **Recursive-alias boundary.** Implement
  [ADR-0024](../decisions/0024-represent-recursive-aliases-as-immutable-applications.md) before
  publishing any product SCC that contains a recursive alias. Allocate immutable project-local
  alias templates for the complete alias SCC, lower recursive references as lazy immutable
  instantiations, keep bodies in checker-owned draft/publication state, and unfold one outer layer
  through the semantic query coordinator. The general source checker and product lowerer consume
  the same guarded-recursion plan. Mutable Array/Tuple/Function shells, cyclic declared recipes,
  error-typed recursion, fabricated member surfaces, and Ready opaque aliases are forbidden.
- **Acceptance / witness.** Demand and source order do not affect normalized results or stable source
  ownership. Product-native group, edge, SCC, demand, terminal, cache, and exact materialized-owner
  traces match their independent product/binder projection. A passive scope outside the returned
  append-only audit records ordinary and `force_all` owner publication, cache writes, cache hits,
  fault activation, receipt construction, and publisher lifecycle with monotonic stamps. Its scope
  only reads rows appended after its captured cursor. Raw owner rows are authoritative: validation
  never sorts or deduplicates them and rejects a duplicate phase/group pair. Repeated demand is
  ID-stable inside one session, and every candidate call receives unique project and publisher
  identities. Cache writes and hits carry their publisher identity.
  Ordinary semantic probes are projected only after the product-native demand closure is fixed.
  Each admitted probe has independent evidence from one materialized product group through its
  exact final WU3 binder-map type or value slot and its visibility coordinate: global type, global
  value, or external-module `(source path, name)`. A shared name never crosses those coordinates.
  User final-global contributors may admit only global type/value coordinates; they never admit a
  module probe. Probe rows cannot add an owner, dependency, SCC member, or demand edge. `force_all`
  remains the unfiltered full semantic-probe census and keeps its frozen receipt unchanged.
  The implementation registers `Weak<SessionCore>` and `Weak<DurableCache>` rows from the actual
  owners; scope finish upgrades those exact weak references and requires both to be dead after the
  call. Drop appends its lifecycle event but never deletes the registry's weak row. The spec patch
  defines only the read-only liveness receipt. It does not fabricate mirror
  counters or pretend that the future weak hooks already exist; the lifecycle E2E remains RED until
  the implementation lands them.
  Recursive SCCs become atomically Ready or Unavailable. The unavailable fixture has an independent
  exact eight-root typed terminal census; every root proves a nonempty discarded provisional write
  set and an empty published set. No resolving or provisional unavailable value enters a durable
  cache. Ordinary cases perform strictly less passively observed product-owner work than test-only
  `force_all`; debug counts must match that receipt. The black-box shadow compares only user
  diagnostics, incompletes and check results, semantic probes, final global contributors, the live
  user EventStore ledger, and `force_all` library diagnostics/incompletes normalized by source
  path/span/content with legacy library event and record ordinals removed. Known-broken controls
  catch an omitted independently projected product edge, premature SCC terminal from the unique
  compilation-global product SCC whose names are exactly `Symbol` and `SymbolConstructor`,
  resolving cache write, escaped provisional unavailable write, stale global-object product, and an ordinary call
  to `force_all`, with an exact typed validator error and a stamped pre-receipt seam witness for each.
  Requests carry only the fault identity; the checker-owned fault gate defines its canonical seam.
  `PublishBeforeSccComplete` keeps the canonical Array demand roots unchanged. Their independently
  computed closure already contains the authenticated Symbol SCC, including its separate type and
  value product rows. A no-fault sibling uses those identical roots. The fault witness requires one
  exact premature commit on that SCC, clean commits everywhere else, and one canonical activation
  after every selected Symbol SCC owner is materialized and before that SCC's first durable-cache
  write and the receipt. It is the first of exactly two nontrivial SCCs in the canonical Array
  publication schedule; the second has the exact compilation-global mapped name set
  `ReadonlyArray` / `FlatArray` and must remain clean.
  The provisional-write fault runs only on `unavailable`, stale-global only on `global-this`, and a
  missing activation returns `FaultNotReached`. The boundary enumerates an exact set of relative
  paths, rejects every unexpected directory, file, or symlink, parses every admitted file as Rust,
  and applies a positive AST allowlist to modules, imports, paths, attributes, and macros. It rejects
  unsafe or foreign code, alternate source inclusion, thread/process/filesystem/network access, and
  any dependency from `typokat-check` to `typokat-library`. External checker/product types cross only
  the audited neutral `project_local_pass_port` adapter. Audit append hooks are private and may appear
  only in the actual session, terminal, cache, fault, and global-owner files.
  Supplementary external work scopes and provider metrics require zero profile load, library parse/bind,
  publication, replay, frozen-base, evidence, compiler, or source-reparse work on every candidate
  call, while the same scopes observe direct legacy calls. The source/capability boundary is the
  authority; it does not rely on a dynamic provider guard. The shadow route is deleted with the old
  lifecycle in WU7.
  Legal array, tuple, function, object, mutual, union-wrapped, and generic recursive aliases retain
  structural semantics in both source and product routes. Direct alias cycles remain typed
  Unavailable. Regular recursion terminates through the existing relation cycle stack without
  durable assumption-dependent writes; non-regular recursion reaches an explicit query budget.
  A documented unsupported signature member does not make an interface or object-signature group
  Unavailable when the complete-source route soundly omits that same member. This exception does not
  apply to class publication: class declaration SCCs retain their complete-surface atomicity
  invariant. A retained member is supported only when its complete signature and every referenced
  dependency lower without recovery. Otherwise the lowerer omits that member and stages the exact
  complete-source incomplete record, including surface, source path, span, and content. It never
  publishes a partial signature or substitutes a recovery `any`, `error`, `unknown`, or `never`.
  All retained members and all incomplete records finalize before the declaration SCC becomes
  atomically Ready. Type predicates remain backlog `50`, optional methods remain backlog `49`, and
  polymorphic `this` type annotations remain backlog `75`; WU4 implements none of them. This omission
  creates no owner, dependency, SCC-member, or demand edge, and request `ProductGroup` roots remain
  observationally inert. `IDBCursor` publishes its real supported `key` and `continue` members while
  preserving the recursive `IDBValidKey` semantics and the complete-source user ledger.
  Cold/warm and forward/reverse relation order must not change the verdict.
  `force_all` record parity has one producer: the production publisher. Under test-only
  `force_all`, the ordinary session demands every admitted product group through its existing
  lowerer, SCC coordinator, interner, store, aliases, terminals, and durable cache. It schedules the
  ordinary declaration validations for every Ready library group through the same `Pass`
  obligation path used for augmentation-affected library roots. Semantic probes are projected from
  those published terminals; no fresh semantic universe or second lowering run exists.
  The lowerer stages one typed incomplete record at every unsupported site it visits, including
  every site inside a member it then omits. Omission never truncates the record-producing member
  walk, publishes a partial signature, or substitutes a recovery `any`, `error`, `unknown`, or
  `never`. `force_all` returns the production-staged incompletes, production validation diagnostics,
  and one terminal row per product group: Ready with its observable surface census, or Unavailable
  with its typed reason. Exact sites remain grounded in the immutable product; constructor recipes
  retain their authenticated `abstract` bit and targeted metamorphic witness.
  No second evaluator, scratch `Store`, scratch interner, audit rule table, fixture replay, provider
  call, or recovery type exists in `project_local/`. The boundary proves one semantic construction
  owner and one recipe-lowering module, while behavior proves that ordinary and `force_all` requests
  use the same publisher. Acceptance is ADR-0027's named, attributed comparison against ADR-0026's
  558-row starting authority: retained rows are exact; every removal carries production evidence
  from an omitted member, typed Unavailable terminal, or documented single-report collapse; every
  added diagnostic has a named cause and divergence entry; any unattributed removal fails. The
  reviewed result becomes ADR-0018's census before WU7 deletes the legacy producer.
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
  user parse/prepare, project binding, typokat materialized product owners, checking, release, and
  peak RSS. Record exact typokat allocation calls only in a separate non-timing instrumented run;
  allocation counting must not alter the timed comparison.
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
  exact WU0 corpus, full unit/workspace suite, clippy, ordinary `581/697` conformance, complete
  shared/cold parity replacement, official-suite ratchet, symbol/Reflect truth, named library
  census, package verification, and WU6 benchmark evidence all pass at the same reviewed tree.
  Backlogs 113, 115, and 116 are deleted; the sprint is archived with commit map and exact
  measurements.
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
- **Correct the oracle before architecture claims.** Symbol/Reflect and the resulting named library
  census delta land in the complete-source semantic authority before the new product consumes them.
- **“Better than tsgo” is a binding gate.** It means strict semantic/control agreement, strict cold
  and repeated wall-time wins with confidence, and no greater median peak RSS under authenticated
  comparable lifecycles.
- **Performance failures optimize the target model.** They do not authorize frozen semantics,
  sparse replay, or an unfair comparator.
- **Recursive aliases are immutable lazy applications.** Alias declaration identity is
  project-local, bodies publish outside the store behind an SCC terminal, and queries unfold one
  outer layer. → [ADR-0024](../decisions/0024-represent-recursive-aliases-as-immutable-applications.md)

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

- **2026-08-22 — WU0 audit.** Existing B14/B43/B103 and CLI/batch tests already cover most merge,
  order, module, parse, and observable-isolation behavior. WU0 added only the missing packaged
  `Reflect`, Array publication-SCC, mixed direct-script + module-augmentation, and bidirectional
  project-isolation truths. Real `tsc 6.0.3` reproduced every marked diagnostic.
- **2026-08-22 — rejected historical count.** The stopped feasibility run retained the number 44
  but not the named library-record delta. An isolated reconstruction kept the baseline census at
  559 records under a consistent symbol fix and found zero missing/changed DOM TK2344 rows under a
  controlled old-base/new-candidate symbol toggle. The removed materializer therefore contributed
  unpreserved behavior. WU1 pins the actual named before/after census without assuming 44.
- **2026-08-22 — target contract.** → proposed ADR-0023. The candidate runner gate remains ignored
  RED; ordinary conformance remains on the current production lifecycle.
- **2026-08-23 — WU0 GO.** Real `tsc 6.0.3` matched all six new fixture projects. The candidate
  contract passed `3/3` with its intentional RED runner ignored, marker parsing passed, and the
  immutable ordinary receipt remained `580/696`. Independent adversarial review returned GO after
  the user-authorized second fix round.
- **2026-08-23 — WU1 GO.** Both ordinary `TSSymbolKeyword` lowerers now use the canonical
  project-local `well_known().symbol` identity. Real `tsc 6.0.3`, cold complete-source checks, and
  enabled B121 agree on `2322`/`2345`; packaged Reflect and Array SCC oracles are complete. The
  named library census removed only the obsolete Reflect incomplete: diagnostics stayed at 267,
  incompletes moved `292 -> 291`, and records moved `559 -> 558`. Candidate tests passed `3/3`
  with the intentional RED ignored, all-target check passed, and ordinary conformance passed
  `581/697/581/0` in 64.41 s. Independent review caught and then cleared stale canonical-evidence
  hashes; it found no sparse-only fix.
- **2026-08-23 — WU2 spec GO.** The full-profile oracle pins all 82 sources / 2,936,611 bytes,
  13,958 declaration sites, 2,099 current-binder groups, 2,400 canonical group fragments, exact
  type/member/form counts with explicit zeros, and canonical site/lexical/attachment digests.
  Current-binder endpoints are labeled allocation-local and forbidden from the product schema.
  The future std-only product crate has an intentional RED dependency/ownership/walker contract;
  full-profile one-pass, AST-drop, perturbation, edge/SCC, and determinism witnesses remain in the
  library-owned integration layer. Independent spec review returned GO after three fix rounds.
- **2026-08-24 — WU2 implementation GO.** The immutable declaration product now owns the complete
  AST-free full-profile declaration and lexical-scope recipes. The authenticated product digest is
  `a3e543680440ed88755d4d43f5a5a02c071052a061705e378d29d03a313d6c82`; construction covers all 82
  packaged files and remains free of binder and semantic identities. The product, one-root
  lifecycle, allocation, and fresh-build witnesses landed across `82410f3` through `4d781b5`, with
  independent reviews clearing the retained-syntax and ownership boundaries.
- **2026-08-24 — WU3 declaration universe GO.** The candidate now consumes the immutable product
  into one fresh project-local binder and binds the prepared Reflect witness once. All 14 normalized
  scope, group, namespace, augmentation, source-order, project-isolation, and no-semantic-work
  projections match the complete-source oracle. The exact normalized binder digest is
  `10cee1d655a8e48e07d76bd27c003a7f77b1da68418e907587cf3e2cec7b9554`. Spec and implementation
  landed as `7725f66` through `ae02145`.
- **2026-08-24 — WU3 measured and independently reviewed.** Commit `26955ba` measures only the
  retained product + prepared user AST -> fresh binder phase. The production-equivalent probe has
  no `test-utils` feature or heavy normalized test projection. Under
  `cpu-lease run -n 2 --no-smt --`, 10 warm-ups and 31 wall samples had a median of
  `297,850,091 ns`. One DHAT sample reported 63,499 allocation calls, 16,014,223 allocated bytes,
  4,159,067 live bytes at the snapshot, and 5,317,723 peak live bytes. Each sample bound 82 library
  units plus one user unit, allocated 59,537 binder layered rows, reused the exact product `Arc`,
  and performed zero semantic-base/compiler/event/query/relation/replay work. Release wall contract
  passed `5/5`, DHAT contract `3/3`, WU3 parity `14/14`, both clippy feature modes passed with
  `-D warnings`, and independent review returned GO after rejecting the first test-utils-contaminated
  probe. The wall result is one current 31-sample run, not a long-term trend.
- **2026-08-25 — WU4 oracle correction.** The immutable product intentionally does not encode the
  legacy replay owner's type/value space, replay root consumers, statement owners, event ordinals,
  or the raw library record ledger. WU4 therefore uses product groups, dependencies and publication
  SCCs as its structural authority. The complete-source lifecycle remains a temporary test-only
  black-box shadow for normalized observable outputs and is deleted in WU7; it cannot certify the
  candidate scheduler graph. In particular, the product SCC containing `ConcatArray` is a
  singleton; the historical `Array` / `ArrayConstructor` / `ConcatArray` cycle belonged to the
  legacy replay slice. Candidate atomic-cycle controls therefore use independently declared
  product-native cyclic evidence rather than preserving that replay-only partition.
- **2026-08-25 — WU4 passive acceptance correction.** Candidate traces are debug evidence, not
  authority for their own work. The revised RED contract wraps every candidate call with legacy-work
  and provider scopes, independently observes owner publication and session/cache lifecycle, freezes
  the exact eight-root unavailable rollback census, requires typed fault errors at pre-receipt seams,
  and proves unique A/B/A project and publisher sessions with zero retained cache after every call.
- **2026-08-25 — WU4 capability-boundary correction.** The candidate API moved from the library
  provider into a recursively audited checker-owned `project_local/` tree. The provider now only
  prepares the immutable product and live frontend callback inputs. The audit is append-only and
  read-only to tests, owner work remains raw with duplicate rejection, faults carry only their
  identity and activate at checker-owned canonical seams, and lifecycle proof is reserved for
  implementation-owned `Weak<SessionCore>` / `Weak<DurableCache>` hooks rather than mirror state.
- **2026-08-25 — WU4 AST-boundary correction.** The candidate tree now has an exact relative-path
  inventory and a positive Rust AST capability allowlist. Grouped or aliased imports, inline or
  undeclared modules, path attributes, opaque inclusion macros, unsafe/foreign code, and external
  checker paths outside `project_local_pass_port` fail the ordinary boundary test.
- **2026-08-25 — WU4 probe-projection correction.** Review confirmed that the RED validator's
  ordinary expected probe set filtered global types, global values, and module types through one
  shared name set. That could admit an undemanded external-module homonym. The corrected spec uses
  the materialized product key, its exact final binder-map type/value slot, and a distinct global or
  external-module `(source path, name)` coordinate. Synthetic controls cover global/module
  homonyms, a module dependency edge, and a user-global contributor that must not admit its module
  homonym. The packaged Array witness requires global `Iterator` and excludes the unreachable
  external-module `Iterator` without inspecting or predicting numeric identities. Probe projection
  runs after closure and cannot create work. The full `force_all` oracle JSON and hash are unchanged.
- **2026-08-26 — WU4 premature-SCC fault correction.** The earlier fault scenario replaced the
  canonical Array roots with the first nontrivial SCC member, so it could test a different workload
  and accept an unrelated premature commit. Product evidence shows that the unchanged Array closure
  already contains the unique compilation-global SCC named `Symbol` / `SymbolConstructor`: three
  product rows for Symbol type, SymbolConstructor type, and Symbol value. The corrected spec selects
  that SCC by names, product graph, final mapped slot and global visibility without numeric or
  iteration-order assumptions. The closure has exactly one later nontrivial SCC with the mapped
  global names `ReadonlyArray` / `FlatArray`, so the validator also proves that only the selected
  first SCC is premature. It requires one owner event for every selected member before activation
  and the first selected cache write afterward. It rejects replaced, missing or extra roots and
  authenticates the exact premature commit and passive activation interval. An identical-root
  no-fault sibling proves that the injected fault, not a changed demand set, causes the typed failure.
- **2026-08-25 — qualified-path provenance prerequisite.** WU4 review found that WU2 retained path
  names but discarded each segment's exact source site and collapsed every non-group binder outcome
  into a generic unresolved recipe. The revised scaffold adds empty canonical/validated tables and
  ignored RED product tests for all single and qualified Reference/Query paths; the authenticated
  packaged profile currently has zero import qualifiers. A test-only projection observer records
  binder outcomes before normalization. It freezes three real unresolved rows (`U`,
  `globalThis.IteratorObject`, and `IteratorObjectConstructor`) and synthetic typed controls cover
  the absent `NamespaceOnlyLeaf` and deferred import/enum outcomes. Production extraction still
  emits zero rows, so WU4 lowering must wait for its spec-first implementation. Name-length span
  inference, later binder-topology re-resolution, and candidate or legacy oracle copying remain
  forbidden.
- **2026-08-26 — recursive-alias design boundary.** Sound member-failure handling exposed the real
  packaged `IDBValidKey` array recursion behind `IDBCursor`. Whole-SCC unavailability violates the
  frozen observable contract, while mutable constructor shells violate stable `TypeId` meaning.
  The approved design uses immutable alias templates, lazy applications, checker-owned atomic body
  publication, and one-layer query projection. → [ADR-0024](../decisions/0024-represent-recursive-aliases-as-immutable-applications.md)
- **2026-08-26 — WU4 deferred-member boundary.** Atomic lowering exposed documented deferred
  interface members transitively reachable from `Array` and `IDBCursor`: type predicates (backlog
  `50`), optional methods (`49`), and polymorphic `this` annotations (`75`). WU4 does not implement
  those forms. It may omit one only when the complete-source route omits the same full member with
  the exact staged incomplete record; every retained interface signature and dependency must lower
  without recovery before atomic Ready. Class surfaces remain all-or-unavailable, omission creates
  no scheduling edge, and `IDBCursor.key` / `continue` retain recursive `IDBValidKey` semantics.
- **2026-08-26 — WU4 force-all audit boundary.** The first atomic deferred-member implementation
  proved that one `ProductLowerer { force_all }` cannot serve both sound publication and exact
  library-record inspection. Sound whole-member omission produced the correct ordinary `Array` and
  `IDBCursor` results but first produced only 411 candidate records (120 diagnostics / 291
  incompletes) against the unchanged 558-row authority (267 / 291). A later mixed-lifecycle patch
  regressed the candidate further to 288 (10 / 278); neither result is a new baseline. The exact
  diff also exposed missing product evidence: extraction discarded the `abstract` modifier on
  constructor type recipes, so the candidate could not authenticate the `ClassDecoratorContext`
  default incomplete. WU4 therefore separates publication lowering from a test-only scratch
  `ForceAllRecordAudit`, retains the constructor modifier in the immutable product, and keeps the
  existing 558-row ADR-0018 authority unchanged. Audit recovery is permitted only inside the
  branded scratch model and cannot affect publication, probes, scheduling, or caches.
- **2026-08-27 — immutable-alias census correction.** Named analysis proved that the 558-to-411
  live complete-source change was not dropped audit work: immutable guarded alias applications
  removed 97 `TK2430` and 51 `TK2344` false positives produced by the old recovery `error` frontier.
  TypeScript 6.0.3 checks the pinned profile clean. The remaining added `Awaited` `TK2456` is a
  conditional-recursion regression and must be fixed rather than pinned. The post-fix named
  authority is therefore 410 rows (119 diagnostics / 291 incompletes), with no incomplete change.
  WU4 does not emulate resolved model-gap debt. →
  [ADR-0025](../decisions/0025-rebaseline-library-records-after-immutable-aliases.md)
- **2026-08-27 — complete alias-evaluation correction.** M25 distribution, legal conditional
  recursion, root-lazy controls, and a second named census disproved the 410-row interpretation.
  No old diagnostic outcome disappeared: 19 `TK2344` rows changed only rendered detail, five
  `TK2430` rows were temporarily replaced by budget `TK2589`, and one ordinary union alias added a
  sixth budget row. The bounded planner fixes restore 558 outcomes with only the 19 same-code/site
  rendering pairs. →
  [ADR-0026](../decisions/0026-retain-library-outcomes-after-alias-evaluation.md)
- **2026-08-27 — alias-correct oracle rebaseline.** The 558-row library record authority retains
  every code and site; 19 `TK2344` details changed in named remove/add pairs. Complete-source
  semantic probe counts remain 2,982 per case, while their digests move because recursive aliases
  now retain and evaluate immutable applications instead of recovery types. No user result, event,
  source visit, contributor, product, or incomplete count changed.
- **2026-08-27 — production-owned census correction.** Read-only review proved that the scratch
  force-all audit had become a second 3,476-line semantic evaluator and could stay green while the
  production publisher emitted no records or published incorrect terminals. The 267 diagnostics
  are principally relation-model outcomes over supported declarations, not a reason for recovery
  semantics. WU4 deletes the audit, makes test-only `force_all` exercise the ordinary session and
  validation path, and reviews an attributed named diff from the 558-row starting census. →
  [ADR-0027](../decisions/0027-produce-library-census-from-the-project-publisher.md)
- **2026-08-28 — production census rebaseline.** The production publisher reports 709 named rows
  (`322` diagnostics, `387` incompletes), a reviewed `558 - 8 + 159` diff, and terminal receipt
  `3,406 / 3,377 / 29`; the 709-row multiset becomes the sole authority. →
  [ADR-0028](../decisions/0028-rebaseline-the-production-owned-library-census.md)
- **2026-08-28 — temporary census feature boundary.** Until WU5 makes the project-local session a
  production lifecycle, only opt-in verification builds expose `LibraryRecordCensus` and enable
  its test inspection stack. →
  [ADR-0029](../decisions/0029-gate-the-library-record-census-behind-an-opt-in-feature.md)
- **2026-08-28 — WU5 lifecycle inventory and compatibility boundary.** The current tree has three
  semantic routes: frozen shared, sparse/fallback, and complete-source standalone; only the
  project-local candidate already owns binder and meaning per project. WU5 introduces one
  high-level `CheckerSession` in `typokat-check` that consumes an `Arc<LibraryDeclarationProduct>`,
  owns product binding and one project-local semantic session, and returns real `CheckResult`
  values. The driver remains the non-published owner of parsing, workers, and report assembly.
  Frozen/sparse and complete-source implementations remain the production comparison controls;
  the project-local route stays test-only through WU6. WU7 performs the production dispatch switch
  and deletion. Public check signatures remain stable through that switch. GitHub code search found
  no indexed consumer of `LibraryBaseProvider`,
  `FrozenLibraryBase`, or `typokat_library` outside this repository, so the low-level public and
  documentation-hidden lifecycle API will be removed as an intentional pre-1.0 break in WU7, not
  wrapped permanently. Initialization precedence remains per public operation: repeated
  `check_source` / `check_files` / `check_project` acquire the immutable product before parsing,
  while standalone and Bundler checks reject user parse/inventory errors before product
  acquisition. Clean projects enter the same session in every mode; only its creation point differs.
- **2026-08-28 — WU5 session shadow complete.** `eb1c0f99` introduced the one-shot
  `CheckerSession`; the driver acceptance commits from `340e65cd` through `5bf87bf7`, implementation
  `f47e8976`, corrected product-structure contract `dab80506`, and operation-shape pair
  `0c87aaae` / `07ba6091` complete the test-only migration shadow. Source, independent files,
  repeated projects, standalone/CLI, Bundler, and official-batch-equivalent requests now exercise
  the same product binder and project-local publisher without switching a public route. Binder
  import/export diagnostics are reserved before binding, retained on the bound product, and
  consumed once by the ordinary user `Pass`; strict source-re-export admission remains a separate
  invariant. Concurrent sessions share the exact product `Arc`, have distinct project, publisher,
  and cache identities, and retain no mutable state after return. Verification passed the 10-test
  driver shadow block (`10/10`, `21.28 s`, one test thread, 1 GiB runtime limit), the complete
  product-project-structure block (`10/10`), default production clippy with warnings denied, test
  clippy with no new driver warning, formatting, and two independent final reviews. Public frozen,
  sparse/fallback, and complete-source dispatch remains the production control by the explicit
  WU5→WU6→WU7 sequencing; the candidate stays test-only until WU6 proves it and WU7 cuts over.
  Therefore backlog `116`, the low-level lifecycle deletion, and ADR-0029's temporary census
  feature boundary remain open for WU7 rather than being closed prematurely.
- **2026-08-28 — WU6 pre-timing boundary.** The existing authenticated harness already supplies
  the cold four-row panel, the exact 82-file TypeScript 6.0.3 profile, and the staged native
  TypeScript-Go 7.0.2 binary. The pinned upstream revision is locally reproducible at
  `2bd066d87f5bafd315be9f40889d0a60b9e58e0b`, but no in-process comparator runner existed. The
  session panel therefore freezes one fresh Program/checker and project-global merge per frame,
  bound immutable library-`SourceFile` sharing only on TypeScript-Go, immutable declaration-product
  sharing only on typokat, and a progressive `1 -> 32 -> 687` gate. The collector must stop on the
  first semantic, lifecycle,
  speed, or memory failure and enforces a 1 GiB child address-space limit. The authenticated old
  687-frame control is 12,946,437 us total route time and 144,109,568 bytes peak worker RSS. No WU6
  candidate timing has run yet. The old wall value is a sum of internal frame-route intervals;
  WU6 must compare it only to the candidate's same interval sum, while typokat versus TypeScript-Go
  uses external end-to-end batch wall samples. The four cold rows are the three-way semantic
  agreement set. The full official corpus uses the committed typokat scoreboard and pinned
  TypeScript baselines as separate ordered outcome authorities; it does not falsely require the
  research checker's known coverage gaps to equal TypeScript-Go.
- **2026-08-28 — WU6 measurable-runner correction.** Runner-seam review found three fields that
  could not be observed fairly under the first RED schema. Timed receipts now include a distinct
  parse/prepare phase. Exact allocation calls move to a separate non-timing typokat counting-
  allocator receipt; RSS remains the cross-tool memory gate. Teardown proof is tool-specific:
  typokat requires dead session/cache weak handles, while TypeScript-Go requires zero runner-owned
  frame handles and may not force GC or finalizers. TypeScript-Go is single-threaded, and its
  semantic oracle is read from Git blobs at the pinned TypeScript revision rather than trusted from
  the staged corpus copy. No candidate timing has run.
- **2026-08-28 — WU6 live-authority boundary.** The pure panel verifier cannot report `GO` from a
  caller-supplied evidence callback. It reports only `PANEL-PASS` / `PANEL-FAIL`. WU6 `GO` is
  reserved for the live orchestrator that starts both source- and binary-authenticated runners
  itself under the recorded CPU lease and links every warmup, sample, control, concurrency, and
  allocation receipt to those identities. Each JSONL request carries the exact authenticated
  official-suite source bytes; a case id and fabricated semantic response cannot satisfy parity.
- **2026-08-28 — WU6 preflight evidence correction.** The bounded one-frame live preflight does not
  claim cold/batch linkage. The authenticated cold oracle contains no candidate commit or live
  runner identity, so copying the current commit and oracle hash into a linkage receipt would be
  fabricated evidence. The preflight instead re-authenticates the clean candidate, both runner
  sources, and both runner binaries after all semantic and fault-control processes. The full live
  orchestrator remains responsible for collecting current-candidate cold evidence and proving the
  contract's cold/single/batch linkage before it can report `GO`.
- **2026-08-28 — WU6 class-only oracle normalization.** Every frame result has explicit
  `parse_errors`, `diagnostics`, and `incompletes` lists. The committed scoreboard contains only a
  class/status witness for 42 parse-error and four unresolved eligible frames, so those normalize
  to the stable tokens `parse-error` and `unresolved`. Runner-specific message text is not treated
  as authenticated evidence.
- **2026-08-28 — WU6 exact-flags oracle correction.** The first authenticated official frame
  exposed that the TypeScript `.errors.txt` baseline was produced with the test's own options,
  while WU6 strips directives and applies canonical `strict` / ES2025 / Preserve flags. The
  baseline expected eight diagnostics; the batch TypeScript-Go runner, the authenticated cold
  TypeScript-Go CLI, and authenticated `tsc` 6.0.3 all produced the same twelve diagnostics under
  the WU6 flags. A full cold comparison then found the first legitimate 6.0.3-versus-7.0.2
  difference after 76 matching frames, at `privateNameDuplicateField.ts`; requiring one 687-row
  cross-version oracle is therefore invalid. Official baselines remain source/version evidence.
  The four cold workloads retain three-way `tsc` / TypeScript-Go / typokat truth. The 687-frame
  panels use separate ordered authorities: typokat's committed scoreboard and a committed 1.17 MiB
  exact-flags oracle produced by 687 fresh processes of the authenticated cold TypeScript-Go
  binary. That artifact pins every source, argv, stdout, stderr, return code, binary/profile
  identity, and normalized outcome. The batch runner must reproduce it and may not generate or
  bless it. No candidate timing has run.
- **2026-08-28 — WU6 live preflight passed.** The bounded collector ran under a two-vCPU no-SMT
  lease at clean candidate `2306b8ab84594944ff11b3b1c229c9e05a64758a` and returned only
  `PREFLIGHT-PASS`. The retained local artifact is
  `target/wu6/session-preflight-2306b8ab.json` (file SHA-256 `ddfcd768ed620606ebe70393e8b39b5a70813d116321d982a98c67133b7f02d2`,
  canonical evidence SHA-256 `bd9c41fccdcb42d2a7f0e8e569be0c9dc9a91a12f356f3aa4941999b9c3b98d2`).
  It authenticates the typokat runner (`95cbb210da3d862735bb0bd5bae9399da117e85265996f97f55df3239e00c091`),
  the TypeScript-Go runner (`eab171db92b071545523a5af3b2086b5ad4915ebb72ee6a5f934277dca40b20c`),
  both exact source hashes, the 82-file profile, the first separate semantic oracle for each tool,
  dead typokat session/cache weak handles, zero TypeScript-Go retained frame handles, all four
  fault controls, 1 GiB child limits, and final candidate/source/binary re-attestation. An earlier
  diagnostic pass exposed machine-global `sccache`; the accepted run instead used direct pinned
  Rust 1.95.0 compilation from a private non-home cwd and rejected every Cargo configuration
  discovery path before and after build. Independent review recomputed the evidence and passed.
  This is not WU6 `GO`: cold linkage, progressive panels, concurrency, allocation evidence, and
  performance gates remain uncollected. No candidate timing has run.
- **2026-08-29 — WU6 live cold/session linkage passed.** The non-injectable collector ran under a
  two-vCPU no-SMT lease at clean candidate `6a6254bec3c5fb025ffa6a16e299baed9e59d77b`
  (tree `f790d2ced7915211ac8ff22fb3934a9cf1495fdd`) and returned `LINKAGE-PASS`. The
  retained local artifact is `target/wu6/session-linkage-6a6254be.json` (file SHA-256
  `f8006b525738b182688f5a555dc1896e466b49befca98b8d2d8c1fe0e2ee6329`, canonical evidence
  SHA-256 `648e01403e385bcf5225860ce9a276013a1d83b79271992d8e39ef738f4dcb5d`). It links
  the ordinary cold CLIs and the same-binary single/batch runners across all four frozen rows
  (`1 / 1 / 2 / 32` files), authenticates the exact 82-file profile, source and binary identities,
  and retains dead-session/cache lifecycle receipts. Independent raw recomputation returned
  `LINKAGE-RECOMPUTED` with oracle SHA-256
  `2df1b378c1d31367db41b538700c38e6f068a3a0bec9264f60bbf34c1b26a785`. This is still not
  WU6 `GO`: the progressive 1/32/687 timing and memory panels, concurrency evidence, allocation
  receipt, and current-provider 687-frame comparison remain uncollected.
