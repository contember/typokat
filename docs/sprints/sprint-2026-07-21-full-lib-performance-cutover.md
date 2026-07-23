# Sprint — full default-library performance cutover (2026-07-21)

**Goal.** Ship the exact pinned TypeScript 6.0.3 ES2025 full-host library as typokat's production
default type universe, with sound user checking and a fresh-process end-to-end wall time at least
**2× faster than pinned native TypeScript 7** on the approved reference workload.

**Theme.** Backlog [`14`](../backlog/14-libdts-loading.md) is not complete merely because the 82
declaration files can finish in a test-only pipeline. The production CLI must actually use the
result, preserve every library-owned diagnostic/incomplete outcome, support the ADR-0011
base/delta and collision semantics, and win a fail-closed apples-to-apples benchmark. The previous
feasibility sprint removed two nonlinear barriers but left runtime library compilation at roughly
10.8 seconds. Native TypeScript 7.0.2 checks the same pinned library bytes in roughly 0.3 seconds,
so meeting the requested 2× target requires eliminating normal-startup compilation work rather
than polishing it by a constant factor. The leading design is a deterministic, shipped semantic
snapshot decoded into the same `FrozenLibraryBase`; it must earn a superseding ADR and an early
performance GO before production work proceeds.

## Refs re-verified at HEAD (2026-07-21)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The sole 1.0 library profile remains TypeScript 6.0.3's 82-file
  `lib.es2025.full.d.ts` closure: 2,936,611 bytes, 58,349 LF bytes, registry identity
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d` —
  `tests/lib_es2025_full_profile.rs:9-18`, `tests/lib_es2025_full_profile.rs:92-105`.
- ✔ Production still parses and checks `src/prelude.ts` inside every run through
  `PRELUDE_SOURCE` and `bootstrap_trusted_prelude`; there is no production `FrozenLibraryBase` —
  `src/check/checker/mod.rs:139-149`, `src/check/checker/mod.rs:210-224`.
- ⚠ The exact profile and source-backed `LibraryCompiler` are now production modules, and the
  canonical archive is packaged. The decoder/user-route feasibility oracle remains `#[cfg(test)]`;
  ordinary `check_source` still creates `Interner::with_intrinsics()` and calls the prelude-backed
  checker. There is no production base/provider yet — `src/library/`,
  `src/check/checker/library_snapshot_feasibility/`, `src/driver.rs`.
- ✔ The production compiler parses all 82 units and returns an AST-free owned runtime product plus
  exact evidence. Its feasibility follow-up route remains valid only for a caller-certified
  non-colliding source; it is neither the production base/provider nor WU5's private collision
  route — `src/check/checker/library_compiler.rs`.
- ✔ The obsolete 8.45 MB WU0D evidence projection and its release coordinator were removed after
  the canonical ten-section snapshot superseded them. ADR-0012 records the accepted snapshot
  artifact identity; the archived predecessor sprint retains the historical WU0D measurements.
- ✔ The B14 single-file and project corpora remain disabled — `tests/conformance.rs:175-179`.
- ⚠ Before the obsolete WU0D projection was removed, the exact release WU0B profile completed
  rather than hanging: one fresh run measured
  10,784,008 us internally / 10.94 s externally / 72,904 KiB RSS. The phase split was registry
  36,618 us, parse 17,087 us, bind 27,650 us, reserve/fill 793,025 us,
  publication/validation 68,090 us, and statement-check/evidence 9,833,246 us. The historical WU0D
  5-second gate therefore reported NO-GO; ADR-0012's decoded-snapshot route supersedes that gate.
- ✔ The current stable comparator is `typescript@7.0.2`; npm reports integrity
  `sha512-8FYau96o3NKOhbjKi/qNvG/W5jhzxkbdm5sj9AbZ/5T5sWqn3hJgLfGx27sRKZWTvyzCP8dLRBTf5tBTSRVUNA==`.
  Exploratory direct-native runs on this host checked the normal 83-file ES2025 graph in
  0.34-0.37 s and the explicit pinned TypeScript 6.0.3 82-file graph in 0.32-0.33 s. These are
  planning observations only; WU0 owns the frozen binary identity and authoritative interleaved
  distributions.
- ✔ ADR-0011 accepts the exact embedded profile, one immutable AST-free base, identity-preserving
  private deltas, a conservative preflight, and a correctness-first same-pipeline private rebuild.
  It also says the first initialization performs real semantic compilation, so replacing that
  work with a shipped snapshot requires a superseding decision before implementation —
  `docs/decisions/0011-freeze-pinned-default-library-base.md:286-315`,
  `docs/decisions/0011-freeze-pinned-default-library-base.md:317-339`.
- ⚠ The superseded ignored WU0 readiness bundle was removed when WU2 introduced its exact
  production artifact spec and fail-closed package gate. Production runtime readiness is still
  unproven until WU3–WU8 — `src/library/wu2_spec.rs`, `tests/library_package_assets.rs`.

## Binding performance claim

The sprint may close as shipped only with this narrow claim:

> On the recorded reference host, the ordinary production typokat CLI checks every approved
> fresh-process TypeScript 6.0.3 ES2025-full workload at least 2× faster than the pinned native
> TypeScript 7 executable, while using the same 82 library source bytes and preserving the approved
> semantic outcomes.

For each benchmark row and each of three independent trials:

```text
speedup = median(tsgo wall time) / median(typokat wall time)
```

The one-sided 95% bootstrap lower confidence bound of `speedup` must be at least `2.00`, and the
ratio of tsgo p95 to typokat p95 must also be at least `2.00`. The engineering target is `2.25×`
to leave noise headroom. The threshold is derived from the frozen comparator samples; no absolute
time from the exploratory runs is hard-coded. A failed row is NO-GO. The run log cannot weaken,
drop, rename, or replace a row after seeing its result.

The primary benchmark is fresh-process/compiler-cold with an ordinary warm filesystem cache. It
includes process creation, production CLI startup, default-library snapshot validation/loading,
user-source I/O, parse/bind/check, diagnostic construction where applicable, and normal shutdown.
It excludes downloading tools, building binaries, staging the comparator runtime, and generating
the shipped snapshot. Every measured sample is a new process; no daemon, incremental state, or
same-process singleton reuse counts toward the headline.

### Approved benchmark matrix

All rows use committed byte-pinned sources and the exact 82-file registry:

1. **fast-clean** — a non-colliding external module that exercises arrays/tuples, `Promise`,
   iterators/generators, `RegExp`, primitive/object/function members, DOM, and `Intl`; both tools
   exit 0 with empty output;
2. **fast-errors** — the same library surface with stable bad assignments/member accesses; both
   tools produce the approved normalized code/span identities, so performance cannot be won by
   skipping diagnostics;
3. **collision** — a legal script/`declare global` merge that forces typokat's correctness route
   and has an equivalent tsgo oracle;
4. **fanout** — a fixed mixed 32-file invocation covering shared-base reuse, independent deltas,
   and at least one collision, compared with the equivalent native TypeScript invocation.

The 2× claim applies to every row. If ADR-0011's private full rebuild cannot meet the collision row,
the sprint must stop and decide a sound faster collision architecture; it may not silently narrow
the claim to the easy fast path.

### Comparator and sampling contract

- Pin the current stable TypeScript 7 package at WU0 (`7.0.2` at sprint creation), npm integrity,
  upstream revision, platform artifact, direct native executable SHA-256, and size. If a newer
  stable TypeScript 7 ships before cutover, freeze it as a second comparator and require the gate
  against both; do not move the original baseline.
- Stage an untimed comparator runtime whose default library files are byte-for-byte replaced by
  typokat's vendored TypeScript 6.0.3 profile. `--listFilesOnly` must attest exactly the 82 manifest
  libraries plus the benchmark inputs. Timing uses normal default-library loading, not `--noLib`.
- Run the direct native executable, never npm/Node startup. Neither side may use `--skipLibCheck`,
  `--noCheck`, incremental state, a daemon, hidden environment switches, or benchmark-only code.
- Pre-read both executables, the profile, and workload before each block. Run five unrecorded
  warmups, then 30 measured launches per tool in 15 balanced `A,B,B,A` blocks. Repeat the trial
  three times in separate time windows under the same sanitized environment, cwd, CPU set,
  priority, resource limits, and normal thread availability.
- Record every raw monotonic-wall sample, median, mean, p95, MAD, min/max, and a deterministic
  100,000-resample bootstrap interval. Record ten separate interleaved `/usr/bin/time -v` memory
  samples per tool; typokat must stay below 512 MiB in every sample and at or below 1.25× tsgo's
  median RSS.
- The exact release binaries used for semantic proof and timing must be identical. Standard
  `cargo build --release` is required; unrecorded `RUSTFLAGS`, `target-cpu=native`, PGO, or feature
  gates invalidate the claim.
- Rename/comment perturbation controls must take the same route. No filename, fixture hash,
  reachable-surface, output-suppression, lazy-subset, test-only, or WU0-injection special case is
  allowed. A shipped snapshot is valid only if every ordinary user source uses it.

## Work units

### WU0A — freeze the cross-tool contract and RED acceptance (effort L)

- **Problem.** Existing `tooling/bench` intentionally uses `--noLib --skipLibCheck`; WU0D is a
  typokat-only libtest with a five-second absolute gate. Neither can prove the requested production
  CLI ratio, and benchmark design after implementation would invite target selection.
- **Verify first.** Re-run the exploratory direct-native comparator, verify npm/upstream/platform
  provenance, exercise exact 82-file staging, and enumerate every difference between the two
  commands. Confirm all four proposed rows have stable TypeScript 6.0.3 and 7.0.2 outcomes.
- **Scope.** Commit, before implementation:
  - `tooling/full-lib-bench/` with byte-pinned sources, lock manifest, expected file inventory,
    semantic oracles, a fail-closed runner spec, and disabled RED production-path assertions;
  - a canonical runner protocol with bounded stdout/stderr, timeout/process-group containment,
    sanitized environment, balanced scheduling, deterministic statistics, binary/host/profile
    identities, and raw evidence format;
  - anti-gaming tests for stale/wrong binaries, wrong library bytes/order, extra default libs,
    forbidden flags, warm-state reuse, malformed output, renamed fixtures, and partial schedules;
  - the exact target formula, matrix, memory gates, and artifact schema in the tooling README.
- **Acceptance / witness.** The comparator-only self-tests and TypeScript oracles pass; the
  typokat production assertions are demonstrably RED because the CLI still uses `src/prelude.ts`.
  The runner cannot emit GO without all rows, three complete trials, semantic parity, memory
  evidence, and exact binary/profile identities.
- **Touch points.** `tooling/full-lib-bench/`, `tests/cases/b14_full_lib_loading/`,
  `tests/cases/b14_full_lib_loading_project/`, `tests/conformance.rs`, packaging manifests.

### WU0B — semantic-snapshot feasibility and decision gate (effort L)

- **Problem.** Runtime WU0B compilation is about 65-75× above the likely 2× target. Its 8.45 MB
  canonical evidence blob is not a runtime base and includes source/reporting material that a
  normal check must not decode.
- **Verify first.** Inventory every field required by ADR-0011's `FrozenLibraryBase`, distinguish
  durable query inputs from WU0 evidence/probes, and prove the existing compiler can project a
  deterministic pointer-free archive without changing semantic identities.
- **Scope.** Behind test-only/explicit tooling boundaries, produce a representative versioned
  snapshot and strict owned decoder generated by the same library compiler. Exclude source bodies,
  phase counters, benchmark data, rendered diagnostics, and probe-only indexes. Measure validation,
  decode, base construction, one clean user check, artifact size, and RSS. Independently
  regenerate twice from clean builds and byte-compare the outputs.
- **Acceptance / witness.** A fresh-process prototype using the decoded base—not `check_source`'s
  old prelude—passes the fast-clean semantic matrix and leaves at least the full 2× statistical
  target plus engineering headroom against the frozen comparator. Snapshot identity changes for
  every semantically relevant mutation and remains identical for clean regeneration. Decode
  corruption/truncation/unknown-version tests fail closed before user checking.
- **Stop/falsifier.** If the real decoded-base path cannot plausibly achieve the cross-tool gate,
  or the archive cannot represent the complete AST-free base without a second semantic authority,
  record NO-GO and stop. Do not begin WU1, weaken 2×, or substitute a lazy surface slice.
- **Touch points.** `src/check/checker/wu0b_library.rs`, a test-only snapshot decoder module,
  `tooling/library-profile/`, `tooling/full-lib-bench/`, full-profile fixtures.

### WU1 — decide and pin the shipped snapshot architecture (effort M)

- **Problem.** ADR-0011 requires real semantic work during first initialization. A checked-in
  semantic snapshot changes provenance, release generation, runtime validation, corruption
  behavior, and the meaning of the private rebuild path.
- **Verify first.** Adversarially review WU0B's product completeness, reproducibility, performance,
  binary-size cost, and whether the same compiler remains authoritative for snapshot generation
  and private rebuilds.
- **Scope.** Write and accept a superseding ADR only after WU0B GO. It must define the versioned
  internal format, generator authority, checked-in/package lifecycle, digest binding to profile +
  checker schema, runtime validation, upgrade/rollback policy, source retention for private
  rebuilds, and exactly which ADR-0011 guarantees remain unchanged.
- **Acceptance / witness.** The decision has one authoritative generator and decoder, no parallel
  hand-authored semantics, and an explicit exit plan. If snapshot generation or validation becomes
  non-reproducible, production can return to the source compiler behind the same typed provider,
  but that fallback cannot claim or silently bypass the 2× gate.
- **Touch points.** New `docs/decisions/0012-*.md`, ADR/readme indexes, this sprint run log.

### WU2 — production LibraryCompiler and canonical snapshot (effort XL)

- **Problem.** The current compiler/reporting/profile modules are test-only and return an evidence
  structure rather than a production archive.
- **Verify first.** Split WU0 evidence-only projections from the minimum complete runtime product;
  prove parser diagnostics, library event ownership, publication terminals, and source identities
  are not lost by the split.
- **Scope.** Promote one source-backed `LibraryCompiler` used by explicit snapshot generation and
  private rebuilds. Generate the versioned canonical snapshot deterministically from the exact 82
  sources, retain semantic diagnostics/incomplete identities separately from runtime tables, bind
  the artifact to compiler schema/profile/checker revisions, and package the snapshot plus sources
  and upstream notices. Generation is explicit and untimed; normal `cargo build` does not silently
  regenerate or trust a stale blob.
- **Acceptance / witness.** Clean regeneration is byte-identical; any compiler/profile/schema
  mutation invalidates verification; package extraction contains exact assets and notices; the
  generated semantic identity matches a fresh source compilation. No production call can select a
  different library pipeline.
- **Touch points.** New `src/library/` production modules, existing WU0 modules, profile tooling,
  `Cargo.toml`, package tests, `src/library/typescript-6.0.3/`.

### WU3 — strict decoder and immutable FrozenLibraryBase (effort XL)

- **Problem.** Production needs typed immutable tables, not an opaque evidence `Vec<u8>`, and no
  partially decoded base may become observable.
- **Verify first.** Enumerate every `Store`, interner bucket, constraint, publication, binder/scope,
  declaration, namespace, class, value, intrinsic identity, root-name index, and next-id field
  required by arbitrary user checking.
- **Scope.** Decode into a fully owned `FrozenLibraryBase` behind
  `Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>`; validate header/version/length/order/
  ranges/IDs/references/hash tables/terminals/digests before publication. Prove
  `Send + Sync + 'static`, no allocator/AST/pass/query cache survives, and base rows reference no
  delta. Publish only after the complete object validates.
- **Acceptance / witness.** Mutation/truncation/reordering/overflow/unknown-tag tests fail closed;
  direct inspectors prove the source-compiled and decoded bases have identical canonical semantic
  projections; 1, 2, and 32 callers receive one pointer-identical base; repeated fresh processes
  produce the same identity. The WU0B performance headroom survives on the production decoder.
- **Touch points.** `src/library/`, `src/types/`, `src/binder/`, `src/check/checker/`, driver-facing
  typed provider API.

### WU4 — identity-preserving user delta (effort XL)

- **Problem.** A shared base is unusable if user runs clone/remap it or if interning can create a
  duplicate of an existing base type.
- **Verify first.** Inventory every arena/table ID domain and every direct indexing/interning path;
  use compile-fail/private-field tests to prevent construction outside the provider.
- **Scope.** Add immutable-prefix + private-delta views for the type store, interner buckets,
  constraints, binder scopes/symbols/declarations/groups/namespaces/value storages, declaration
  values, class IDs, and type parameters. Reads route by prefix; interning probes base before local;
  base rows never reference/mutate a delta; no delta is shared between runs.
- **Acceptance / witness.** Existing shapes reuse exact base IDs, new shapes allocate after frozen
  counters, unrelated runs cannot observe each other's rows, and single/parallel/project user
  checks retain deterministic diagnostics. Warm inspectors show zero library parse/bind/check work,
  zero base-sized clone/remap, and per-check allocation independent of base size.
- **Touch points.** `src/types/`, `src/binder/`, `src/check/checker/`, `src/driver.rs`, direct tests.

### WU5 — collision routing and fast private semantics (effort XL)

- **Problem.** User scripts and `declare global` can merge with the library. ADR-0011's private
  full rebuild is sound but the current source compiler cannot satisfy the 2× collision row.
- **Verify first.** Re-run the exact binder classifier over every committed conformance and
  official-suite input; freeze route incidence before optimizing. Measure which library
  declaration SCCs actually depend on each collision family and whether a snapshot-derived replay
  can preserve same-universe identity without becoming a second publication authority.
- **Scope.** Implement the exhaustive preflight before any semantic mutation. Keep the shared fast
  path for provably non-colliding inputs. For collisions, use the same `LibraryCompiler` authority;
  optimize only through a separately specified and reviewed mechanism that reconstructs the exact
  merged universe. If the accepted solution differs from ADR-0011's private full rebuild, supersede
  that clause before implementation. Retain one-process-wide containment for expensive fallbacks.
- **Acceptance / witness.** False-negative classifier mutations route private; legal merge,
  `globalThis`, UMD/namespace, value/type/namespace-slot, destructuring, and opposite-order cases
  match tsc. The collision and fanout benchmark rows each satisfy the full 2× confidence/p95 gate,
  every run stays within 512 MiB, and all-colliding fanout is deterministic and bounded.
- **Stop/falsifier.** No 2× collision result means no broad 2× claim and no sprint completion. Do
  not relabel the collision row out of scope or accept the unaugmented snapshot as success.
- **Touch points.** `src/library/`, binder preflight/classifier, checker/compiler pipeline,
  `src/driver.rs`, B14 project fixtures, routing/readiness manifests.

### WU6 — identity-selected bridges and full library corpus (effort L)

- **Problem.** Arrays, regexp literals, primitive wrappers, utility intrinsics, `globalThis`, and
  `undefined` need universe-local library identities; name-based exceptions or an error-type
  fallback would create false-clean checks.
- **Verify first.** Re-audit the committed bridge matrix against the decoded base and current
  consumers; require concrete evidence for every native bridge and reject unused special cases.
- **Scope.** Wire only evidence-selected identities from `LibrarySemanticIdentities`; enable the
  B14 single-file/project corpora; promote exact official-suite missing-library witnesses; preserve
  all unrelated model incompletes under their existing owners.
- **Acceptance / witness.** Positive and negative arrays/tuples, `Promise`, iterators/generators,
  DOM/Intl, regexp, primitives/object/function, utilities, shadowing, and augmentation cases match
  TypeScript 6.0.3. No `TK2304` remains for a present standard-library declaration, no unsupported
  surface becomes `any`/error/empty success, and module-local same-name declarations cannot hijack
  native identities.
- **Touch points.** `src/check/checker/`, evaluator/annotation bridges, B14 corpora,
  official-suite gates, bridge/ledger/readiness manifests.

### WU7 — production provider, CLI cutover, and batch protocol (effort XL)

- **Problem.** All public driver modes still bootstrap the minimal prelude, use infallible APIs,
  and the official suite pays one process per case.
- **Verify first.** Characterize `check_source`, `check_files`, `check_project`, CLI exit behavior,
  spawn/join failures, and every consumer before changing signatures.
- **Scope.** Add the process-wide typed provider/singleton, initialize before rayon, and migrate all
  three driver modes to Result-bearing APIs. Cache deterministic init failure, map it to stable CLI
  exit 2 without partial user output, add the isolated same-process official-suite protocol, and
  atomically remove `PRELUDE_SOURCE`, `bootstrap_trusted_prelude`, and `src/prelude.ts` only after
  every path uses the full base.
- **Acceptance / witness.** The normal release CLI—not a libtest—uses the decoded base for all
  benchmark and conformance sources. Initial/middle-case failure, crash, timeout, malformed frames,
  case-id mismatch, worker failure, and restart tests fail closed without cross-case leakage.
  Single/parallel/project semantics agree and package/source searches find no production prelude
  fallback.
- **Touch points.** `src/library/`, `src/driver.rs`, `src/check/checker/mod.rs`, `src/lib.rs`,
  `src/main.rs`, API call sites, official-suite protocol, deletion of `src/prelude.ts`.

### WU8 — authoritative 2× gate and optimization loop (effort XL)

- **Problem.** Prototype timing cannot support a production performance claim, and optimizing only
  the easiest row would violate the contract.
- **Verify first.** Freeze the exact release commit and binaries; run semantic, identity, route,
  package, conformance, official-suite, warm-sharing, and memory gates before collecting timing.
- **Scope.** Run the complete cross-tool protocol. If any row misses, profile that exact production
  row, write a new RED performance/semantic guard, implement one evidence-backed optimization via a
  subagent, independently review it, and repeat the entire matrix. Keep raw failures; never replace
  evidence after learning which row is slow.
- **Acceptance / witness.** All three trials for every row have a one-sided 95% lower confidence
  bound and p95 ratio ≥2.00; all semantic outputs/identities match their oracles; typokat RSS meets
  both memory gates. Commit raw canonical JSON, summary, binary/profile/host facts, commands,
  snapshot and package sizes, route incidence, and independent statistical validation.
- **Stop/falsifier.** A semantic difference, incomplete evidence, identity mismatch, forbidden
  optimization, missing row, or sub-2× result is NO-GO. The target is not averaged across rows or
  traded against memory.
- **Touch points.** `tooling/full-lib-bench/`, production hot paths selected by profiles,
  readiness/routing/freeze artifacts, sprint run log.

### WU9 — independent adversarial review and closure (effort L)

- **Problem.** Snapshot trust, base/delta identity, collision routing, cache order, and benchmark
  asymmetry can all create a fast false-clean checker.
- **Verify first.** Give a fresh reviewer the frozen inputs, ADRs, specs, complete diffs, raw
  benchmark evidence, route logs, and TypeScript oracles without implementation guidance.
- **Scope.** Hunt false negatives, partial/corrupt base exposure, stale snapshot acceptance,
  source/snapshot semantic drift, base/delta aliasing, route misses, private/shared identity leaks,
  initialization races, output suppression, official batch leakage, and benchmark gaming. Route
  fixes to separate implementation agents and repeat every affected review/gate.
- **Acceptance / witness.** Independent PASS with zero unresolved HIGH findings; full `cargo test`,
  clippy, format, B14 corpus, official-suite ratchet, package verification, all ADR gates, and the
  complete 2× matrix pass on the final identical binary. Then delete backlog 14, mark `D-libdts`
  complete, update living reference docs/README, archive this sprint with exact commit/evidence
  maps, and remove obsolete WU0-only tooling.
- **Touch points.** Whole diff and evidence tree; docs/backlog/manifest/reference/archive indexes.

## Out of scope (explicit)

- Bundler/package/`node_modules` resolution and broader module semantics — backlog
  [`15`](../backlog/15-modules-imports.md).
- Parallel mutable cross-file export identity and incrementality — backlogs
  [`16`](../backlog/16-parallelism-type-universe.md) and
  [`17`](../backlog/17-incrementality.md).
- Alternate TypeScript versions, `--lib` selections, targets, NodeNext, and host profiles. The sole
  product profile remains the exact pinned TypeScript 6.0.3 ES2025 full-host closure.
- Fixing unrelated model/checker gaps (`50`, `75`, `63`, etc.). Their explicit unavailable/
  incomplete outcomes must survive loading; this sprint must not approximate them to make the lib
  appear clean.
- Emit, JavaScript/JSDoc checking, language service/API work, and the deferred bytecode VM.
- The independent namespace binder refactor sprint. It may proceed separately but is not a loader
  prerequisite or a source of assumed performance.
- Claims across arbitrary hosts/projects. Closure may claim only the pinned reference host and
  approved workload matrix; broader claims require separately committed evidence.

## Decisions

### Real fork

The decision is not whether to memoize the current runtime compiler. It is whether the immutable
standard library is rebuilt from 2.9 MB of source in every fresh process or shipped as a
reproducible semantic product while retaining source compilation as the correctness authority.

Weighted axes: **soundness and reproducibility** > **end-to-end performance / benchmark integrity**
> **consistency with ADR-0011** > **reversibility** > artifact size and implementation speed.

1. **Deterministic shipped semantic snapshot — recommended.** Best when the profile is fixed and
   immutable, the normal fast path dominates, and runtime compilation is orders of magnitude above
   the target. It removes parse/bind/check from startup while preserving the source compiler for
   generation and private semantics. Cost: a versioned format, generator/decoder, package bytes,
   and a superseding ADR.
2. **Keep optimizing runtime compilation.** Best when profiling shows a bounded local algorithm can
   cross the target without removing semantic work. HEAD contradicts that condition: roughly
   9.83 seconds sits in statement-check/evidence versus a likely ≤0.15-second target. Keep runtime
   optimization for collision/private work, not as the assumed fast-path plan.
3. **Lazy/reachable library slices.** Best for a product whose contract permits selectable or
   demand-loaded libraries. It is not valid here: the accepted product is one complete frozen
   82-file base, and a tiny benchmark must not omit untouched declarations. Reconsidering this
   requires a new product/semantics decision, not a sprint-log shortcut.

**Recommendation.** Attempt the shipped snapshot behind WU0B's reversible test-only gate, then
write the superseding ADR only on measured GO. Confidence is medium-high: the fixed immutable
profile makes precomputation natural, but the complete production decoder and collision row remain
unproven. This recommendation is wrong if a complete snapshot-backed user check cannot retain
enough headroom for the statistical 2× gate, or if decoding requires a second semantic authority.
In that case archive the sprint NO-GO and re-plan; do not weaken the target.

## Sequencing and commits

1. WU0A benchmark/RED spec commits alone. WU0B test-only snapshot feasibility follows, then an
   independent adversarial review. Failure stops the sprint.
2. On WU0B GO, WU1 records the architectural decision before production implementation.
3. WU2 generator/archive and WU3 decoder/base proceed spec-first; each gets its own RED commit,
   implementation subagent, independent false-negative review, and leader verification.
4. WU4 delta and WU5 routing/private semantics are sequential identity boundaries. Their specs may
   be prepared in parallel, but implementations and reviews follow dependency order.
5. WU6 bridges/corpus can prepare its oracle matrix while WU4-WU5 run, but enables no fixture until
   the owning production path exists.
6. WU7 performs one atomic driver cutover. WU8 runs only on that ordinary production binary and may
   iterate evidence-backed optimizations without changing the frozen contract.
7. WU9 reviews the complete final system and closes backlog/docs only after every semantic,
   performance, memory, package, and official gate passes.

Every semantic or performance implementation follows the mandatory corpus/spec → implementation
subagent → independent adversarial review loop. The leader writes and commits specs separately,
supervises agents, re-runs the final gates, and commits explicit paths only.

## Run log

<!-- Append discoveries/deviations/blockers. Graduate each entry: changed rationale → ADR;
     future work → backlog; transient → leave it for archive. Never weaken a hard gate here. -->

### 2026-07-21 — planning baseline

- Unrestricted current WU0B completes in 10.784 s internally / 10.94 s externally / 72.9 MiB;
  statement-check/evidence accounts for 9.833 s and reserve/fill for 0.793 s.
- Stable native TypeScript 7.0.2 exploratory runs are roughly 0.3 s on the same profile class;
  authoritative comparator staging, hashes, matrix, and distributions belong to WU0A.
- Three independent planning angles converged: a normal-startup source rebuild has no credible
  constant-factor path to 2×; a complete deterministic shipped snapshot is the only current
  candidate worth a bounded feasibility spike. The adversarial review requires collision/fanout,
  semantic parity, memory, and normal-CLI evidence so the benchmark cannot be won by a fast but
  unusable path.

### 2026-07-22 — WU0A contract and WU0B semantic closure

- WU0A froze the fail-closed cross-tool matrix and production RED gate at `daee751`. WU0B then
  promoted the owned full-profile runtime state and strict type/binder/checker codecs, closed
  reservation, forward-operator, inference, nominal/structural, and native-member semantic gaps,
  and reached the exact decoded 82-file user route at `16d323e` through `a4e4043`.
- Two configless offline release builds now come from distinct tracked-source copies with canonical
  all-scope path remaps and must produce byte-identical libtests. Commits `88f9129` through
  `3cc666f` hardened build/source/toolchain provenance; `6ec41a9` and `2d6e1e4` pinned the WU0A
  oracle keys. The complete preflight is 17 PASS / 4 ignored coordinator probes.
- The deterministic archive is 10,003,957 bytes, SHA-256
  `af97017b22c9f8ff3726de9dbd49a3039cf70f2dd5a4fd9df9f71328be721dd0`, with 296,414 typed
  references. Its exact profile, schema, wire inventory, projection witnesses, clean/error
  semantics, and independently regenerated bytes agree. Generic mutation/corruption decoders
  remain fail-closed; the canonical timing route admits only the compile-time-pinned full-file
  identity.

### 2026-07-22 — WU0B performance iterations and final GO

- The first complete prototype rebuilt 31 Debug projection subtables during every decode. The
  three-window run `wu0b-final.json` was a genuine NO-GO at 953.049 ms overall p95 and about
  125 MiB RSS. Phase attribution found projection reconstruction at roughly 340–361 ms per base;
  no interner collision/memo pathology existed (25,768 buckets, one size-two bucket).
- Commits `e92bd95` through `fde4d16` moved exact projection witnesses to untimed generation.
  Commits `c6516c0`, `d9da6a1`, and `cf89045` then separated full clean/errors calibration from
  the measured route, which now performs exactly one validation, one eager decode, and one clean
  user check. Raw records remain artifact/profile/WU0A-bound and reject duplicate, malformed, or
  trailing payloads.
- Commits `5c092c1` and `4ca587a` pinned and owned the exact archive, replaced redundant body plus
  section hashing with one full-file digest, and replaced global reference-manifest rebuild/sort/
  reserialization with an exact streaming comparison. Generic adversarial reconstruction remains
  available. Validation fell to about 6.3 ms and canonical decode to about 66 ms.
- The first complete canonical run reached all 45 timing children but remained NO-GO:
  `wu0b-canonical-final.json` recorded window p95s 124.163 / 122.758 / 121.527 ms. This missed the
  unchanged 120 ms gate by 1.5–4.2 ms; median external walls were 116.483 / 117.794 / 115.772 ms.
- Commits `4c58523` and `00ac0d0` overlap only independent canonical interner/binder decoding and
  immutable reference enumeration in two measured scoped joins. Independent review confirmed
  fixed interner-before-binder error handling, no partial publication, unchanged generic decoding,
  and no claim of parallel user checking.
- **WU0B GO:** authoritative `wu0b-parallel-final.json` completed two byte-identical release
  builds, two 17/4 preflights, two byte-identical calibrated regenerations, and three windows of
  five warmups plus ten recorded fresh processes. Overall external nearest-rank p95 was
  **110.409 ms**; window p95s were **110.913 / 106.753 / 110.409 ms** and medians were
  103.425 / 102.761 / 105.655 ms. Maximum externally observed RSS was **57,836 KiB**. Recorded
  median internal validation/decode/user-check ranges across windows were 6.307–6.513 /
  51.628–52.739 / 31.722–33.009 ms. Release libtest SHA-256 was
  `1f80b8f9a5e4fcfb8f003c631eeb56a55b6f991b444deaffbd8eb33665eb0731`; evidence contract SHA-256
  was `f5488b831c35cea04e42d5f9a0527e59b7105ed8e523278a0f8248639d16e61b`.
- This GO authorizes WU1; it does not ship a production loader or prove the final cross-tool 2×
  claim. Semantic on-demand stdlib loading remains rejected. A later physical immutable-indexed
  representation may be profiled only over the same complete authenticated base. Parallel mutable
  cross-file identity remains backlog 16 Stage 2; WU0B only established measured parallel decode
  of one immutable base.

### 2026-07-22 — WU1 canonical snapshot decision

- [ADR-0012](../decisions/0012-ship-the-canonical-default-library-snapshot.md) is Accepted. It
  narrowly supersedes ADR-0011's runtime-source initialization choice while retaining the one
  `LibraryCompiler`, base/delta identities, preflight, and private combined collision compilation.
- V1 binds the exact eager archive and bounded initialization parallelism. Semantic lazy loading
  remains rejected. Production must replace prototype panics with typed initialization errors, and
  the WU0B non-collision continuation must not be promoted as WU5's private combined universe.

### 2026-07-22 — obsolete WU0 cleanup

- Removed the superseded WU0D candidate runner, its external coordinator, and the unused seven-
  section evidence projection from the WU0B source compiler. Removed the disconnected WU0B
  reporting RED fixtures; active reporting and provenance tests remain.
- Renamed the active binder provenance test by behavior. The five active WU0B compiler/profile/
  snapshot files retained their evidence-bound names pending the WU2 replacement.

### 2026-07-22 — WU2 production compiler and canonical package

- RED commits `99fa60d` and `9a09563` pin the source-backed compiler/product boundary, exact
  10,003,957-byte artifact, semantic evidence counts, and explicit product-to-archive generation.
  Implementation commit `d533848` promotes `ExactLibraryProfile` and `LibraryCompiler`, packages
  the canonical archive, and groups the remaining test-only oracle under the descriptive
  `library_snapshot_feasibility` namespace. The stale ignored WU0 readiness bundle was retired.
- `263f9ab` adds the fail-closed package verifier and a required CI job. The final clean-tree gate
  regenerated the archive in two isolated clones, required byte identity and the pinned digest,
  validated both package inventories and upstream notices, and ran two offline extracted-package
  checks: 1 PASS / 0 fail in 210.97 seconds. The adversarial Python contract suite passed 11/11.
- Independent WU2 re-review found no unresolved HIGH issue. It confirmed one source compilation per
  generation, zero source/compiler activity inside product-to-archive conversion, strict no-follow
  tree and archive handling, exact profile/notice inventory, and mutation/custom-build rejection.
  Its sole MEDIUM finding—package verification absent from CI—was fixed by `263f9ab` before the
  clean-tree gate.
- WU2 does not change the ordinary user route: `src/prelude.ts` remains production and the decoded
  snapshot is not published. WU3 must now replace the test oracle with a typed, fail-closed,
  pointer-identical `FrozenLibraryBase` provider before any CLI cutover.

### 2026-07-22 — WU3 immutable base and source-cold checkpoint

- RED commits `87d2b56`, `c02576b`, and `075b397` pin strict admission, complete typed
  reconstruction, pointer-identical success and failure for 1/2/32 callers, dense identity
  prefixes, AST-free ownership, and the external release gate. `3808a37` publishes the production
  `FrozenLibraryBase` and instance-scoped provider; `4f9f306` adds the fail-closed fresh-process
  coordinator. The promoted codec and artifact tests now use semantic names rather than WU0/WU2
  implementation labels.
- Independent review found and closed four HIGH issues before implementation acceptance: the timed
  probe re-encoded the full archive, type-parameter/class terminal prefixes admitted gaps, one
  generic decoder fixture bypassed the canonical publication contract, and infallible/partially
  joined worker spawning could escape the typed cached-failure boundary. Final re-review is PASS
  with zero unresolved HIGH or MEDIUM findings. The complete suite passes: 962 library tests,
  17 ignored release probes, and every integration/conformance test; clippy and format are clean.
- The authoritative clean-tree gate at `4f9f306` built two isolated offline release libtests with
  identical SHA-256 `663bc74d1c64d3a7d80fcdac708c9d6f4c264763275309904d1c123cc98c194e`,
  then ran three windows of five warmups plus ten samples. Window medians were **96.434 / 97.198 /
  96.677 ms**, nearest-rank p95s were **102.076 / 103.919 / 101.825 ms**, and maximum RSS was
  **41,156,608 bytes**. The 120 ms / 512 MiB WU3 gate passes with no source reads, compiler calls,
  generator calls, retained archive, or retained projection witnesses.
- WU3 proves a valid product cache, not a generally faster checker. A separate source-cold profile
  therefore becomes a mandatory checkpoint before WU4: preliminary release attribution puts
  parse and bind below one percent each and roughly 90% in semantic/evidence work. The first RED
  core benchmark must reproduce the DOM listener-map pattern (`K extends keyof EventMap`,
  `EventMap[K]`, recursive receivers, overloads, heritage, and a wide shared hub) and pin traversal,
  exhaustion, memo-copy, semantic-order, and scaling bounds. Snapshot-backed and source-cold
  results remain separate claims throughout the sprint.

### 2026-07-22 — source-cold semantic hot-path repairs

- `41585cc` added the missing recursive DOM listener-map/shared-heritage synthetic witness. Its
  exact 82-file source-cold baseline was **4.8278 s** total / **4.3737 s** semantic; parse and bind
  remained below one percent each, locating the regression in semantic query work rather than
  library I/O or front-end construction.
- `ac9c1e9` caches publication-clean traversals without changing poison-first ordering or mutation
  invalidation. Publication edge visits fell from **15,450 / 61,338** at the two witness sizes to
  **85 / 85**, while the exact source-cold compile fell to **3.5211 s** total / **3.0675 s**
  semantic. `8a2c4ee` then replaced per-query durable evaluator-memo copies with a borrowed parent
  plus local delta; the witness's durable seed copies fell from **1,025 to zero**.
- `bf2fb24` held the type graph fixed while scaling repeated identical failures. Before the next
  repair, 96 additional assignments caused **+384** clean zero-write planner transactions,
  **+384** durable false-reason rebuilds, and **+4,320** uncached relation frames, with no graph
  growth, taint, or exhaustion. The spec review found that fixed-graph identity/cardinality was
  inferred rather than asserted; the final RED added the explicit equality guard before commit.
- `831c0c3` adds pass-local completed top-level relation outcomes, second-touch admission for
  negative reason chains, and shared invalidation with Store/publication identity. The fixed-graph
  deltas are now zero: both sizes retain **2** negative certificates while cache hits rise from
  **28 to 124**. Independent review kept unions out of the ordinary-demand fast path because they
  can carry durable evaluator normalization and added a production-path regression. It also found
  a HIGH stale-durable-true risk after Store/publication identity changes; refresh now runs at every
  coordinator entry and clears projection, evaluation, relation, publication, certificate, and
  admission state, with constraint Yes-to-No, publication replacement, and non-relation demand
  regressions. Telemetry now includes certificate/admission fork writes, and one-shot negative
  outcomes use second-touch admission: 128 unique failures retain zero reason chains. Final
  independent review is PASS with no unresolved HIGH or MEDIUM findings; the semantic/full gate
  passes **991 / 0 / 18**.
- The first five-run diagnostic source-cold checkpoint after `831c0c3` had medians of **3.499665 s**
  total and **2.996934 s** statement-check. RED commit `c052e93` isolated repeated effective
  binder-environment construction, and `a2cd299` interns those environments behind semantic
  identities. The five totals were **2.850369 / 2.755217 / 2.855423 / 2.801402 / 2.809538 s**
  (median **2.809538 s**); statement-check times were **2.391928 / 2.345213 / 2.442220 /
  2.393001 / 2.398403 s** (median **2.393001 s**). Attribution confirms the intended repair:
  binder frames scanned fell from **2,215,499 to 11,549**, flattened environment entries from
  **12,793,760 to 86,992**, and environment sort items from **6,716,994 to 52,001**.
- RED commit `c9d0777` then specified repeated contextual relations and implementation commit
  `96970ff` reused completed contextual `Yes` outcomes. Both this iteration and the binder-
  environment iteration completed independent adversarial review and the required semantic/full,
  format, and clippy gates without an unresolved HIGH or MEDIUM finding. The binder iteration's
  library gate passed **998 / 0 / 18**; the final contextual-memo gate passed **1004 / 0 / 18**.
  The five post-`96970ff` totals were **2.812432 / 2.710030 / 2.748137 /
  2.773897 / 2.736670 s** (median **2.748137 s**); statement-check times were **2.406788 /
  2.315922 / 2.339626 / 2.355644 / 2.331062 s** (median **2.339626 s**). Every run preserved
  exactly **273 diagnostics / 610 incompletes**.
- The contextual memo reduced uncached frames from **1,106,680 to 681,094** and stack-key builds
  from **1,107,894 to 682,545**, but produced only **237** high-level hits from **8,814**
  admissions. Object-property work remained unchanged at **188,486 target / 202,630 source**
  counters, which explains the comparatively small **~2.2%** wall-time improvement after binder
  interning. Across the source-cold repair series, the original **4.8278 s** total is now
  **2.748137 s**, about **43.1% lower**.
- Lazy standard-library semantics is not the next fix: the measured parse/bind share is below one
  percent, while the accepted product is one complete immutable 82-file ambient universe.
  Parallelism remains valuable after the frozen base and identity-preserving private deltas allow
  independent user files to share it, but it cannot substitute for removing the redundant
  serialized semantic work exposed here.
- The exploratory pinned native TypeScript runs remain **0.32-0.33 s**, so source compilation is
  still roughly **8.3-8.6x slower**. That comparison is non-authoritative: the ordinary CLI has not
  yet cut over to the provider, and WU8 still owns the official cross-tool collector and claim.
  Snapshot-provider startup remains roughly **96-97 ms**; the next implementation step is WU4's
  identity-preserving user delta, not further source-cold tuning.

### 2026-07-23 — WU4 identity-preserving user delta

- RED commits `e01dece` and `b0233b1` specify a private single/project delta, exact cross-file
  identity, deterministic diagnostics, physical work receipts, and allocation/traversal invariance
  between a 2-row and 4,098-row synthetic base. Follow-up specs `a8b1f30` and `8f75bc2` preserve a
  supported frozen alias and the canonical project diagnostic display.
- Implementation commit `53ad026` gives Store, interner, binder, and checker state an immutable
  shared prefix plus mutable local suffix. Existing shapes retain exact frozen IDs, new IDs stay
  dense after the base counters, and two sequential, 32 parallel, and project checks share no user
  rows. All 31 post-Pass state families retain their `Arc` or scalar base identities.
- The adversarial scan audit also removed continuation-time whole-prefix work from namespace
  attachment/materialization, merge and declaration-site lookup, source-key and symbol counts,
  placement/global/UMD reporting guards, reference enumeration, and direct borrowed iteration.
  The physical work ledger reports no base-sized copy, remap, or traversal on the accepted user
  route; local row counts and the 2-versus-4,098 sentinel remain constant.
- Independent type-layer and checker/binder reviews are PASS with no unresolved HIGH, MEDIUM, or
  LOW findings. The full gate passes **1,041 / 0 / 18** library tests plus every integration,
  conformance, and doctest target; clippy, format, and diff checks are clean. Authoritative double
  generation remains byte-identical to the 10,003,957-byte packaged snapshot (`af97017...`), and
  the diagnostic evidence digest remains `34cc5c...` without repinning.
- WU4 deliberately does not route collisions. WU5 must preflight before semantic mutation and
  replace the current whole-source private rebuild with an exact combined-universe mechanism whose
  collision and fanout rows meet the 2x gate.

### 2026-07-23 — WU5 private-route decision and RED contract

- The current source-backed full universe measures **2.695 s**: parse **11.7 ms**, bind **17.3 ms**,
  reserve/fill **267.5 ms**, publication **54.8 ms**, and statement semantics **2.344 s**. A
  collision-free frozen user check measures **60.6 ms**. The dominant removable work is repeated
  library semantics, not source loading or binding.
- Private routing cannot be treated as a rare direct-collision fallback. The B14 matrix is exactly
  **2 shared / 10 private**. A preliminary conservative census of the pinned 874-case official
  corpus finds 814 cases with script top-level value contributors and estimates roughly **815 / 874**
  private projects, while only about 16 directly collide with a frozen root. This is a planning
  proxy until the production OXC preflight emits the authoritative incidence.
- ADR `784ca25` accepts a fresh private canonical-snapshot seed plus full 82-file/project parse-bind
  and selective replay of the authenticated affected-owner reverse closure. ADR `07b2c0a` separates
  route-unaware semantic-prefix authentication from an exact continuable library-only binder/root
  checkpoint and requires append-only replacement semantic rows.
- RED commit `42b14fc` specifies mutation-free exhaustive preflight, value/type/namespace and
  cross-slot collision routing, unique global-object contributors, full-source semantic/event
  parity, exact merged slot identities, snapshot-index admission, a calibrated 31+6-family physical
  work ledger, independent owner/edge closure oracles, O(V+E) scheduler work, the locked production
  collision, and 32 genuinely independent contending private projects.
- Three independent RED reviews are PASS after closing self-reported/gameable work counters,
  incomplete module and cross-slot coverage, an invalid opposite-order identity witness, conflated
  binder/semantic prefixes, missing ledger parity, in-process RSS, and a one-project fanout loophole.
  The unchanged production gate passes **1,041 / 0 / 18** plus all integration/conformance/doctest
  targets; clippy, format, and diff checks are clean.

### 2026-07-23 — WU5 exhaustive collision preflight

- Follow-up RED commits `dc2214c` and `3124732` isolate authenticated replay-index admission and
  preserve the adversarial routing failures found during implementation review. Implementation
  commit `edec9fa` makes one exhaustive binder-owned source visitor serve two allocation-isolated
  projections: ordinary binding retains only lexical occurrences, while preflight retains only the
  global binding census. The canonical filename-to-source-kind classifier is shared by driver,
  compiler, and preflight.
- The preflight handles every current OXC statement, declaration, module declaration, binding
  leaf, global augmentation, UMD, and `globalThis` form before issuing its one-shot shared-delta
  capability. Parser recovery routes private and a parser panic rejects before semantics. The B14
  matrix remains exactly **2 shared / 10 private**; nested recoverable `global` and UMD placements
  cannot escape to the shared route, while external module-local namespaces stay shared.
- Review rejected the first implementation for a duplicated placement walker and fabricated work
  calibration, then rejected its first repair for production census allocations, non-exhaustive
  statement placement, and incompletely calibrated hooks. The final implementation measures actual
  delta forks, four layered insertion paths, primary and secondary event reservations, durable
  query writes, relation insert/promotion, and a real private library compilation. Two independent
  re-reviews are PASS with zero unresolved HIGH or MEDIUM findings.
- The leader gate passes **1,069 / 0 / 18** library tests plus every integration, conformance, and
  doctest target. Clippy, format, and diff checks are clean. This closes route selection only;
  authenticated replay-index admission, the continuable private binder, affected-closure replay,
  and the collision/fanout performance gates remain open WU5 work.
