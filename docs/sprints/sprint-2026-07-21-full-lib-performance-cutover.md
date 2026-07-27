# Sprint — full default-library performance cutover (2026-07-21)

**Goal.** Ship the exact pinned TypeScript 6.0.3 ES2025 full-host library as typokat's production
default type universe, with sound user checking and a fresh-process end-to-end wall time
**demonstrably faster than pinned native TypeScript 7** on the approved reference workload,
compiling that library from source in every process. (The goal read "at least 2×" until 2026-07-26,
when [`ADR-0017`](../decisions/0017-compile-the-default-library-from-source.md) retired the shipped
snapshot that made 2× reachable — see the Binding performance claim below.)

**Theme.** Backlog [`14`](../backlog/14-libdts-loading.md) is not complete merely because the 82
declaration files can finish in a test-only pipeline. The production CLI must actually use the
result, preserve every library-owned diagnostic/incomplete outcome, support the ADR-0011
base/delta and collision semantics, and win a fail-closed apples-to-apples benchmark. The previous
feasibility sprint removed two nonlinear barriers but left runtime library compilation at roughly
10.8 seconds against native TypeScript 7.0.2's roughly 0.3 seconds on the same bytes, which is why
the sprint opened by planning a shipped semantic snapshot. That plan shipped and was then retired:
the 10.8 seconds fell to 1.85, and attribution showed 62 % of what remained was artifact generation
with no comparator analogue, leaving the checking pipeline itself at parity. The library is now
compiled from source in every process (ADR-0017), and the remaining work is the production cutover
and the cross-tool gate.

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
> fresh-process TypeScript 6.0.3 ES2025-full workload demonstrably faster than the pinned native
> TypeScript 7 executable, while using the same 82 library source bytes, compiling its default
> library from source in every process, and preserving the approved semantic outcomes.

For each benchmark row and each of three independent trials:

```text
speedup = median(tsgo wall time) / median(typokat wall time)
```

The one-sided 95% bootstrap lower confidence bound of `speedup` must exceed `1.00`, and the ratio of
tsgo p95 to typokat p95 must also exceed `1.00`. The engineering target is `1.25×` to leave noise
headroom. The threshold is derived from the frozen comparator samples; no absolute time from the
exploratory runs is hard-coded. A failed row is NO-GO. The run log cannot weaken, drop, rename, or
replace a row after seeing its result.

**Restated 2026-07-26 from `≥2.00` to `>1.00`, by explicit decision, and the reason is on the
record.** The 2× target was written when a shipped semantic snapshot was the plan; its 112 ms median
cold start is what made 2× reachable.
[`ADR-0017`](../decisions/0017-compile-the-default-library-from-source.md) retires that snapshot
because precomputing one pinned profile does nothing for arbitrary user code, and source compilation
costs 277 ms against the comparator's 289 ms. The claim this sprint can honestly support is
therefore "faster, proven statistically", not "twice as fast". This is a **weakened** gate; it is
recorded as such rather than presented as equivalent. Every other control below — same library
bytes, no `--skipLibCheck`, fresh process per sample, identical binaries for semantics and timing,
no lazy subset or output suppression — is unchanged and still binding.

The primary benchmark is fresh-process/compiler-cold with an ordinary warm filesystem cache. It
includes process creation, production CLI startup, default-library source compilation, user-source
I/O, parse/bind/check, diagnostic construction where applicable, and normal shutdown. It excludes
downloading tools, building binaries, and staging the comparator runtime. Every measured sample is a new process; no daemon, incremental state, or
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

The claim applies to every row. If ADR-0011's private full rebuild cannot meet the collision row,
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
  allowed. No precomputed semantic artifact may be reintroduced to win a row; per ADR-0017 the
  library is compiled from source in every measured process.

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
  match tsc. The collision and fanout benchmark rows each satisfy the binding confidence/p95 gate,
  every run stays within 512 MiB, and all-colliding fanout is deterministic and bounded.
- **Stop/falsifier.** A collision row below the binding threshold means no claim and no sprint
  completion. Do not relabel the collision row out of scope or accept the unaugmented snapshot as
  success.
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
  fallback. **No library-owned record reaches CLI output on the cut-over binary.** This absorbs the
  residual of the deleted backlog `99`: [ADR-0018](../decisions/0018-pin-library-owned-records-as-a-named-census.md)
  proved containment through `check_project_with_library`, but
  `the_cli_prints_no_record_for_a_clean_file` is a weak witness while the CLI still runs
  `src/prelude.ts` — it must be re-read after the cutover, when it finally has 875 records to
  suppress.
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
  bound and p95 ratio above the threshold in the **Binding performance claim** — `>1.00` with a
  `1.25×` engineering target, restated there from `≥2.00` on 2026-07-26 before any result was seen;
  the `≥2.00` this bullet used to carry was left stale by that restatement. All semantic
  outputs/identities match their oracles; typokat RSS meets both memory gates. Commit raw canonical JSON, summary, binary/profile/host facts, commands,
  snapshot and package sizes, route incidence, and independent statistical validation.
- **Stop/falsifier.** A semantic difference, incomplete evidence, identity mismatch, forbidden
  optimization, missing row, or a row below the binding threshold is NO-GO. The target is not
  averaged across rows or traded against memory.
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

### 2026-07-23 — WU5 collision replay dependency index generation

- Commit `8b588ca` instruments the one authoritative source compilation with typed semantic
  producer/consumer ownership and emits a canonical dependency-to-consumer replay graph. The
  exact 82-file profile has **45,925 owners**, **2,238 root slots**, **47,253 owner sites**,
  **9,922 reverse edges**, **6,940 root edges**, **45,241 SCCs**, **42,496 statement owners**, and
  **45,925 baseline records**. Its 10,996,257-byte manifest is deterministic with SHA-256
  `cc125e22a561b069f62f6707e5eb3f8187be0959bb75d8cbfb665266d21c2c95`; all unowned-demand,
  invalid-site, noncanonical-edge, and typed-reference coverage counters are zero.
- The tracer covers type groups, values, namespaces, classes, global-object contributors, and
  statements without retaining AST state. Replay-aware lookup wrappers own semantic demands;
  an exhaustive recursively discovered source guard rejects unmanifested raw resolver/storage
  access, including cfg-test and nested-query bypasses. Exact root normalization uses independent
  binder evidence and a strict two-way census rather than existing checker storage.
- Full-suite review exposed class-application state/surface conflation and an over-filtered nested
  `global` census. The first now uses a state-only class requirement after one traced observation.
  The second separates exact replay candidates from conservative preflight-only candidates:
  legal external augmentations are exact, direct script augmentations are rejected, and nested or
  uncertain placements route private without contaminating snapshot roots. Independent re-reviews
  are PASS.
- The final leader gate passes **1,103 / 0 / 19** library tests plus all integration,
  conformance, and doctest targets; clippy, format, and diff checks are clean. This commit generates
  the index but deliberately leaves the snapshot at ten sections. Persisted section admission,
  the continuable private binder, closure replay, and collision/fanout performance gates remain
  open WU5 work.

### 2026-07-23 — WU5 authenticated replay admission and startup gate

- RED commit `beba550` and implementation commit `26709e4` persist the replay manifest as the
  authenticated eleventh snapshot section. The canonical artifact is now **21,000,266 bytes**,
  SHA-256 `539a52fdd66130c35172d2405032e442f52d161dfd2ebcae873a03151a7e2960`;
  section 11 remains 10,996,257 bytes with the independently pinned manifest SHA-256
  `cc125e22a561b069f62f6707e5eb3f8187be0959bb75d8cbfb665266d21c2c95`. Strict decoding,
  owner/root/site/edge/SCC/statement/baseline partitions, generation-health counters, and
  self-consistent-but-unpinned mutation families all fail closed before publication. Independent
  review is PASS after 60,000/40,000-case semantic fuzz predecessors and the full typed mutation
  matrix.
- The first clean release gate exposed a pre-existing reproducibility defect: four unit-test
  `env!("CARGO_MANIFEST_DIR")` literals embedded the two physical clone roots. Commit `0475f1f`
  replaces them with fail-closed runtime repository discovery and adds a recursive source guard.
  Two isolated release libtests are byte-identical again.
- The first complete 45-process timing run at `0475f1f`
  (`run-20260723T053958Z-3306213.json`) was a real NO-GO. Every sample exceeded 120 ms; window
  medians were **127.563 / 126.784 / 128.414 ms**, window nearest-rank p95s were
  **129.771 / 132.400 / 135.822 ms**, and overall p95 was **132.400 ms**. Median internal phases
  were 15.601 ms artifact validation, 87.212 ms decode/admission, and 2.290 ms publication.
  Relative to the pre-replay WU3 gate, internal work explained 96.9% of the 31.184 ms mean wall
  regression; launcher noise and build selection did not.
- Heap attribution found 45,241 SCC-owner allocations, a redundant replay-section SHA, and three
  complete const-promoted copies of the 21 MB snapshot in the release libtest. RED commits
  `d464629` and `a4187c4`, followed by reviewed implementations `87f581b` and `28e464e`, retain
  singleton SCC members inline, reuse the directory-authenticated section digest, and give the
  packaged snapshot one immutable static definition. The release executable now contains exactly
  one byte-for-byte payload; the local layout witness shrank its libtest from 107,023,568 bytes
  with four test-induced copies to 44,022,944 bytes with one. These fixes preserved the wire and
  semantic identities but alone still measured 125.241 ms median / 128.423 ms p95.
- RED `6d1ce00` then pinned an opaque, consuming admission witness around the exact verified
  `Cow<'static, [u8]>`. Implementation `25081b7` removes only the second whole-artifact SHA in the
  canonical decoder; package identity failures retain `ArtifactAdmission`, structural parsing
  retains its typed stages, and the generic adversarial decoder still validates body and every
  section digest. Independent review found zero unresolved HIGH or MEDIUM issues.
- **Replay-admission GO:** authoritative clean-tree run
  `run-20260723T062645Z-3691945.json` at `25081b7` produced two byte-identical 44,179,400-byte
  release libtests, SHA-256
  `dc4397d2c19873fb9bf8e5c2bd646402e3c46cae3cf3d5f85eecbceb0d3962b5`, then completed three
  windows of five warmups plus ten recorded fresh processes. Window medians were
  **111.674 / 112.242 / 111.749 ms**; p95s were
  **114.308 / 118.921 / 119.317 ms**; overall median/p95 were
  **111.991 / 118.921 ms**. Median validation/decode/publication were
  **15.744 / 71.112 / 2.299 ms**, and maximum external RSS was **88,322,048 bytes**. The unchanged
  120 ms / 512 MiB gate passes with one typed validation identity, one initialization, one
  publication, and zero source/compiler/generator activity.
- This GO closes persisted replay admission, not WU5. The remaining critical path is the exact
  continuable library-only binder checkpoint, affected-owner reverse closure, append-only semantic
  replay, and the collision/fanout 2x gates. The final window retains only 0.683 ms p95 headroom, so
  later WU5/WU8 work must preserve or improve startup rather than treating this checkpoint as
  surplus budget.

### 2026-07-24 — WU8 transactional declaration planning and replay closure bounds

- Leader baseline at `933bfd5` in a pristine worktree: **1,217 passed / 11 failed / 19 ignored**.
  Seven of those failures were the RED specs of this batch (`9d0acb6`, `586133d`, plus
  `user_delta_project_scale_spec::per_check_allocation_and_traversal_do_not_scale_with_frozen_base_size`);
  the other four are the stale packaged snapshot, below.
- Commit `9d47612` splits the declaration-surface planner into a plan phase over `&self` and a
  commit phase that interns only on success. The 82-file profile's **4,072** zero-argument
  interface-property roots now commit **zero** recipe rows and zero durable edges
  (`RecipeArenaMeasure { durable_rows: 0, durable_edges: 0, reachable_rows: 0, reachable_edges: 0 }`,
  `eager_materialization_roots = 0`); the 8-pad and 512-pad deferred shapes are identical at
  `{ durable_rows: 6, durable_edges: 4 }`. The source-compiled archive drops **19,039,216 →
  19,025,866 bytes**, SHA-256 `590bf4727d61a0638b7a889ece9e0705182e8efcf580b1d61ef73d0131f21646`,
  reproducible across two runs; its pin moves with it.
- Two semantic corrections rode along and are the reason this is not a pure perf commit. Interface
  arity now reads `recovery_params`, which **recovers a dropped `TK2314`** on a merged generic
  interface reached through a lazy property (matches `tsc` byte for byte). A published template is
  taken lazily only when `TypeTag::Object`, so the frozen prelude's `OmitThisParameter<T>` marker
  keeps its specialized argument.
- Independent adversarial review of the planner returned FAIL on three findings, all applied before
  the commit: hand-rolled `free_params` diverged from the canonical derivation on `Application` and
  the zero-argument arms and was guarded only by `debug_assert_eq!` — in release an under-reported
  vector silently drops the specialization mapper in `intern_declared` (the derivation is memoized
  and now runs once per committed row, measured neutral, and `src/types/intern/declared.rs` returned
  to byte-identical with HEAD); the commit-time dependency check was provably dead code, since
  `Binder::resolve_type` *is* `resolve_type_traced(…, || {})`; and the `RawAccessAllowance` reason
  claimed authentication where only replay happens. The review reported the diff byte-identical to
  HEAD over **462 fixtures and 874 official-corpus files**, `tsc --strict` parity on 11 probes, and
  could not break the `TypeTag::Object` gate with injected conditional/mapped/keyof zero-parameter
  aliases or Object-tagged generic templates at root, array, tuple and `readonly` positions.
- Independent adversarial review of the `dfa62b4` replay closure **passed it on soundness** —
  complete and exactly equal to the pre-`dfa62b4` DFS over 520,000 fuzz graphs, deterministic
  cross-process (the one hash container is never iterated) — and disproved the suspected duplicate
  `ClassId`-across-components pathology, which is linear. It instead found that memoizing only at
  query roots re-walks a shared pass-through spine once per root, i.e. a regression against the code
  `dfa62b4` replaced, invisible to every committed counter.
- RED `0d79bc8` adds an `owner_expression_visits` counter over the actual per-expression traversal
  and replaces the wall-clock probe with a deterministic guard. Implementation `bc59c9f` flattens
  the expression vector in one ascending pass (it is already topologically ordered; the precondition
  is now asserted): **131,840 → 2,305** visits at 256×256 and **2,100,224 → 9,217** at 1024×1024, so
  growth tracks the 4× input growth exactly (3.999×) at ~8× under budget, with the completeness pin
  unchanged. It also promotes the Kahn drain guard to a real `assert_eq!` — an undrained
  condensation would admit unauthenticated replay dependencies, and the debug-only guard vanished in
  release.
- The ascending pass is a trade, not a universal win: full memoization is quadratic on an
  accumulating chain (33,406 visits at depth 256, 526,846 at 1,024) where root-only memoization was
  linear. Per-class owner closure is transitive-closure-hard, so only the set representation can
  bound both shapes; filed as backlog `85`. The review also surfaced a pre-existing reachable
  panic — a function-local `interface` aborts the check worker — filed as backlog `84`.
  **Correction (2026-07-25):** this entry first recorded "the real profile builds 44 owner
  expressions, so neither bound is live". That was wrong. Every test compiling the 82-file profile
  is `#[ignore]`d, so the suite that figure came from never ran the real profile; it builds **7,073**
  expressions and materializes **2.24 M** owner entries to answer **3** queries. Backlog `85` is a
  live 22% hotspot, not scale-ladder work — see the profile entry below.
- Final leader gate on the clean tree at `9d47612`: **1,226 passed / 4 failed / 19 ignored**, clippy
  `--all-targets -D warnings` clean, and every integration target green (conformance 14, divergences,
  manifest, surface, incomplete-outcome, class-id-exhaustion). The four remaining failures are the
  packaged `canonical.snapshot`, last regenerated at `90ff28d` and therefore stale since before this
  batch. **Regeneration and the pin family in `src/library/artifact.rs` remain open**, and must run
  from a clean committed tree via `tooling/library-package/verify.py`.

### 2026-07-25 — cold source-compile attribution and the checker/generator split

Two independent measurement passes at `c844680` (span instrumentation in one scratch worktree,
`pprof` at 499 Hz over 40 cold release processes in another; 23,655 samples, 0% stack truncation,
profiler overhead measured at zero by interleaved A/B). Neither host window was a clean room —
load 2.1–9.6, a `node` at ~51% and two Chrome processes at ~25% throughout — so treat absolute
totals as ±20% and the *shares* as the result. The two passes agree on every share.

- **Cold source compilation is 1.85 s, and 83.5% of it is not type checking.** The
  `library_release_probe_once` phase line accounts for only 757 ms of it; the missing **1,092 ms
  (59%)** is one uninstrumented function, `build_collision_replay_index`. Within it,
  `require_terminal_class_dependency_closure` is 456 ms and `canonical_record_bytes` 412 ms.
  Separately, `statement_check_us=556381` is **96% not statement checking** — the span swallows
  `canonical_library_evidence` (411 ms) and `finish_semantic_effects` (96 ms); real
  `check_statements` is **19.8 ms**.
- **The comparator line, measured in the same window.** Splitting the run into the checking
  pipeline (parse + bind + reserve/fill + publication + statement-check-proper) versus artifact
  generation gives **277 ms** of checking against **1,152 ms** of evidence/replay-index generation,
  which has no comparator analogue. Pinned native TypeScript 7.0.2 on this host — the
  `contract.toml` binary, digest-verified, 82 profile libs plus the sentinel, `--listFilesOnly`
  printing exactly 83 files — measured **289 ms** (median of 15, 281–306 ms). So typokat's cold
  parse+bind+check of the pinned library is **~0.96× tsgo, i.e. at parity**; the 5.1× gap on the
  probe's total is entirely artifact generation. Probe scaffolding a production run would not do
  (assertions, `check_source(TINY_SOURCE)`, test-only probes) is 9,133 µs — **0.62%** — so the run
  is 99.4% real work.
- **Two accidental quadratics, both in generation.** `LineIndex::new` is 44.46% / 822,430 µs of the
  run: `canonical_record_bytes` renders one diagnostic per call via `std::slice::from_ref`, and
  `render_compact_to_writer` rebuilds the line index per call — **1.21 GB scanned for a 2.94 MB
  library, 413×**, over 530 renders, and the identical bytes are computed **twice** (once in
  `canonical_library_evidence`, once in the replay index). The ordinary CLI diagnostic path passes
  all diagnostics in one call and is unaffected. Second, `terminal_expression_owners` materializes
  2.24 M owner entries and 6.50 M element copies to answer **3** class queries (backlog `85`).
- **Measured ablations**, 7 interleaved rounds each, not projections: memoizing the `LineIndex` per
  file **−900 ms** (semantics-preserving, identical `outcome-v1` line); skipping the owner closure
  **−439 ms**; both **−1,337 ms → 525 ms, 3.54×**. The residual 525 ms is
  `ReplayDependencyTrace::finish` 164 ms (of which `dependency_first_sccs` 100 ms),
  `finish_semantic_effects` 83 ms, `construct_pending_interface_sccs` 65 ms, `load_strict_profile`
  26 ms, publication 22 ms, bind 19 ms, `check_statements` 16 ms, parse 14 ms.
- **The run is single-threaded** — `cpu_us/wall_us = 0.994`, one core of 16. `rayon` is used in
  exactly one place, `src/driver.rs:143` (`check_files`), which this path never reaches. The only
  embarrassingly parallel phases here are parse (14 ms) and per-file bind (20 ms), so the parallel
  ceiling on this workload is **≤30 ms of 1.85 s**. Parallelism is not the lever for the library
  profile; it may still be for multi-file user projects.
- **Instrumentation debt.** The 07-21 phase split had an 8 µs residual because none of this code
  existed; `build_collision_replay_index` arrived in `8b588ca` and `canonical_library_evidence` in
  `d533848`, both untimed. `replay_index_us`, `evidence_us`, and `terminal_closure_us` should become
  first-class phases, `evidence` should leave the `statement_check` span, and the probe should
  assert its own residual stays under a few percent so the next unmeasured phase fails the probe
  instead of silently reopening a 59% hole.

### 2026-07-26 — the shipped snapshot is retired; the library compiles from source

The project owner directed that the snapshot be removed: it is an optimization hack, and the goal is
to beat native TypeScript **without** a precomputed artifact, because precomputing one pinned profile
does nothing for arbitrary user code. Recorded as
[`ADR-0017`](../decisions/0017-compile-the-default-library-from-source.md), which supersedes
`ADR-0012` and `ADR-0015` wholesale and narrowly supersedes `ADR-0013`'s decode-seeding clause and
`ADR-0014`'s digest-authentication clauses.

- **The fork decision above rested on a measurement that has since been corrected.** "Real fork"
  rejected option 2 (keep optimizing runtime compilation) on the ground that "roughly 9.83 seconds
  sits in statement-check/evidence versus a likely ≤0.15-second target". The 07-25 attribution
  showed that span was 96% not statement checking; real `check_statements` is 19.8 ms, the checking
  pipeline is 277 ms against the comparator's 289 ms, and the rest was generation. The decision was
  correct on 07-21 evidence and is not correct on today's.
- Two preparatory commits. `4483560` moved the five binder-level items `library_compiler.rs` reached
  into the codec for — `RootNameRow`, `collect_root_rows`, `encode_root_index`, and the source
  checkpoint digests — out of it, and retargeted the test-only `load_strict_profile` sites to
  `ExactLibraryProfile`; that made the codec a leaf with no non-snapshot consumers, at
  1255 passed / 5 failed / 19 ignored, unchanged.
- `0497550` then deleted it: **60 files, −18,574 / +793**, including the 21,003,926-byte
  `canonical.snapshot`, `library_snapshot_codec/` (8,617 lines), `artifact.rs`, `snapshot.rs`,
  `tooling/full-lib-snapshot/`, `tooling/library-base/`, and the `library-package` CI job. All 82
  `.d.ts` sources, `profile.toml` and the TypeScript notices are retained — they are the input.
  Release binary **27 MB → 6.8 MB**; lib suite **242 s → 53 s**; **1269 passed / 1 failed /
  14 ignored**, the one failure being the pre-existing RED guard for unimplemented backlog `95`.
  Four permanently-failing stale-artifact tests went with the artifact.
- **`FrozenLibraryBase` is unchanged as a type**; only its provenance moved, via a new fallible
  `publish(CompiledLibraryBase)` fed by the compiler's frozen runtime product. `LibraryBaseProvider`
  keeps its `OnceLock` failure-caching and loses the decode stages from its error taxonomy.
- **The checkpoint authenticator is deleted, not made vacuous.** It compared a source-compiled
  binder against digests taken from the decoded archive; with one producer left it would compare a
  value against itself. `LibraryBinderCheckpoint` keeps private fields, no `Clone`, and a single
  construction site, so provenance is guaranteed structurally. `RuntimeCollisionReplayIndex`
  collapsed as well: admission is now the single construction path and re-checks the generator's
  structural guarantees on every compile, packaged or focused — strictly more coverage than before.
- **Two findings filed rather than absorbed.** Re-pinning the library evidence to source truth
  exposed 273 → 265 diagnostics with bytes rising 91,453 → 125,251 and incompletes unchanged; the
  pins were 102 commits stale and the delta predates the removal. `243a878` was ruled out by direct
  measurement (its parent `ddfd649` already gives 265) → backlog
  [`98`](../backlog/98-library-diagnostic-count-delta.md). Roughly 6,300 lines of orphaned
  byte-level codec are gated `#[cfg(test)]` rather than deleted, because part of it is used as a
  traversal backing live reference-integrity assertions → backlog
  [`97`](../backlog/97-orphaned-wire-serialization.md).
- **The binding performance claim above is now unreachable as written and must be restated.** The
  snapshot's 112 ms median / 119 ms p95 cold start is what bought ≥2×. Source compilation is at
  277 ms against 289 ms, i.e. ~1.04×. Restating the gate is a contract change and is pending an
  explicit decision; it must not be silently missed. `tooling/full-lib-bench/` itself is not
  materially snapshot-coupled — four lines of `provider_probe` schema (`snapshot_schema`,
  `snapshot_product_sha256`) go when WU7 builds the `library-info` subcommand, which does not exist.
- Remaining critical path is unchanged in shape: WU6 corpus, WU7 production provider + CLI cutover +
  deletion of `src/prelude.ts`, WU8 the gate, WU9 review. The 1,152 ms of generation the 07-25 entry
  attributed is still on the from-source path — deleting the codec did not delete it, because
  `canonical_library_evidence` and `build_collision_replay_index` live in
  `compile_owned_injected_frontend`, not in the codec. That cut is the next work unit.

### 2026-07-26 — artifact generation leaves the cold path

`a0977ea`. Publishing the base was **1443 ms** at `4948095` and is **296.2 ms** (median of 7 cold
release processes; 288.3–300.7). The record dump is byte-identical across the change — 875 records,
265 diagnostics and 610 incompletes, md5 `476244c54621b73fe6655ed391caded7` — verified by diffing
the dumped records, not by comparing a count or a digest, because a count and a digest are exactly
what hid backlog `98`.

Phase attribution before → after (in-place probes, medians of 3, since removed):

| | before | after |
|---|---|---|
| parse / bind | 11.7 / 11.0 | 11.0 / 10.7 |
| reserve_fill | 100 | 99.7 |
| publication | 44 | 43.2 |
| `check_statements` | 15 | 14.1 |
| finish_effects + ledger | 86 | 84.3 |
| `canonical_library_evidence` | 267 | — |
| `build_collision_replay_index` | 900 | — |
| … `canonical_record_bytes` | 266 | — |
| … `validate_terminal_class_deps` | 477 | **16** (validation kept, closure deferred) |
| … `trace.finish` | 132 | — |
| `admit_replay_index` | 18 | — |

- **Two premises in the brief were wrong, and both mattered.** `LibraryCompiler::compile` has no
  production caller in-repo — all four are `#[cfg(test)]` — so `CompiledLibrary`, `LibraryEvidence`
  and `LibrarySemanticIdentity::evidence` were already suite-facing, not just the evidence blobs.
  And the manifest digest was not *a* production consumer of the replay index but the **only** one;
  `schedule_sparse_collision_closure` is reserved for the ADR-0015 collision route, and ADR-0015 is
  superseded wholesale. There is no production collision route yet.
- **Lazy assembly from a retained trace is impossible**, not merely unattractive: assembly needs the
  oxc ASTs (`source_global_binding_census_with_provenance`) and the source text
  (`canonical_record_bytes`), both bound to the frontend arena, and ADR-0011 requires an AST-free
  base. Deferral can therefore only mean "the colliding run re-compiles from source and assembles
  then" — which is what ADR-0013 as narrowly superseded already prescribes. Stated as
  `ReplayIndexPlan::{Assemble, Deferred}` at the entry point rather than hidden in a lazy field.
- **Leader intervention on the first round.** It deferred `validate_terminal_class_dependencies`
  wholesale. Its four checks read as replay validations but test **production** state — semantic
  identities against exact recomputation, every terminal `Ready`, runtime class names having
  published owners, named functions having value owners. "The suite still covers it" does not hold
  once the base path and the assembling path are different code, and that is the reasoning that let
  `98` sit for 102 commits. Split to `Option<&ReplayDependencyTrace>`, one implementation and two
  callers: **16 ms** on the base path, the 463 ms closure deferred. The measured cost of restoring
  it was 15.3 / 16.2 / 16.1 ms.
- The dropped `FrozenBaseWitnessForTest.replay_manifest_sha256` was a **tautology**: stored at
  compile time, never recomputed, therefore constant across any before/after comparison. The witness
  detects a leaked delta row through `type_count` (16,926, exactly the fork frontier) and three live
  reference traversals (104,038 store / 16,943 interner / 85,806 binder).
- **Filed, not absorbed:** the frozen base retains **no** library records — `ledger.finish()`
  materializes all 875 inside the frontend and production drops them on return; `OwnedLibraryRuntimeState`
  has no record store. True before this change as well; the evidence blobs were a byte projection,
  not retention. So ADR-0011's "preserve every library-owned diagnostic outcome exactly" holds today
  only in the sense that they are computed exactly. WU7 needs somewhere to put them →
  [`99`](../backlog/99-library-records-are-not-retained.md).
- **Where the remaining 296 ms sits, and why it matters for the gate.** `reserve_fill` (99.7 ms) and
  `finish_effects + ledger` (84.3 ms) are 184 of the 296. The ledger half materializes records
  production then discards, so `99` and the gate are the same work. Exploratory native TypeScript
  numbers on this host put the pinned 82-file graph at 0.32–0.33 s, so a cutover CLI at ~296 ms plus
  a user check lands near parity, not comfortably above the 1.25× engineering target. Those two
  phases are where the margin has to come from; parse and bind together are 22 ms and cannot supply
  it.

### 2026-07-26 — the loader gets an instrument; five defect families named

A throwaway spike wired `check_source`/`check_project` to a published base in a scratch worktree and
measured what a cutover actually costs. `2a85492` then landed the behaviour-neutral half of it.
Production still runs `src/prelude.ts`.

- **Nobody had ever run this code.** Seven `#[cfg(test)]` gates stood between non-test code and a
  published `FrozenLibraryBase` — `resume_frozen_library`, `finish_frozen_library_continuation`,
  `frozen_global_augmentation_count`, `NamespaceTable::global_augmentation_count`,
  `check_project_programs_with_owned_library`, `into_user_project_base`, and the collision-preflight
  capability. That is why the defects below survived the whole sprint: the instrument that finds
  them did not exist. **This is the single most under-estimated item in WU7.**
- **The blast radius is small and enumerable.** Of 410 fixtures, **381 are unchanged (93 %)**;
  19 differ (38 diagnostics lost, 37 gained) and 8 crash. The predicted class — fixtures losing a
  `TK2304` because a global now resolves — is **empty**; nine `TK2304` were *gained* instead, because
  the base publishes no `globalThis`, no cross-file script globals and no UMD globals.
- **b14 enabled: 7 of 13 flat green, 1 of 12 project green.** Those eight are now on. The rest reduce
  to **five named defect families**, which is the work list for the loader:
  1. **`declare global` continuation and collision merge *panic*** — `bind.rs:2179` (4 projects) and
     `mod.rs:1267` (1). Everything else is a wrong answer; this is no answer. Highest priority.
  2. **native↔library identity** — `Array<T>`, `ReadonlyArray<T>`, `String` as *annotations* lower to
     the library interface's structural expansion instead of the intrinsic type. Member access is
     bridged (`library_identities.rs:286`); annotation lowering is not. A 15-surface probe shows
     `Promise`, `Math`, `JSON`, `Date`, `Map`, `Set`, `RegExp`, `keyof string` all work.
  3. **`globalThis`, cross-file script globals, UMD globals** are not published from the base.
  4. **function-shaped constraint satisfaction** — an object type with a call signature fails
     `(...args: any) => any`. Same family as the census's 64 `TK2344`.
  5. **`intrinsic` string types** — `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` stop
     evaluating, so their assignment errors vanish. Six `intrinsic-keyword` incompletes, and
     `m28_utility_types` loses six `TK2322`.
- **Loading the library makes ordinary checks *faster*.** Warm per-fixture check **0.31 ms** against
  the prelude's **0.47 ms**, because the prelude re-parses, binds and checks its 47 lines on every
  single check while the library path forks an already-checked base. The 291 ms base is paid once per
  process. Conformance 3.9 s → 7.1 s in debug, entirely the one-time publication.
- **Two hazards not previously recorded.** The collision preflight parses and walks user source on
  the **caller's 8 MB stack**, outside the `CHECK_STACK_SIZE` worker (`driver.rs:68`) that exists for
  exactly that reason — it overflowed on a 471-file census. And a route census over the corpus gives
  **shared 285 / private 185 / rejected 1**: script-mode `var`/`function` are global-object
  contributors and route private, so **39 % of the corpus needs WU5's private-combined path**. That
  puts WU5 on the critical path for the suite, not in the tail.
- **WU7's "initialize the provider before rayon" is vacuous.** `check_files` (`driver.rs:141`) is the
  crate's only rayon site and has **no production caller** — `main.rs:148` uses `check_project`, and
  grep finds `check_files` only in its own two unit tests. The checker is single-threaded in
  production today; architecture §8.2 Stage 1 is not wired to the CLI.
- **The 265 library-owned diagnostics cannot reach user output**, before or after a cutover: each
  user check builds a fresh `EventStore` reserving only user programs (`mod.rs:1195-1207`), confirmed
  empirically by 381/410 byte-identical fixtures and a zero-incomplete probe. Backlog
  [`99`](../backlog/99-library-records-are-not-retained.md)'s risk is therefore the **inverse** of
  what WU7 states — not leakage, but that the set is invisible and unmeasurable, exactly as
  [`98`](../backlog/98-library-diagnostic-count-delta.md) predicted.
- **Sequencing decided, against the leader's first instinct.** The leader proposed closing the model
  gaps before wiring anything, on the theory that a library missing types in ~600 places is worse
  than the prelude. The evidence says otherwise: the incompletes never reach a user, and several
  families the census flagged (`symbol`, `bigint`, type predicates) are *expected* by
  `unsupported_surfaces.ts`, which exists to prove typokat stays non-permissive about them. But the
  reverse order fails too — the panics and the 39 % private incidence are loader defects no model
  work touches. **Wire early, cut over late**: every one of the five families was found in one
  afternoon *because* the driver was wired, and closing them blind would be guesswork.
- Harness trap, pre-existing, cost a debug cycle: `parse_markers_catch` / `parse_incomplete_catch`
  (`tests/conformance.rs:1180`, `:1294`) install a **process-global** silent panic hook while the
  rest of the binary runs concurrently, so a conformance failure printed `FAILED` with no report.
  The durable fix is for the marker parsers to return `Result` instead of panicking.

### 2026-07-27 — first production-shaped measurement, and two planning facts it changes

The CLI was pointed at the library base in a throwaway worktree (one line: `check_project` →
`check_project_with_library`) and benchmarked against the pinned comparator. This is the first
number for the shape WU8 will eventually gate on, rather than for an in-process harness.

- **Starting point 0.97×, not "already under tsgo".** fast-clean read typokat 297.8 ms against tsgo
  289.6 ms. Earlier in-process figures (277 ms vs 289 ms) were not end-to-end and should not be
  cited. The whole cost is the library build: 280 ms of library plus ~19 ms of user file, against a
  297.8 ms wall clock — the accounting closes with nothing hidden.
- **After four commits it is 1.12–1.14×** (260 ms), across two independent trials. The movement is
  `090ec7e` (publication clone, −38.5 ms of phase) and `0ba0a1b` (heritage composition, −6.6 ms);
  `27ad034` and `3711b00` are their work-counter guards.
- **typokat is single-threaded and tsgo is not — measured, not assumed.** typokat runs at 99–100 %
  CPU and burns 0.25 s of CPU time; tsgo runs at 164–191 % and burns 0.50–0.56 s. So typokat does
  the same job for **~2.2× less CPU** on one core, and wins wall-clock anyway. RSS 69.8 MB against
  tsgo's 98.4 MB — ratio 0.71 against the contract's 1.25 ceiling.
- **The 2.0× gate is therefore not reachable single-threaded.** The measured best case for the whole
  single-threaded stack — every optimization identified, including ones not yet built — is ~181 ms,
  i.e. ~1.6×. Closing to 2× requires parallelism, and parallelism is blocked on type identity, not on
  effort: `TypeId(self.tag.len())` is insertion-ordered (`types/store.rs:690`) and unions canonicalize
  by sorting raw `TypeId` (`types/intern/operators.rs`), so any reordering changes rendered
  diagnostic text. `StableHash` (`types/hash.rs`) "reserves the future cross-run content hash slot
  and deliberately returns a zero digest today". That is backlog `16`, blocked on `14` and `15`.
  **Correction to this entry as first written:** it said WU8's 2× target needed re-scoping. The
  binding gate had *already* been restated to `>1.00` on 2026-07-26, before any result was seen —
  what was stale was the `≥2.00` text still sitting in WU5's and WU8's own bullets, now pointed at
  the binding claim. So ~1.2× **passes** the gate and sits just under its `1.25×` engineering
  target; it is 2× that is out of reach single-threaded, and 2× is no longer what is being claimed.
- **Two of four benchmark rows still cannot run.** `collision` and `fanout` exit 101. Filed as
  backlog [`103`](../backlog/103-library-merge-panics-and-routing.md), which now owns WU5's ground:
  five panic sites, all one cause — an in-place merge into the frozen prefix.
- **The quiet half of that boundary was the larger defect** — backlog
  [`102`](../backlog/102-frozen-prefix-writes-vanish-silently.md), shipped in `74a6da3`/`6161527`.
  Five sites wrote with `if let Some(row) = table.get_mut(id)` and dropped the user's declaration
  when the row was in the frozen prefix. An ordinary cross-file `globals.d.ts` reported `TK2304`.
  It fires on *fresh* names, so no collision classifier would ever have caught it.
- **Profiler attributions were wrong twice, in both directions, and the implementers caught both.**
  The defensive publication clone was under-attributed (25 ms claimed, 38.5 ms measured — a sampling
  profiler under-attributes `free()`); the interface-member clones were over-attributed by ~10×
  (20–35 ms claimed, 3.3 ms measured), with the real cost being 7.9M quadratic name comparisons in
  the same function. Treat pprof output on this workload as a pointer to a *function*, not to a
  *reason*.
- **Two benchmark traps, recorded so nobody re-measures into them.** A warm in-process loop reads
  ~250 ms where a cold process reads ~316 ms, so loop-based figures understate production by ~65 ms.
  And ~9.5 ms of `#[cfg(test)]` probes sit inside the timed phase windows; production never runs
  them, so removing them improves the number and not the product.

### 2026-07-27 (later) — every benchmark row runs; the guard tier closes the panics

- **`collision` and `fanout` execute for the first time**, exit 101 → exit 3. Backlog `103`'s guard
  tier (`f1c7d7e`/`b223817`) converted all five frozen-prefix `.expect` sites into recorded
  refusals, reusing the ledger backlog `102` built rather than adding a second mechanism.
  `declare global` refuses the whole run with exit 2 and no partial output.
- **The guard's honest cost, recorded because it is the kind of thing that gets forgotten.** A
  refused type slot leaves the annotation as an error type, so `interface console` now passes every
  member read unchecked — a refusal that manufactures the same silent channel backlogs `45` and
  `101` turned out to be. And a `declare global` project produces no diagnostics at all. Both are
  ledgered `dir=under` under `103`'s correctness tier and both disappear only when the merge works.
- Three silent-diagnostic families closed the same day (`45` operators, `100` composed conditions,
  `101` ternary/logical values), plus `102`'s vanishing writes and `104`'s excess-property descent.
  Each was found by *using* the checker, not by reading it — which is `96`'s whole argument.
- **Perf, cumulative**: 0.97× → ~1.2× on `fast-clean`, from `090ec7e` (publication clone),
  `0ba0a1b` (heritage composition) and `2582684` (jemalloc). Measured compiled-in, jemalloc beat
  mimalloc by 20 ms and 12 MB of RSS — the opposite of what the profiling pass predicted, because
  the clone fix had already removed the allocations mimalloc was winning on.
- **Sprint bookkeeping fixed.** The binding claim was restated `≥2.00` → `>1.00` on 2026-07-26, but
  WU5's and WU8's own bullets still carried `≥2.00`; they now point at the claim. At ~1.2× the gate
  is met and the `1.25×` engineering target is close. **What blocks closure is not performance —
  it is `103`'s correctness tier**, without which WU7 cannot cut the CLI over.
