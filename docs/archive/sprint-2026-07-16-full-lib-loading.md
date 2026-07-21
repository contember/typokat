> **OUTCOME — archived partial / WU0 NO-GO, 2026-07-21.** WU0 committed the exact pinned
> TypeScript profile and feasibility harness, then localized and removed the first substitution
> barrier and bounded diagnostic type rendering. Standalone `lib.dom.d.ts` now completes in 2.03 s
> at 42,800 KiB maximum RSS, but the authoritative
> `perl tooling/wu0d-release/run.pl --single primary off` gate still terminated the process with
> exit 143 after 5,268,202 us (`target/wu0d-release/runs/20260721T114215Z-284894`), so its
> 5.00 s / 512 MiB acceptance contract remains unmet. WU1–WU8 were neither authorized nor run;
> `src/prelude.ts` remains production, readiness manifests remain PENDING, completion manifest
> `D-libdts` remains incomplete, and backlog 14 stays open for a newly planned sprint.
>
> **Commit map.** Plan/profile/gate: `4588932`, `d91223c`, `070f622`. Exploratory WU0
> feasibility/attribution sequence (temporary parts later removed): `a55f522`, `3b9117a`,
> `d6d159b`, `885e5c2`, `e233b21`, `081ec72`, `3afff82`, `661df38`, `da0b420`, `52bb552`,
> `df639c2`, `3c90bda`, `7629167`. Substitution diagnosis/spec/implementation/record: `35f0759`,
> `5916b5f`, `fbde2aa`, `8f89d53`, `5f6408e`, `6d53746`. Bounded-rendering
> spec/implementation: `2479357`, `eff7a8f`. Temporary-attribution cleanup: `ca1a68e`, `2153c83`.
> The cleanup removed temporary WU0C–WU0G attribution/debug tooling while retaining the WU0B
> measurement prototype and the fail-closed WU0D release gate.

# Sprint — pinned full default-library loading (2026-07-16)

**Goal.** Replace the curated minimal prelude with the exact TypeScript 6.0.3 ES2025 full-host
library profile, compiled once into an AST-free shared semantic base with identity-preserving
private deltas and a correctness-first private rebuild for global collisions.

**Theme.** Backlog [`14`](../backlog/14-libdts-loading.md) and parallelism Stage 1 are one
capability: real standard-library declarations must be ordinary checker input, but their immutable
semantic result must not be parsed, bound, checked, or cloned once per worker. The binding decision
is [ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md). This sprint remains
spec-first and cutover-last: the current `src/prelude.ts` production path stays active through WU4.
WU0A lands the disabled RED corpus and exact profile before WU0B may add a measurement-only injected
prototype; every hard audit/performance/routing gate must then pass before production WU1 or driver
cutover may start.

## Refs re-verified at HEAD (2026-07-16)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The namespace/declaration-space prerequisite is shipped and the pinned ES5 model gate is GO:
  four `TK2430` diagnostics and 187 explicit incompletes, with no namespace-owned residual —
  `tests/fixtures/lib-es5-6.0.3/readiness.toml:34`, `tests/lib_es5_readiness.rs:91`.
- ✔ Production still parses and checks `src/prelude.ts` for each checker invocation; its handoff is
  owned semantic state, but `Store`, `Interner`, `Binder`, `ScopeGraph`, symbols, declarations, and
  namespace tables are monolithic mutable arenas — `src/check/checker/mod.rs:90`,
  `src/check/checker/mod.rs:169`, `src/types/store.rs:34`, `src/types/intern/mod.rs:123`,
  `src/binder/bind.rs:27`.
- ✔ `check_source` returns `CheckOutput`, while `check_files` and `check_project` return report
  vectors directly. The public API has no library-initialization error channel and worker spawn/join
  still use `expect` — `src/driver.rs:72`, `src/driver.rs:81`, `src/driver.rs:141`,
  `src/driver.rs:160`, `src/main.rs:148`.
- ✔ `ProjectBinderBuilder::add_module` creates isolated module scopes. It cannot bind the 82
  library units as one shared script-global declaration domain — `src/binder/bind.rs:503`.
- ✔ Architecture §8.2 requires a frozen shared prelude plus private deltas for Stage 1. ADR-0011
  narrowly revises §8.3 only for the immutable library binder prefix; user ASTs and mutable user
  binding remain owned and unshared — `docs/reference/architecture.md`.
- ✔ The sole 1.0 library profile is the recursive `/// <reference lib>` closure rooted at
  TypeScript 6.0.3 `lib.es2025.full.d.ts`, ordered by tsc `libEntries` priority: **82 files,
  58,349 LF line terminators, 2,936,611 bytes**. It is the fixed oracle profile for
  `tsc 6.0.3 --strict --target es2025`, not every selectable TypeScript lib.
- ✔ Profile provenance is TypeScript commit
  `050880ce59e30b356b686bd3144efe24f875ebc8`; root SHA-256 is
  `e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab`; ordered raw-body SHA-256
  is `0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267`; length-framed registry
  SHA-256 is `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`.
- ⚠ Release feasibility is not established. `lib.es5.d.ts` finishes in 0.01 s / 9,092 KiB RSS,
  but `lib.dom.d.ts` timed out after 30.03 s, a stripped-reference same-universe 82-file concat
  timed out after 120.10 s, and 82-path project mode timed out after 60.05 s. These are profiling
  evidence, not semantic oracles. On the same host, tsc checks the profile plus one clean source in
  1.19 s / 313,680 KiB RSS.
- ⚠ A conservative private-rebuild classifier may be common, not rare: a simple source grep found
  425 committed `.ts` cases, 130 with top-level function/`var` forms and about 99 without an obvious
  module marker. WU0 must replace this lower-bound regex with the real binder classifier and prove a
  practical route incidence before the architecture earns implementation.
- ✔ The official-suite harness starts one fresh process per 874 case. A cold library build per case
  would make the ratchet unusable, so cutover requires an isolated single-process case protocol —
  `tooling/official-suite/tsofficial.py`.

## Binding constraints

- Follow the mandatory corpus → implementation → independent adversarial review loop in
  [`dev-method.md`](../reference/dev-method.md). WU0 acceptance files land in their own commit before
  any measurement prototype or production implementation. For WU1–WU5, every acceptance,
  invariant, API, and protocol expectation not already committed in WU0 lands in a separate RED/spec
  commit before its implementation; no implementation commit introduces its own acceptance
  expectations.
- Preserve the semantic-query, immutable-publication, typed-exhaustion, and lexical-event
  invariants in [`invariants.md`](../reference/invariants.md). No base row references a delta; no
  project mutates shared state; no unavailable surface degrades to error/`any`/an empty success.
- Use one canonical source-backed library compiler in both modes. The collision path recompiles the
  embedded library and project together; it is not a second loader or a query overlay.
- Library events have a separate audited ledger and never consume user `ModuleOrdinal`, `UnitSlot`,
  or EventStore tickets. User records keep the current four-key order.
- All bridge and intrinsic behavior is selected by universe-local `LibrarySemanticIdentities`, not
  by arbitrary name lookup. Shared and private universes never exchange IDs.
- Stop, document, and request direction when a hard gate or falsifier trips. Do not invent a shim,
  partial base, dual loader, or silent threshold relaxation.

## Work units

### WU0 — authoritative audit, disabled acceptance spec, and feasibility gate (effort XL)

- **Problem.** The ES5 readiness proof cannot be extrapolated to the 82-file closure, and current
  release probes do not finish. The exact semantic ledger, bridge need, global-collision incidence,
  AST-free product boundary, and finite performance envelope are unknown.
- **Verify first.** Reconstruct the closure from the pinned TypeScript tree; compare every path,
  reference edge, priority, byte count, LF count, per-file digest, and aggregate digest with the
  ADR. Run the simplest correct release library-global same-universe pipeline with phase timers for
  registry validation, parse, bind, reserve/fill, publication/validation, and statement checking.
  Cross-check user behavior with `tsc 6.0.3 --strict --target es2025 --noEmit`; cross-check the
  library ledger with all 82 explicit paths plus `--noLib`.
- **Scope.** Commit first, without enabling production behavior:
  - the exact 82 declarations and upstream notices at their final single source-of-truth path,
    `src/library/typescript-6.0.3/`, plus its profile/provenance manifest; nothing reads this path in
    production during WU0;
  - `tests/fixtures/lib-es2025-full-6.0.3/` readiness, ledger, bridge, and routing manifests that
    reference the canonical production asset path instead of copying its declarations;
  - disabled `tests/cases/b14_full_lib_loading/` and project fixtures covering global values,
    arrays/tuples, Promise/iterators, primitive/object/function members, RegExp, `globalThis`, DOM,
    `Intl`, intrinsic aliases, diagnostics, and user global augmentations;
  - a parser/declaration/outcome census by file, source span, stable owner, and availability state;
  - a bridge evidence table: syntax-native representation → universe-local library identity → all
    current consumers → required / not required / explicitly deferred;
  - an exhaustive preflight-name matrix using the same script/external-module classifier and
    declaration/binding-pattern walkers as the binder, including synthetic `globalThis` and
    `undefined` roots;
  - route-incidence results for every committed conformance source and all official-suite inputs,
    grouped by collision, unique global-object contribution, `declare global`, namespace/UMD, and
    classifier uncertainty;
  - release phase profiles, cold/warm/RSS samples, and a Send+Sync/AST-drop feasibility proof for
    every proposed frozen product.
- **WU0 sequencing authorization.** WU0A commits the profile/provenance assets and disabled RED
  acceptance corpus before any spike or production implementation. WU0B may then add one
  measurement-only injected prototype, unreachable from driver defaults and public APIs, that
  implements only the library-global bind/compile/freeze seam and shared candidate collector needed
  for ledger, routing, phase, and `Send + Sync` evidence. The spike uses the real source
  classification/reserve/fill/publish/check path and is designed for direct reuse, but does not
  authorize a production shortcut: WU1 productionizes the canonical compiler, WU2 the segmented
  prefixes, and WU3 routing, each under separate RED specs and revalidation. `src/prelude.ts`
  remains the production path throughout WU0.
- **Acceptance / witness.** WU0 is GO only when all of the following are true:
  1. profile is exactly 82 / 58,349 / 2,936,611 and all fingerprints/license inputs match;
  2. every diagnostic/incomplete/unavailable node has a concrete live owner and no permissive
     publication exists;
  3. one tiny non-colliding source plus cold initialization completes in each of five release
     processes in **≤5.00 s** and **≤512 MiB max RSS** on the recorded host;
  4. the real route-incidence fraction and projected/observed batched suite time have a finite,
     practical threshold explicitly approved by the leader; the readiness manifest records the
     production-invocation denominators, category counts, expected rebuild count, measured and
     projected suite time, approved maximum fraction/time, approver, date, and GO/NO-GO; without
     that durable approval WU1 may not start;
  5. same-process warm p50/p95 and all-colliding fanout baselines are measurable and bounded;
  6. required frozen products are demonstrably AST-free and can become `Send + Sync + 'static`;
  7. the bridge matrix has no unexplained consumer or scattered string-name requirement.
  Failure of any item records NO-GO in the sprint. High fallback incidence requires a new decision
  for a narrow project-local `globalThis` projection or explicit semantics deferral; it does not
  authorize silently weakening the classifier.
- **Touch points.** `src/library/typescript-6.0.3/`, `tooling/library-profile/`,
  `tests/fixtures/lib-es2025-full-6.0.3/`,
  `tests/lib_es2025_full_readiness.rs`, `tests/cases/b14_*`, `tests/conformance.rs`,
  `tests/cases/README.md`, `docs/reference/divergences.md`, measurement-only library hooks.
- **Commit boundary.** Disabled acceptance corpus, profile/provenance assets, offline verification,
  and the measurement-only prototype/evidence are separate commits in that order. No production
  default or public API changes in WU0.

### WU1 — embedded registry and one library-global compiler (effort XL)

- **Problem.** There is no offline profile registry, multi-source shared-global binder, separate
  library event domain, or typed terminal freeze contract.
- **Verify first.** Reconfirm WU0 GO and exact source/license package inventory. Characterize every
  top-level library unit as script or external module using the production classifier.
- **Scope.** Consume and revalidate the exact unmodified sources, TypeScript `LICENSE.txt`,
  `ThirdPartyNoticeText.txt`, `-text` asset rules, and per-file registry metadata committed in WU0.
  Productionize one injected `LibraryCompiler` used by shared and private modes. It binds script units
  into one library global, preserves external-module scopes and `declare global` routing, reserves
  synthetic `globalThis`/`undefined`, records a separate `LibraryEventLedger`, validates the WU0
  ledger, and returns typed terminal owned products. Keep `src/prelude.ts` as production default.
- **Acceptance / witness.** Tampered/missing/extra/reordered/unknown-reference/cyclic profile tests
  fail closed; byte comparison and all fingerprints pass; `cargo package --list` plus package
  extraction includes all assets/notices byte-for-byte. Reverse/reordered inputs do not alter the
  mandated registry order. Library records never enter a user event domain. Unexpected output is a
  typed error, not a panic or suppression.
- **Touch points.** `src/library/{mod,profile,registry,compiler,ledger}.rs`,
  `src/library/typescript-6.0.3/`, generated registry, `.gitattributes`, `Cargo.toml`,
  `tooling/library-profile/`, binder library-global entry points.
- **Commit boundary.** Registry integration and compiler/ledger each receive a separate RED/spec
  commit before their focused implementation commit.

### WU2 — frozen type/binder prefixes and private deltas (effort XL)

- **Problem.** Store and binder tables are single mutable vectors; direct `&Store` consumers and
  one dedup map cannot read an immutable prefix plus a writable delta.
- **Verify first.** Inventory every Store/Interner side column, raw `TypeId` comparison, binder
  table read/write, ID counter, and public method signature. Prove a base-first structural probe can
  preserve `TypeId == structural equality` without remapping.
- **Scope.** Introduce segmented Store/Interner facades with offset-aware delta allocation,
  base-first dedup/equality, immutable prefix reads, delta-only writes, and terminal reservation
  checks. Apply the same prefix/delta contract to library binder scope/symbol/declaration/group/
  namespace/value tables and identity counters. Freeze `LibrarySemanticIdentities` with the base;
  private library compilation produces an unrelated private table. User parsing/binding stays
  owned and interner-free; only immutable library binder rows are shared.
- **Acceptance / witness.** Empty deltas duplicate no base type/table; a delta shape equal to a base
  shape reuses its base ID; delta rows may reference both tiers but the base never references a
  delta; two projects cannot see each other's rows; recursive reserved batches terminate or expose
  typed failure; all direct consumers work through the segmented facade. Base products pass
  compile-time/direct `Send + Sync + 'static` assertions and retain no AST or pass-local cache.
- **Touch points.** `src/types/{store,intern,substitute}/`, `src/class_semantics.rs`,
  `src/binder/{bind,scope,symbol,declaration,namespace}.rs`, checker publication environments and
  library freeze types.
- **Commit boundary.** Type-universe segmentation and binder-prefix segmentation each receive a
  separate RED invariant/spec commit before their independently reviewable implementation commit;
  production still uses the minimal prelude.

### WU3 — collision preflight and private same-universe rebuild (effort XL)

- **Problem.** The shared base cannot be mutated or shadowed when a user declaration changes a
  library/global-object surface; false-negative preflight is a soundness failure.
- **Verify first.** Re-run the WU0 real-classifier incidence and every OXC declaration/name case.
  Differentially compare forced-private and candidate-fast routing on all supposedly safe inputs.
- **Scope.** Before any delta/event/cache mutation, run the shared exhaustive binder name/candidate
  collector. Route every frozen-root collision, every `declare global`/root namespace or UMD form,
  and every script value that may contribute to the effective global object/`globalThis` into the
  same compiler pipeline with embedded library + complete user project in one private universe.
  Include unavailable/incompatible roots and all destructuring leaves. Bound concurrent private
  rebuilds with one process-wide permit that carries no semantic state. Fast and private modes each
  use their own `LibrarySemanticIdentities`.
- **Acceptance / witness.** False positives may rebuild; exhaustive mutation tests permit no false
  negatives. Fast-vs-forced-private outputs are identical for every approved fast case. Supported
  colliding interface/class/function/var/namespace forms, augmentations, opposite input orders, and
  synthetic-root redeclarations match tsc by normalized source identity. Deferred forms match
  their pinned non-permissive diagnostic, incomplete, or unavailable outcome and named divergence
  owner; none may become false-clean. User ordinals and EventStore ordering stay input-local; no
  library ticket leaks. No result publishes until the private run succeeds, and unavailable merges
  cannot fall back to the frozen prefix. The one process-wide private-rebuild permit remains held
  through extraction of owned reports and destruction of the complete private library/project
  universe, then releases. Retained-base plus private-compiler RSS and all-colliding serialized
  wall/RSS gates from ADR-0011 pass.
- **Touch points.** binder shared declaration/name walker, `src/library/compiler.rs`, driver route
  selection, private universe construction, concurrency permit, project/event tests.
- **Commit boundary.** Preflight and private compilation land after separate RED corpora and before
  any production cutover.

### WU4 — evidence-selected semantic bridges and intrinsic roles (effort L/XL)

- **Problem.** Syntax-native types do not automatically see authoritative library declarations,
  while the curated prelude currently recognizes intrinsic/utility roles by a temporary handoff.
- **Verify first.** Freeze the WU0 matrix. No candidate without both good/bad tsc witnesses and a
  complete current-consumer inventory enters scope.
- **Scope.** Add one checker-owned bridge service keyed exclusively by universe-local
  `LibrarySemanticIdentities`. Implement only proven rows: at minimum the WU0-confirmed array/tuple
  heritage/member and RegExp-literal requirements; add readonly arrays, primitive wrappers,
  Object/function apparent members, or `globalThis` projection only if the matrix proves necessity
  and the selected routing architecture supports them. Bind the four string intrinsics,
  `ThisType`, and any retained `OmitThisParameter` behavior by validated declaration role. Re-audit
  the authoritative `ReturnType`; never rewrite vendor sources.
- **Acceptance / witness.** Every selected row has clean and rejecting tsc controls, shadowing and
  shared/private-universe tests, and missing-member failure controls. BigInt/Symbol or other
  unavailable syntax stays explicitly owned rather than gaining a speculative bridge. No arbitrary
  user-visible name hook or unrelated relation change exists.
- **Touch points.** `src/check/checker/library.rs` plus only WU0-proven consumers in expression,
  call, member, declaration, relation/evaluator paths; `src/library/identities.rs`.
- **Commit boundary.** Each bridge family's WU0/RED spec is committed first; its implementation
  lands in a separate focused commit. The current prelude remains production through the end of
  WU4.

### WU5 — singleton lifecycle, API migration, batch harness, and atomic cutover (effort XL)

- **Problem.** Production cannot share the base, report deterministic initialization failure, or
  keep the official ratchet practical with current APIs/process-per-case harness.
- **Verify first.** Re-run WU0–WU4, all hard performance/routing gates, and a dry-run API migration.
  Prove single-file, parallel-file, and project modes obtain the same provider semantics.
- **Scope.** Add exactly
  `OnceLock<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>>`, initialized before rayon and
  never reset/retried. Migrate `check_source`, `check_files`, and `check_project` to Result-bearing
  APIs; map library failure to stable CLI exit 2 without partial user output. Tests inject local
  providers/cells. Map library/check worker spawn and join failures into the typed Result channel so
  the CLI emits stable failure output with no partial reports. Add an official-suite single-process
  isolated-case protocol: one initialized base, fresh allocator/binder delta/type delta/event
  domain per case, no declaration sharing, and preserved per-case
  diagnostics/incomplete/timeout/infrastructure accounting. The versioned non-interactive protocol
  uses one UTF-8 JSON request or response object per line, carries a `case_id` in every frame,
  reserves stdout for protocol frames, and sends process diagnostics to stderr. Requests run
  sequentially under the existing per-case
  deadline. A timeout, crash, malformed frame, duplicate/missing/mismatched ID, or unexpected exit
  kills the worker and attributes timeout/infrastructure failure to the current case; the supervisor
  starts a fresh worker and resumes with the next case. Failure to initialize that replacement is
  run-level infrastructure failure and aborts without fabricating results for remaining cases.
  Atomically switch
  every driver mode from `bootstrap_trusted_prelude` to the new provider, then remove
  `src/prelude.ts`; no dual production path remains.
- **Acceptance / witness.** 1/2/32 callers observe one pointer-identical base and exactly one build;
  deterministic injected failure is cached once and returned by every public API; CLI output/exit
  is exact. Warm structural counters show zero library parse/bind/check and no base-sized clones.
  RED protocol tests force initial initialization failure, middle-case timeout, crash, malformed or
  mismatched output, replacement initialization failure, and following-case isolation. Old and new
  official protocols produce the same scoreboard before cutover; after cutover the isolated
  protocol completes all 874 cases without cross-case leakage. Spawn/join failures are typed and
  render no partial CLI output. All three driver modes use only the new provider, and prelude-source
  symbols/files are absent.
- **Touch points.** `src/library/state.rs`, `src/driver.rs`, `src/check/checker/mod.rs`, `src/lib.rs`,
  `src/main.rs`, API call sites/tests, `tooling/official-suite/{tsofficial.py,test_tsofficial.py}`,
  deletion of `src/prelude.ts`.
- **Commit boundary.** Harness protocol and internal provider/failure plumbing each receive a RED
  spec commit before implementation. The public `check_source`/`check_files`/`check_project`
  signature migration, CLI failure behavior, provider cutover, and `src/prelude.ts` removal land
  together in one atomic cutover commit. No cutover occurs if any hard gate is unapproved or
  failing.

### WU6 — scale, performance, and official/full-stack ratchets (effort L)

- **Problem.** Passing focused library fixtures is not proof of isolation, scale, resolver-facing
  behavior, or deployable package contents.
- **Verify first.** Repeat benchmark methodology on the recorded host; validate package bytes; build
  a release binary immediately before official measurement.
- **Scope.** Run cold/warm/1-2-32-worker/all-colliding profiles, deterministic repeated runs, the
  complete Rust/conformance suite, official-suite identity ratchet, and the exact missing-library
  witnesses in backlog 14 (`Error`, `Promise`, generators, DOM values, Number/String/Object/Date,
  Iterable and implicit Array heritage). Exercise the Bundler-compatible full-stack ambient witness
  jointly owned with backlog 15 without claiming module-resolution completion. Update architecture
  §4/§8.2/§8.3/§12, public limitations, divergence ledger, package/provenance docs, backlog 14/16,
  completion manifest, indexes, and candidate closure documentation only after results are
  adjudicated; do not delete backlog 14, stamp/archive the sprint, or publish closure indexes here.
- **Acceptance / witness.** Full `cargo test`, conformance, fmt, clippy, release build, package
  extraction, deterministic worker-count/order tests, all ADR performance gates, and official
  `run --check` pass. Every official movement is independently classified before `--save`; no
  missing global is replaced by error-type success. Backlog 14 closure remains prohibited until
  WU8, even when the profile is the sole production path and every acceptance row is terminal.
- **Touch points.** benchmark tooling/results, official scoreboard/harness, README/docs/reference,
  `docs/backlog/{14,16,completion-1.0.toml}`, sprint/archive indexes.
- **Commit boundary.** Performance evidence and reviewed scoreboard remain separate atomic commits.
  Candidate documentation is held until WU7 PASS and is not a lifecycle closure.

### WU7 — independent adversarial review and remediation (effort L)

- **Problem.** Store/binder identity, publication, collision routing, singleton failure, and
  cache/event order can silently drop errors even when the happy-path corpus passes.
- **Verify first.** Give a fresh reviewer the WU0 oracle, all diffs, exact tsc version/profile,
  route logs, benchmarks, and scoreboard without implementation guidance that would anchor probes.
- **Scope.** A read-only agent hunts false negatives, partial base exposure, base/delta equality
  errors, collision classifier misses, globalThis drift, shared/private identity leakage,
  initialization races, failure retry, library/user event mixing, official batch case leakage,
  and cache-order changes. Cross-check good/bad controls against tsc 6.0.3. Implementation fixes are
  delegated back to a separate worker; the reviewer repeats the entire failing matrix.
- **Acceptance / witness.** Explicit high-confidence PASS, zero unresolved HIGH findings, full gates
  repeated after the final remediation, and no unadjudicated scoreboard movement. A FAIL keeps the
  sprint active and backlog 14 open.
- **Touch points.** Read-only whole diff/test/benchmark review; remediation files depend on findings.
- **Commit boundary.** Review fixes are atomic and never folded invisibly into the cutover commit.

### WU8 — closure-only lifecycle commit (effort S)

- **Problem.** Backlog and sprint paths are status; closing them before independent PASS would
  publish a false completion state.
- **Verify first.** Require WU7 high-confidence PASS, zero unresolved HIGH findings, the full
  post-remediation gate rerun, package byte verification, and a clean adjudicated official ratchet.
- **Scope.** Apply the already reviewed candidate documentation, delete backlog 14, rescope backlog
  16 if its dependency changed, stamp the sprint OUTCOME with the exact commit map and verification
  measurements, archive this sprint, and update sprint/backlog/top-level indexes.
- **Acceptance / witness.** No active document claims unfinished work, every moved/deleted path has
  no stale reference, the archive outcome matches committed evidence, and the production path is
  the sole complete pinned profile.
- **Commit boundary.** One closure-only atomic commit after WU7; no implementation or scoreboard
  movement is folded into it.

## Hard stop conditions

Stop before WU1 or cutover, preserve the current prelude, and request a new decision when:

1. the full registry/profile or outcome ledger is incomplete, drifted, or ownerless;
2. the simplest library-global release pipeline cannot meet 5.00 s / 512 MiB on the named host;
3. fallback incidence or projected/observed suite time has no explicitly approved practical bound;
4. a frozen product retains an AST/allocator/pass-local state or cannot be Send+Sync;
5. segmented identity cannot preserve structural `TypeId` equality without base mutation/cloning;
6. preflight has a false-negative declaration/name form or cannot run before all state mutation;
7. shared/private library compilers diverge, or a private run leaks/suppresses library/user events;
8. a required bridge needs vendor rewriting, arbitrary names, or unproven semantic breadth;
9. cold/warm/private/fanout/package/official gates fail or require a silent threshold relaxation.

If only cold initialization fails after the in-memory model proves correct, a build-time serialized
snapshot becomes eligible for a separately approved ADR. It does not enter this sprint implicitly.

## Out of scope (explicit)

- User-selectable `--lib`, `noLib`, target-derived libs, ESNext, DOM-free/worker/Node/Bun profiles,
  host filesystem/npm/network discovery, `@types`, or custom lib replacement.
- Backlog [`15`](../backlog/15-modules-imports.md) resolver breadth, package enumeration, or module
  semantics beyond the shared full-stack witness; backlog
  [`18`](../backlog/18-duplicate-identifier-detection.md) duplicate-declaration breadth and backlog
  [`82`](../backlog/82-declare-global-value-space.md) value publication semantics beyond
  correctness-first private compilation.
- Parallelism Stage 2 stable export hashes/shared mutable exports, incrementality, disk semantic
  snapshots, or portable serialization.
- Completing owners 50/75 merely to make the library clean; their exact unavailable ledger remains
  explicit. Likewise no opportunistic enum, `satisfies`/`as const`, flow, operator, or emit/runtime
  work.
- An augmentation recipe IR, reverse-dependency rebuild graph, `ProjectPublicationOverlay`, second
  Store/query authority, manual global declarations, or dual production loader.
- The planned namespace binder refactor. Perform it only if WU0 profiling proves it is necessary;
  do not mix a behavior-preserving cleanup into this capability sprint.

## Decisions

- [ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md) selects the fixed ES2025
  full-host profile, embedded raw assets, one library-global compiler, shared frozen base/private
  deltas, conservative preflight, and a private same-universe collision rebuild.
- Alternative A (compile every run) is WU0's correctness baseline and the bounded collision path,
  not the common production path. Alternative C (serialized snapshot) is profiling-gated follow-up.
- Soundness dominates route speed: false-positive fallback is acceptable; false-negative routing is
  not. High incidence stops for a new decision rather than silently relaxing globalThis semantics.
- The public Result API change and fixed ES2025 oracle are acceptable before 1.0 and land only at
  atomic cutover.

## Sequencing and commits

1. WU0A disabled RED corpus + profile/provenance commits; WU0B measurement-only prototype/evidence;
   review the GO/NO-GO and durably approve the cold and routing thresholds.
2. WU1 compiler/ledger → WU2 type prefix → WU2 binder prefix. These remain behavior-neutral while
   the minimal prelude is production. Every new acceptance/invariant spec lands RED first.
3. WU3 preflight/private fallback. Re-run routing incidence; stop if the approved bound drifts.
4. WU4 evidence-selected bridges/intrinsics, one family per separate RED and implementation commit.
5. WU5 harness/API migration, then one atomic provider cutover and prelude deletion.
6. WU6 measurement/adjudication and candidate docs; WU7 adversarial review repeats after every
   remediation; WU8 performs the only lifecycle closure after PASS and a full gate rerun.

At most one worker owns each source/module family. The leader commits only explicit file lists and
keeps specs, implementation, scoreboard, and docs lifecycle changes separate.

## Run log

<!-- Append discoveries/deviations/blockers. Graduate changed rationale to an ADR and future work
     to backlog. Do not weaken a hard gate in this log. -->

### 2026-07-18 — WU0 hard NO-GO: primary/off cold deadline

- **Frozen inputs.** Commit `da0b42025ddfa4a606390bbd665b5547b1cdfdb7`; release libtest
  `d6179df550804179b0f2e29359deef3ac8c68a6e14533ce9050ce5a72dc7f3c0`; primary profile
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`; host
  `741df2394607d2a16861ffe57588c7d4d39420c6a6d98c59c21ccb7293daabb2`. Host: Linux
  6.17.0-40-generic x86_64, AMD Ryzen 7 PRO 8840HS w/ Radeon 780M Graphics, 16 logical CPUs,
  rustc 1.95.0, cargo 1.95.0. This was a fresh-process cold in-process library initialization;
  immediately before launch the coordinator traversed, revalidated, and read all 88 runtime-profile
  files / 3,022,530 bytes into the filesystem cache.
- **Evidence.** Local artifacts:
  `target/wu0d-release/runs/20260718T102841Z-1606417/`. Verified SHA-256:
  `probe-01-primary-off.meta` =
  `339f7cf6ba25fb28cde4209a9d532d63f47b684f5dfe136ba8b4b280a89f9e9b`; stdout =
  `daf071121a9ffe32bbce44190e37a068c96d08bff63fd7b660dd95b77d786dca`; `run-facts.txt` =
  `9305ddce17dbea4acb7cfb6c0df5f3ae703b67d1d73cc8c45fe8670e4933c023`; Cargo JSON =
  `e0d342bc510f40cb294c2e72cba2b318477f20fb222650d37068b0944eb1e505`.
- **Observed stop.** The exact `primary/off` process failed the 5.00 s cold deadline. The
  coordinator sent TERM; the wrapper exited 143; after the 250 ms grace it attempted group KILL;
  no direct-PID KILL was attempted; the bounded drain completed. Supervisor elapsed was 5,255,404
  us total, including containment and reap; it is not a workload-completion time. Stdout was
  exactly 16 bytes (`\nrunning 1 test\n`); stderr and GNU time output were empty. Peak RSS, GNU
  elapsed time, phase checkpoint, and semantic identity are **UNAVAILABLE**; semantic equality was
  **NOT EVALUATED**. Candidate mode and every other probe were **NOT RUN**. The canonical
  30-process schedule — all five pairs across all three sets — and same-binary validator were
  not started.
- **Verdict and stop.** **HARD NO-GO:** WU0's cold `<=5.00 s` acceptance gate failed before a
  semantic result existed. No candidate comparison or release evidence may be inferred, and the
  full schedule was skipped. WU1–WU8 are not authorized; `src/prelude.ts` remains the production
  path, and the namespace refactor remains out of scope. The readiness manifest remains
  **PENDING**, this sprint remains active, and backlog 14 remains open. User direction is required
  before selecting or planning any replacement architecture or separately approved follow-up.

### 2026-07-18 — WU0E first contained diagnostic: interface-fill substitution blow-up

- **Diagnostic implementation.** Commits `23ece95`, `aa182b5`, `2e6e4fe`, and `df639c2` added the
  test-only WU0E observer, exact trace validator, frozen-binary scheduler, and delegated-cgroup
  containment. Independent review passed after correcting two retained-lifecycle/identity gaps.
  WU0E remains measurement-only and cannot authorize WU0D, alter its fixed 5-second/512-MiB gate,
  or enable WU1. Production still uses `src/prelude.ts`.
- **Frozen run.** Local artifacts:
  `target/wu0e-diagnostic/runs/20260718T200858Z-2267198/`. Release libtest identity
  `d086aca708db7dc563f4f387363caa3ed11d9c7ae156b7a191a2fcaa7440d039`; host identity
  `e06e1572a971db4a09f3c40a3db7e0da69f1848c6b06478d08463253443ab7b9`; profile identity
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`; inventory identity
  `bd4a7e8d3ae3facc5bfac2d2b906fcf75e6c456358f78b33a3bcd08992a3ecb5`. Dossier SHA-256 is
  `f226996069ed079406bb2a0d7d3167b570ae058e9afc8015686208e33f146987`.
- **Contained outcome.** `plain`, `measured-off`, and `candidate-b` each reached the 180-second
  coordinator deadline at 180.012, 180.012, and 180.011 seconds respectively. Sampled peak RSS was
  54,706,176, 54,722,560, and 54,870,016 bytes; all cgroups were removed, all OOM deltas were zero,
  and all same-binary partial validators completed in 35–45 ms. No mode produced a semantic digest.
- **Phase localization.** Every mode completed profile load, parse, bind, reservation, pass
  construction, and parameter metadata in about 126 ms, then remained in
  `fill-interface-scc` until containment. Statement checking was never reached. This falsifies
  binder cost and per-file statement accumulation as the first bottleneck. It also strongly
  falsifies broad provisional relation-cache starvation as this first barrier: interface-relation
  obligations are queued rather than evaluated in this phase, and relation work was not dominant in
  any of three profiles. The narrower cache-starvation hypothesis remains untested for later phases
  after interface construction becomes finite; it is not the first optimization.
- **Anchored profiles.** Three 20-second `cycles:u`, 99-Hz, DWARF-callgraph profiles were captured
  while their traces remained in `fill-interface-scc`; each contained about 2,000 samples with zero
  lost samples. `Substitution::apply` used 19.46% self cycles in plain, 17.49% in measured-off, and
  15.42% in candidate B. `Copied::next`, `Vec`/`String` cloning, malloc consolidation, allocation,
  and free dominated the remainder; glibc allocation/free internals accounted for about 38–40% of
  self cycles in every mode. Candidate B still reached no later phase, so the observed share shift
  is at most a local constant-factor signal, not authorization. A 10-second candidate-B
  `perf stat` window retired 126,031,873,160 instructions over 32,369,706,867 cycles (3.89 IPC),
  with 0.62% branch misses, 1.34% cache misses, and no page faults. The hotspot is instruction-heavy
  repeated semantic/copy work rather than a memory-latency or RSS failure.
- **Instrumentation confound.** Independent audit found that release libtests run global relation,
  inference, query/evaluator, mapped-type, and overload TLS counters even in WU0E `plain`;
  `measured-off` additionally measures every eligible substitution application, and mapped-type
  measurement maintains a real hash set. These hooks do not create recursive blow-up, but can
  amplify it and bias allocator/hash/self-time and candidate-B benefit. The next control is a
  separate compile-time, plain-only, diagnostic sidecar with identical profile and semantic digest;
  it remains ineligible for WU0D evidence.
- **Next evidence gate.** Before choosing an optimization, add bounded counters that distinguish
  repeated identical application keys, unique-key growth, cycle-tainted versus clean outcomes,
  total/deep apply visits, cloned properties/signatures/string bytes, and interner hit/insert rates.
  Run them over dependency-closed completing library ladders, not arbitrary file prefixes. The first
  optimization must explain at least 30% of cycles on repeated profiles, predict at least 20%
  end-to-end improvement, preserve the exact semantic digest, and demonstrate at least 20% median
  improvement across five interleaved fresh-process pairs without regressing controls by more than
  2%. WU0 remains **NO-GO** and WU1 remains blocked.

### 2026-07-19 — WU0 nonlinearity localized: within-run substitution cycle-taint blow-up

- **Method.** Plain release binary (no libtest, no feature instrumentation — sidesteps the recorded
  instrumentation confound), synthetic scaling fixtures, declaration-boundary-snapped real
  `lib.dom.d.ts` prefixes, DWARF perf profiles, and temporary uncommitted in-process counters
  (reverted; worktree clean).
- **Mechanism.** `Substitution`'s run-wide completed memo refuses every cycle-tainted result
  (`cycle_epoch` taint propagates to all open frames), and there is no free-type-param prefilter, so
  a substitution whose map touches nothing in a subtree still walks it. On a hash-consed graph a
  diamond (shared node) beneath a raw-id cycle therefore degenerates to path enumeration:
  exponential visits, flat RSS. Three-way synthetic isolation: acyclic diamond linear (memo works),
  cyclic chain without diamond linear (2,000 nodes / 0.13 s), cyclic diamond ×~4 per +2 depth
  (depth 26: 7.39 s). Per-visit `apply_object` property-vector + `String` clones explain the
  38–40% allocator share in the WU0E anchored profiles; the synthetic profile signature matches
  those profiles symbol-for-symbol.
- **Real-workload witness.** `lib.dom.d.ts` snapped-prefix ladder has its knee at
  `interface GlobalEventHandlers` (16,762–16,992): its addition makes
  `Document extends … GlobalEventHandlers` constructible and pulls the Document hub into the walked
  component — 0.12 s → 2.88 s from that single declaration, unbounded shortly after. In the
  2.88 s workload one deferred `check_constraint_arguments` substitution
  (map `{E → Element}`, backtrace captured) performed **54,727,613 visits over 459 unique memo
  keys** (~119,000× repetition; 6.9 M raw-id re-entries; 14.6 M tainted memo skips). The whole
  dom-alone workload runs only 25 substitutions — the blow-up is within-run, so cross-run caching
  (Candidate B) is not the first lever.
- **Causal proof.** A deliberately unsound diagnostic variant that memoizes tainted results
  unconditionally collapses that run to 2,656 visits (knee 2.88 s → 0.13 s; synthetic depth-26
  7.39 s → instant; full `lib.dom.d.ts` substitution totals become negligible). It is not the fix —
  stale reuse after a cycle root completes is semantically wrong — but it bounds the achievable win
  and pins the mechanism.
- **Second barrier unmasked: diagnostic type rendering.** With substitution neutralized,
  `render_type` dominates: it expands the hash-consed DAG structurally with only a cycle guard — no
  memo, no depth cap, no size cap, no nominal naming — and messages are formatted eagerly at
  emission (`finish_semantic_effects`). Observed single renders of 498 KB and 15.5 MB, growing
  unboundedly. With an emulated depth cap plus the memo experiment, the ordered 82-file
  concatenation **terminates for the first time**: 106.99 s / 11.36 GB peak RSS / exit 3
  (explicit incompletes, zero rendered errors — the render work was for messages never printed).
  Residual barriers past the first two remain unattributed and re-profiling is required after the
  first optimization lands.
- **Selection.** First optimization: **cycle-scoped tainted memo** inside `Substitution` — memoize
  cycle-tainted results keyed like the completed memo, tagged with the deepest re-entered
  in-progress depth, evicted when that frame exits, and taint-propagating on reuse. Sound (reuse
  only while every depended-on frame is live), local to `src/types/substitute/`, and semantics are
  byte-for-byte unchanged; only visit counts change. Predicted effect far exceeds the recorded
  ≥30%-of-cycles / ≥20%-end-to-end gate. Second (separate WU): lazy diagnostic rendering with
  depth/size caps and nominal display. Relation-cache H1 stays deferred. WU0 remains **NO-GO**;
  no gate is altered.

### 2026-07-19 — first optimizations shipped: substitution barrier removed

- **Shipped.** `5916b5f` (RED spec) + `fbde2aa` (cycle-scoped tainted memo) and `8f89d53` (RED
  spec) + `5f6408e` (param-relevant prefilter + internal-cycle fold), each behind an independent
  adversarial review (40k/60k-seed differential fuzz, official-suite reports byte-identical,
  conformance corpus byte-identical).
- **Effect.** The synthetic depth-26 cyclic diamond drops 7.4 s → 4 ms; the lib.dom knee workload
  (declaration-snapped 16,993-line prefix whose one heavy constraint-check run did 54.7M visits)
  drops 3.2 s → **0.13 s (~24×)**; the whole substitution phase disappears from the full-dom and
  82-file-concat profiles. fill-interface-scc is no longer the first barrier.
- **Distribution-guard audit.** The prefilter made the fully shadowed corner match
  `tsc --strict` by removing a spurious `TK2322` and corrupted display. An independent audit then
  found no reachable residual after the prefilter: 460 corpus files produced no blocked-guard
  reach, the original witness is clean in both typokat and tsc, and a guard-filtering diagnostic
  binary produced no output/status delta. The proposed ledger entry and backlog 84 filing were
  therefore withdrawn rather than preserving an unwitnessed issue.
- **Next barrier and bounded fix.** Eager, unbounded diagnostic type rendering
  (`render_type`: no depth/size cap; single messages ≥15.5 MB) dominated full `lib.dom.d.ts` and the
  82-file concat. Commits `2479357` and `eff7a8f` shipped a 320-scalar / 64-depth bounded renderer;
  standalone `lib.dom.d.ts` then completed in 2.03 s at 42,800 KiB maximum RSS. The temporary
  WU0C–WU0G attribution/debug tooling was removed by `ca1a68e` and `2153c83`; the WU0B prototype
  and authoritative WU0D gate remain. WU0 is still **NO-GO** because that unchanged 5 s / 512 MiB
  gate exits 143 after 5.268 s, so no loader implementation or cutover is authorized.
