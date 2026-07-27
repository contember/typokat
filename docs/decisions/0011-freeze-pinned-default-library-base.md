---
id: 0011
title: Freeze the pinned default library as one shared semantic base
status: accepted; "preserved exactly" narrowed by 0018
date: 2026-07-16
---

# 0011 — Freeze the pinned default library as one shared semantic base

## Context

Backlog [`14`](../backlog/14-libdts-loading.md) replaces the bounded `src/prelude.ts` with
typokat's sole 1.0 standard-library profile and delivers parallelism Stage 1. The authoritative
product oracle is `tsc` 6.0.3 with `--strict --target es2025`; typokat 1.0 does not select a
different target, `--lib` set, host profile, or installed TypeScript. The profile is TypeScript
6.0.3's recursive `/// <reference lib>` closure rooted at
`lib.es2025.full.d.ts`: 82 files, 58,349 LF line terminators, and 2,936,611 source bytes. It is the
fixed ES2025 + full-host profile (ES libraries, DOM, iterable DOM, `webworker.importscripts`,
`scripthost`, and decorators), not a target-derived or host-selected default.

The current checker reparses, binds, and checks `src/prelude.ts` for every invocation. Its handoff
is already AST-free, and the published type environment already supports an inherited immutable
epoch. The `Interner`, `Store`, `Binder`, and their identity tables are still single mutable
vectors. Rebuilding the 2.9 MB profile in every per-file worker would multiply work and memory;
sharing a growing interner would violate the architecture's thread and identity boundaries.

Global declaration merging prevents a naive immutable-parent solution. A user script or
`declare global` may touch a library name and must then be checked in one merged universe. Mutating
the shared base would leak between projects; shadowing would lose library fragments; and using the
unaugmented base would be a false-clean prefix. Conversely, implementing a persistent dependency
recipe graph and project publication overlay now would create a second publication authority and
prematurely pull parallelism Stage 2 into this sprint.

The oxc AST is neither `Send` nor `Sync`. Library ASTs may be built on one owning thread, but only
fully owned semantic products may cross into process-wide state. The selected sources must also be
embedded: runtime npm, network, global-`tsc`, resolver, or filesystem discovery would make the
checker profile host-dependent.

The pinned ES5 readiness result proves that backlog 14 may start; it does not prove the full closure
can finish. Release feasibility probes at this decision point found:

- `lib.es5.d.ts`: 0.01 s / 9,092 KiB maximum RSS, with the expected four diagnostics and 187
  incomplete records;
- `lib.dom.d.ts` alone: timeout after 30.03 s / 40,068 KiB, with no output;
- the stripped-reference 82-file same-universe concatenation: timeout after 120.10 s /
  46,348 KiB, with no output; and
- 82-path project mode: timeout after 60.05 s / 47,452 KiB, with no output.

The concatenation and current project-mode probes are performance stress inputs, not semantic
oracles: concatenation makes all files external once it encounters `export {}`, while the current
project binder isolates script globals. On the same host, `tsc` 6.0.3
`--strict --target es2025 --noEmit` checked one clean source against the same 82-file default graph
in 1.19 s wall time and 313,680 KiB maximum RSS. Its extended diagnostics reported 0.16 s parse,
0.07 s bind, 0.84 s check, and 1.09 s total. The benchmark host was Linux x86_64 on an AMD Ryzen 7
PRO 8840HS (8 cores/16 threads) with 59 GiB RAM; release typokat used Rust 1.95.0.

Therefore the [`full-lib-loading sprint`](../archive/sprint-2026-07-16-full-lib-loading.md)
starts with a hard audit/profile gate and has a separate measured performance gate before cutover.
This is a Tier-2 decision: the in-memory boundary is fixed here, while cutover remains reversible.

## Decision

### Embed one exact TypeScript profile

Typokat will vendor TypeScript 6.0.3 at commit
`050880ce59e30b356b686bd3144efe24f875ebc8`, rooted at `lib.es2025.full.d.ts`. The registry records
the exact ordered closure and, for every file, its npm package path, path at the pinned Git commit,
upstream Git blob identity, raw byte length, LF count, final-newline state, reference edges, and
SHA-256 digest. These fingerprints are binding:

- root-file SHA-256:
  `e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab`;
- SHA-256 of the 82 raw bodies concatenated in registry order:
  `0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267`;
- SHA-256 of the length-framed registry stream:
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`.

For the framed digest, each file contributes
`u64be(name_len) || u64be(source_len) || name_utf8 || source_bytes`. Registry order matches
TypeScript's `getDefaultLibFilePriority`: first occurrence in `libEntries`/`libs`, with the closure
root—which is not a `libEntries` name—last. Unknown, missing, extra, duplicated, reordered, or
byte-drifted inputs fail closed.

All 82 inputs end in LF, contain zero CR bytes, and together contain exactly 58,349 LF bytes.
Repository `.gitattributes` marks the vendored asset subtree `-text`, so checkout conversion cannot
rewrite authoritative bytes. The generated registry test byte-compares every checked-in input with
its manifest record before checking the aggregate counts and three fingerprints; a parsed-text or
line-normalized comparison is insufficient.

The vendored files retain their Apache headers. The exact TypeScript `LICENSE.txt` (9,197 bytes,
SHA-256 `a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47`) and
`ThirdPartyNoticeText.txt` (37,824 bytes, SHA-256
`1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c`) ship with them. Registry,
`cargo package --list`, and packaged-crate extraction tests prove that all 82 sources, the generated
manifest, and both notice files are present byte-for-byte. Binary distribution documentation must
ship the same notices. This does not silently choose typokat's own crate license. The 82 files are
the sole default-profile closure; TypeScript's other selectable `lib*.d.ts` files are not part of
the product or package. Production uses no host discovery.

User behavior is cross-checked with:

```text
tsc --strict --target es2025 --noEmit <user inputs>
```

The library-origin ledger is cross-checked independently, with automatic library loading disabled
and all registry paths explicit:

```text
tsc --strict --target es2025 --noEmit --noLib <all 82 manifest paths>
```

### One shared library-global compiler pipeline

One `LibraryCompiler` pipeline serves both the shared-base build and the collision fallback. It
parses the 82 files in registry order on an owning large-stack thread. A dedicated library binder
path binds every script declaration unit into one shared library-global scope in that same order;
it does not call the current `ProjectBinderBuilder::add_module`, which would isolate the files in
separate module scopes. An external-module library file retains its private module scope, and its
`declare global` fragments merge into the same library global; `export {}` must never turn adjacent
script files into module-local declarations. The empty/reference-only root remains last. Input
order, source keys, script/module classification, and global-fragment routing are pinned tests.

`globalThis` and the global `undefined` value are compiler-synthesized standard-library bindings,
not declarations injected into or rewrites of vendored sources. The library compiler reserves
their ordinary binder/value identities deterministically before dependent publication, while
normal lexical lookup still permits legal local shadowing. Their exact type/value/global-object
semantics and collision behavior are part of the strict-`tsc` WU0 matrix; a bare name-based
expression exception is not the shipped implementation.

The compiler keeps library events in a separate `LibraryEventLedger` keyed by library-file ordinal,
source start, event ordinal, and record ordinal. Library files never consume user `ModuleOrdinal`
or `UnitSlot` values. Freeze succeeds only when:

- every reserved type, type parameter, class, group, namespace, and value is terminal;
- parser/checker output exactly matches the audited library ledger;
- no unavailable declaration publishes an error/`any`/empty-object success; and
- all allocators, AST references, construction drafts, and pass-local query/cache state have
  dropped.

The output `FrozenLibraryBase` owns the immutable type store and dedup buckets, immutable library
binding/global tables, published semantic environments, declaration value types, a
`LibrarySemanticIdentities` table, root-name index, and next-id counters. The identity table covers
compiler-synthesized bindings and every declaration selected by a bridge or intrinsic rule. It is
universe-local: the shared fast path uses the table frozen with the shared base, while a private
rebuild produces and uses a different table in its private universe. No `TypeId`, binder ID, or
semantic identity crosses between those universes. The base must be `Send + Sync + 'static`.
Every name in the frozen library-global binder is asserted present in the root-name index, including
unavailable or unpublished slots.

Unexpected library-origin output is never suppressed. During shared initialization it is a typed
failure. During a private rebuild it is either the exact pinned baseline or a deterministic record
owned by an already-reserved user augmentation site; any other new library-origin outcome fails the
run.

### Fast path: immutable base plus an identity-preserving delta

Every non-colliding check uses `Arc<FrozenLibraryBase>` plus a private delta:

- base `TypeId`s and binder IDs form immutable prefixes; delta IDs start at the frozen lengths;
- reads route by prefix;
- interning probes frozen structural buckets before local buckets, so an existing shape reuses the
  base `TypeId`;
- delta rows may reference base or delta IDs, while base rows never reference or mutate a delta;
- type-parameter and class IDs start from frozen counters; and
- scopes, symbols, declarations, groups, namespaces, and value storages follow the same prefix rule.

Structural equality remains `TypeId == TypeId` within one run. This is not the stable cross-run hash
reserved for Stage 2. No base table is cloned or remapped per project, and no delta is visible to
another project.

This narrowly revises architecture §8.3 rather than abandoning its ownership rule. User parsing
and binding remain allocator-owned, interner-free work associated with each user source; no user
AST, mutable user binder row, or user delta is shared. Only the AST-free frozen library binder
tables become a shared immutable prefix. A private collision rebuild instead owns one complete
library-plus-project binder and interner on its compilation thread.

### Conservative preflight and correctness-first private rebuild

Before creating a delta, reserving a user event, or touching any evaluator/projection/relation
cache, the driver runs one shared binder/preflight name walker over the user AST. It uses the same
script-versus-external-module classifier as binding and compares global roots with the complete
frozen library-global name index.

The census includes every script top-level binding leaf, including destructuring; every value,
type, and namespace root; every member of `declare global`, even when placement or syntax will later
be diagnosed; every explicit `globalThis` extension; and root namespace/UMD forms. Module-local
declarations are excluded only through the shared external-module classifier. The walker is
exhaustive over relevant OXC statement, declaration, binding-pattern, and module forms.

If any such root name collides with any frozen library-global name—ready, unavailable,
unpublished, or incompatible—the run does not use the frozen semantic base. Independently of name
collision, the conservative initial classifier also routes every script/`declare global` value
declaration that may contribute a property to the effective global object, including a unique new
name, and every explicit `globalThis` extension. A class of those forms may return to the fast path
only if WU0 directly proves that it cannot change any frozen global-object, `typeof globalThis`,
name-resolution, or dependent-library surface.

The fallback invokes the same registry/compiler pipeline to bind the embedded library and the
entire project before lowering either side, then builds and publishes them together in one private,
mutable universe. This preserves ordinary declaration merging and makes library dependencies see
the merged groups. It publishes no shared state, mixes in no shared-base identity, and cannot fall
back to the unaugmented frozen surface. If a required merged surface is unsupported, that complete
surface is typed unavailable at its user owner; no base prefix is accepted as success.

Opposite user input orders must produce identical per-source semantics and records after mapping by
normalized source identity. Within each invocation, however, `ModuleOrdinal` follows that
invocation's original input order and user records retain the binding four-key replay order; the
two invocations are not required to serialize records in byte-identical global order.

This private rebuild is the production correctness path, not a partial overlay. It deliberately
avoids AST-free reconstruction recipes, reverse-dependency SCCs, a `ProjectPublicationOverlay`, or
a second publication authority. Optimization of collision runs requires measurement and a new ADR.

`check_files` bounds private rebuild concurrency to one process-wide permit initially. The permit
controls only full private compilations, not normal deltas or the shared base; it owns no semantic
state. An all-colliding fanout must remain memory-bounded and its serialized wall-time scaling is a
recorded acceptance gate rather than an assumption that collision is rare.

### Exact initialization and public API lifecycle

The production singleton is:

```text
OnceLock<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>>
```

It caches deterministic failure as well as success and is never retried or reset. Initialization
occurs before rayon fanout. Concurrent callers wait, then clone the same `Arc`; no caller observes
construction state. The initialization path returns errors rather than using
`debug_assert!`, `expect`, or `panic`.

This is an intentional pre-1.0 public API break:

```text
check_source(...)  -> Result<CheckOutput, Arc<LibraryInitError>>
check_files(...)   -> Result<Vec<FileReport>, Arc<LibraryInitError>>
check_project(...) -> Result<Vec<FileReport>, Arc<LibraryInitError>>
```

`LibraryInitError` records whether failure occurred in shared initialization or private rebuild.
The CLI maps it to the existing infrastructure/usage exit code `2` with the stable prefix
`error: failed to initialize embedded TypeScript 6.0.3 library:` and renders no partial user
results. Tests inject a provider/base and may explicitly expect initialization; they never mutate,
reset, retry, or order-depend on the production singleton.

The official-suite harness currently starts one process for each of 874 cases; repeating cold
initialization would make the ratchet unusable. Before cutover it gains a single-process isolated-
case batch protocol. The process initializes the production base once, then checks each case with a
fresh allocator, binder delta, type delta, event domain, and report. Cases are never combined into
one project and cannot share user declarations or deltas. Per-case diagnostics, incomplete records,
infrastructure failure, and timeout accounting remain explicit in the harness output.

User lexical reservations remain exact in both modes. A private rebuild reserves user tickets
before semantic construction and writes at most the existing cardinality for the user declaration
or demand owner. Library-ledger records never replay as user events. A typed unavailable library
surface carries its frozen cause; a user demand fills the preallocated lexical ticket for that
source site, with the existing four-key replay order and no cache-order duplication. This preserves
ADR-0008's event ownership rather than introducing dynamic library diagnostics.

### Bridges are evidence-selected and universe-identity keyed

The library bridge is one checker-owned boundary. WU0 must populate a necessity matrix with a clean
and failing strict-`tsc` witness, current typokat outcome, required
`LibrarySemanticIdentities` field, and decision for each candidate. The matrix explicitly covers
mutable and readonly array/tuple projection; primitive wrapper apparent members; object apparent
members; callable/`Function` apparent members; regular-expression literals; and the synthesized
`globalThis`/global-object surface. Only rows proven necessary may ship. Backlog 14 already requires
the `Array<T>`/tuple member and heritage bridge and the `RegExp` literal bridge; every other row,
including readonly collections, primitive wrappers, `Object`, and callable/function projection,
remains a candidate until WU0 proves it. No speculative class/value reconstruction breadth is
authorized.

A selected bridge projects syntax-native types through the applicable effective standard-library
declaration without changing primitive identity or unrelated relation rules. Shared mode uses its
frozen base's universe-local identity table; private mode uses the table produced by that private
compilation. Neither mode uses arbitrary names or imports identities from the other. Ordinary `Promise`,
iterator/generator, constructor, `Math`, `JSON`, `console`, `Intl`, DOM, and other globals remain
normal declarations unless the matrix proves a native representation needs a bridge.

The four TypeScript string `intrinsic` aliases, contextual `ThisType`, and any retained
`OmitThisParameter` specialization are likewise keyed by universe-local declaration identity. Vendored
sources are not rewritten. The curated prelude's `ReturnType` divergence is re-audited against the
authoritative definition before cutover.

### Performance gates precede implementation and cutover

The feasibility timeouts above make performance part of WU0, not closure polish. WU0 instruments
registry validation, parse, bind, reserve/fill, publication/validation, and library statement
checking separately, then runs at least five fresh release processes on the recorded host with
ordinary warm filesystem cache. Every sample must initialize the shared base and check one tiny
non-colliding source in at most 5.00 s wall time and at most 512 MiB process maximum RSS as reported
by `/usr/bin/time -v`. Failure is a hard stop under this ADR; no base/delta, bridge, or cutover work
proceeds by declaring the threshold aspirational.

After initialization, a same-process isolated-case benchmark batches enough empty/small checks to
report stable p50 and p95 distributions. Both must be no worse than 1.25× the corresponding
user-only delta/provider baselines measured in the same process. This relative gate replaces a
flaky two-millisecond whole-process wall threshold. Direct inspectors additionally prove that warm
runs perform zero library parse/bind/check work, allocate or clone no base-sized store/binder/
declaration-value table, and keep per-check allocation independent of the 82-file base size.

The remaining binding gates are:

- 1, 2, and 32 non-colliding workers observe one pointer-identical base;
- one collision run, measured while the shared base remains retained, includes the private
  library compiler's peak live state, the resulting private library-plus-project universe, and the
  user state, yet keeps total process maximum RSS at or below 512 MiB;
- the private path completes within the separately recorded finite fallback wall budget; and
- all-colliding fanout with the one-permit limiter stays at or below the measured single-fallback
  RSS ceiling and within 1.25× linear serialized wall time.

The sprint may tighten these thresholds. Weakening them or accepting an unbounded cold/private
path requires explicit approval and a superseding decision; it cannot happen silently in the run
log.

### Atomic replacement of the minimal prelude

The current minimal prelude remains the production path through the registry, compiler, delta,
preflight/fallback, bridge, and profiling work. One atomic driver cutover then replaces
`PRELUDE_SOURCE`/`bootstrap_trusted_prelude` for single-file, parallel-file, and serial-project
entry points. Only after all three use the new pipeline is `src/prelude.ts` removed.

This narrowly supersedes ADR-0003's temporary minimal slice and ADR-0004's concrete
`src/prelude.ts` handoff. Their one-canonical-pipeline, ordinary lookup, index-alignment, and
no-special-global principles remain binding. ADR-0006/0008/0009/0010 immutable publication and
event rules remain binding. ADR-0007 still governs Bundler module resolution, not the embedded lib.

## Consequences

- Non-colliding checks share one immutable semantic base while retaining worker-local ASTs and
  deltas.
- A global collision pays a bounded private full rebuild, but gains ordinary same-universe merging
  without shared mutation, partial publication, or a second query/publication overlay.
- Collision detection becomes a soundness boundary. Its root-name census, classifier parity, and
  selection-before-mutation order require exhaustive direct tests.
- The checked-in package grows by roughly 2.9 MB plus TypeScript license/notice and registry data.
- The first initialization remains real semantic work. Profiling and an approved finite cold budget
  are prerequisites, not closure polish.
- Driver callers must handle `Result`; this is accepted before 1.0 and is migrated atomically.
- The official-suite ratchet uses one process with a fresh isolated delta per case, avoiding 874
  repeated cold builds without changing case semantics into one project.
- Updating TypeScript is an explicit profile migration with new sources, fingerprints, audit
  ledger, performance evidence, and review.

## Alternatives considered

Weights: soundness/no false-clean 30%, identity/order determinism 25%, warm sharing 20%,
reversibility/delivery risk 15%, cold startup 10%. Scores are 1–5; totals normalize to 100.

| Alternative | Soundness | Identity | Warm | Reversibility | Cold | Total |
|---|---:|---:|---:|---:|---:|---:|
| A — rebuild library for every run | 4 | 4 | 1 | 5 | 1 | 65 |
| B — frozen base/delta + private collision rebuild | 5 | 5 | 5 | 3 | 3 | **90** |
| C — build-time serialized semantic snapshot | 4 | 5 | 5 | 1 | 5 | 82 |

### A — rebuild the library for every run

This remains the simplest WU0 correctness baseline and the selected collision path. It is best if
sharing is abandoned or every real project collides. It is not the common path because the measured
full-closure probes already time out and per-worker duplication violates Stage 1.

### B — frozen base/delta with private collision rebuild

Selected. It gives the common path exact shared identity and bounded worker memory, while routing
the hard declaration-merging case through the already-understood one-universe model. It costs a
store/binder prefix refactor, conservative preflight, and a slow bounded fallback.

### C — build-time serialized semantic snapshot

Best only if measured cold initialization remains dominant after B and the store has a stable,
portable serialization schema and content identity. Today it creates a second compiler/artifact
compatibility boundary. It is a possible later optimization, not a parallel loader.

## Falsifiability and exit

Stop before cutover if any condition holds:

1. A frozen product retains AST/allocator/pass-local mutable state or cannot become
   `Send + Sync`.
2. Base/delta interning cannot preserve structural `TypeId` equality without base mutation or
   per-project base cloning.
3. The shared preflight cannot conservatively enumerate every global root and global-object value
   contribution or select fallback before delta, user-event, or semantic-cache mutation.
4. The private same-universe path cannot reproduce legal library/user merging, user ordinal/event
   isolation, opposite-order determinism, or exact library-ledger handling.
5. WU0 finds a profile mismatch, unowned outcome, permissive unavailable surface, or misses the
   five-second/512-MiB release gates.
6. A required bridge needs rewritten vendor files, scattered name cases, or unproven class/value
   reconstruction.
7. A warm/private/fanout gate fails, or the singleton requires a retry/reset/panic path.

Until cutover, the current prelude remains production. If the frozen model fails, retain only the
audit/disabled spec; do not ship a dual loader, mutable shared base, partial base, or manual global
shim. A new architecture needs a superseding ADR. If only cold startup fails after the in-memory
model proves correct, alternative C becomes eligible for a separately measured decision.
