---
id: 103
title: A merge into a library-owned name is refused, not performed; there is no collision route
blocked-by: []
---

# 103 — A merge into a library-owned name is refused, not performed; there is no collision route

**Summary.** A user declaration that merges with a library-owned name — `interface Window`,
`namespace Intl`, `class Date`, `declare global` — does not merge. **The guard tier shipped**
(`f1c7d7e`/`b223817`): those shapes used to panic and are now typed refusals, so `collision` and
`fanout` run for the first time. What remains is the correctness tier: making the merge actually
happen. The classifier exists (`src/library/collision_preflight.rs`) but is never called on real
input, and the private-rebuild route ADR-0011 mandates is not implemented. Effort XL. **Blocks the
WU7 CLI cutover**, because refusing every `declare global` is a product regression against
`src/prelude.ts`.

## Problem

Every row below is TypeScript that `tsc 6.0.3 --strict --target es2025` compiles (some with their
own duplicate-identifier diagnostics, noted in the corpus headers). None of them merges in typokat:

| Input | Outcome today |
|---|---|
| `interface Array<T>`/`String`/`Window`, `type Partial<T>`, `class Date` | refused; the fragment is not appended, so reads of the augmented member report `TK2339` |
| `interface console` | refused; the type slot stays the library's, so the annotation degrades to an error type and **every** member read through it goes unchecked |
| `namespace Intl`, `declare namespace Intl` | refused; qualified reads report `TK2694` |
| `declare global { … }` | the whole run is refused, exit 2, **no diagnostics at all for the project** |

The `interface console` and `declare global` rows are the ones to watch: a refusal that yields an
error type manufactures exactly the silent channel backlogs `45` and `101` were, and a whole-run
refusal is a blind spot over every file in the project. They are the price of the guard tier and
they disappear only when the merge works.

## Root cause

Frozen binder tables are a `LayeredVec` — an immutable `Arc<[T]>` base plus a mutable local — and
`get_mut_local` refuses any id below the prefix boundary, exactly as
[ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md) requires. `declare_type` reuses
an existing symbol's type group and appends a fragment, and on `lib.d.ts` that group id sits in the
prefix (`TypeGroupId(48)` against a base length of 2099). The guard tier turned that refusal into a
recorded `FrozenPrefixWrite`; it did not give the fragment anywhere to go.

**There is no classifier on this path.** `fork_user_delta` takes its capability from
`issue_caller_certified_capability()`, which runs the preflight over an **empty** input set — no
inputs, no candidates, so it always answers `CollisionRoute::SharedDelta`. The whole module is
`allow(dead_code, reason = "activated by the WU5 private-route cutover")`, and the frozen
`root_names` index is never consulted for real user source.

## What already exists

The binder half of the private route is **built and green**. `LibraryBinderCheckpoint`
(`bind.rs:161`) is an *unfrozen* library binder; `continue_library_project_binder`
(`library_compiler.rs:4086`) binds user files onto it, and because nothing is frozen the merge
succeeds. The committed test at `collision_replay_index_spec.rs:355` feeds it exactly the failing
`collision` workload and asserts the augmented type group keeps its identity.

What is missing is everything downstream: type reservation/publication, checking, and driver routing.
`src/library/mod.rs:38` and `:47` still have `private_combined_universe_spec` and
`private_replay_scale_spec` commented out.

## Approach / acceptance

The guard is done. What is left is [ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md)'s
private full rebuild, built on the checkpoint continuation that already works, plus the routing that
selects it.

*Route it.* Give the preflight a production entry point taking real `FileInput`s and the base's
`root_names`, and call it before `fork_user_delta`. Run it **inside** the large-stack worker — the
run log records the preflight overflowing the caller's 8 MB stack on a 471-file census.

*Cost.* A colliding project repeats compile and publication in a fresh universe, so ~0.25 s → ~0.5 s.
Fine for one project, ruinous for a batch: route incidence over the official-suite corpus is
**shared 285 / private 185 / rejected 1**, so ~341 cases × ~0.25 s ≈ 85 s added to one process under
ADR-0011's one-permit bound. That is why the replay-index machinery
(`AdmittedCollisionReplayIndex`, 47,253 owner sites) is specced as the optimization *over* the naive
rebuild. Treat that as a separate item; do not fold it in.

*Corpus.* `tests/cases/b103_library_merge_refusals/` and its project sibling already pin every shape
and both input orders — the acceptance is that their refusals become correct merges while the
controls (fresh globals publish, module scope shadows) stay exactly as they are. Add the cases the
guard could not reach: `globalThis`, UMD globals, value/type/namespace-slot collisions,
destructuring, a classifier false-negative mutation that must route private rather than shared, and
proof that no identity of any kind crosses between a private universe and the shared base.

## Touch points

`src/binder/bind.rs`, `src/binder/namespace.rs`, `src/check/checker/mod.rs`,
`src/library/collision_preflight.rs`, `src/library/base.rs`, `src/driver.rs`,
`tooling/full-lib-bench/workloads/collision/`, `tooling/full-lib-bench/workloads/fanout/`.

<!-- Origin: found 2026-07-26 when the production-shaped CLI was first pointed at the library base
     and two of four benchmark rows exited 101; characterised by the family-1 diagnosis work unit. -->
