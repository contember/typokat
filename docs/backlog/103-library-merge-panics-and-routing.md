---
id: 103
title: Merging a declaration into the frozen library panics; there is no collision route
blocked-by: []
---

# 103 — Merging a declaration into the frozen library panics; there is no collision route

**Summary.** Any user declaration that merges with a library-owned name **panics** the checker —
`interface Window`, `declare global { … }`, `namespace Intl`, `class Date`, `type Partial<T>`, all of
which `tsc 6.0.3 --strict` accepts. Five distinct panic sites, one underlying cause: an in-place
merge into the frozen prefix. The classifier that is supposed to route these inputs away exists
(`src/library/collision_preflight.rs`) but is **never called on real input** in production, and the
private-rebuild route it would select is not implemented. This is sprint WU5's territory. Effort XL
for correct semantics, M for the guard. **Blocks the WU7 CLI cutover outright.**

## Problem

Every row below is legal TypeScript that `tsc 6.0.3 --strict --target es2025` accepts, and every one
panics the checker on the library base:

| Input | Panic site |
|---|---|
| `interface Array<T> { … }` at script top level | `src/binder/bind.rs:2179` `allocated type group exists` |
| `interface Window { … }`, `interface String { … }` split over two files | `bind.rs:2179` |
| `type Partial<T> = …`, `class Date { … }` | `bind.rs:2179` |
| `interface console { probe: number }` | `bind.rs:2230` `resolved symbol exists` |
| `namespace Intl { … }`, `declare namespace Intl { … }` | `src/binder/namespace.rs:5447` `namespace exists` |
| `declare global { interface Window { … } }` | `bind.rs:2179` |
| `declare global { interface Brand {} }` (fresh name), `declare global { var x: number }` | `src/check/checker/mod.rs:1267` |

Two of the four `tooling/full-lib-bench` rows — `collision` and `fanout` — exit 101 for this reason,
so the cross-tool benchmark cannot even run on half its matrix.

## Root cause

The same `LayeredVec` boundary as [`102`](./102-frozen-prefix-writes-vanish-silently.md), reached by
callers that `.expect(...)` instead of dropping the write. `declare_type` (`bind.rs:2148`) reuses an
existing symbol's type group and appends a fragment; when that group id belongs to the frozen prefix,
`get_mut_local` returns `None`:

```
array_type_group = TypeGroupId(48)   type_groups.base_len() = 2099
48.checked_sub(2099) → None → panic
```

`mod.rs:1267` is different in kind: `finish_frozen_library_continuation` (`bind.rs:1396`) already
detects the case and returns a typed `Err("frozen-library continuation does not yet admit declare
global")` — and the caller `.expect()`s it. The single-source path
(`check_program_with_owned_library`, `mod.rs:1428`) propagates it correctly with `?`; only the
project path unwraps.

**There is no classifier on this path.** `fork_user_delta` (`src/library/base.rs:481`) takes its
capability from `issue_caller_certified_capability()` (`collision_preflight.rs:30`), which runs the
preflight over an **empty** input set — no inputs, no candidates, so it always returns
`CollisionRoute::SharedDelta`. The whole module is `allow(dead_code, reason = "activated by the WU5
private-route cutover")`. The frozen `root_names` index is never consulted for real user source.

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

**Guard first (effort M), correctness second (effort XL).** They are separable and the guard is
urgent, because a panic on legal input is worse than a refusal.

*Guard.* Give the preflight a production entry point taking real `FileInput`s and the base's
`root_names`, call it before `fork_user_delta`, and map anything other than `SharedDelta` to a typed
error and a stable CLI exit with no partial output. Run it **inside** the large-stack worker — the
sprint run log records the preflight overflowing the caller's 8 MB stack on a 471-file census. Then
fail closed independently: convert the five `expect` sites to recorded typed failures and make
`mod.rs:1267` propagate. The classifier must not be the only thing standing between a user and a
panic.

*The guard is not the cutover.* Route incidence over the official-suite corpus is **shared 285 /
private 185 / rejected 1** — 39% would be refused, because script-mode `var`/`function` are
global-object contributors. Refusing `declare global` and 39% of inputs is a product regression
against `src/prelude.ts`, so ship the guard while the prelude remains the CLI default.

*Correctness.* [ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md)'s private full
rebuild is the answer, built on the checkpoint continuation that already works. Cost: a colliding
project repeats compile + publication in a fresh universe, so ~0.30 s → **~0.6 s**. That is fine for
one project and ruinous for a batch — ~341 colliding official-suite cases × 0.29 s ≈ 100 s added to
one process under ADR-0011's one-permit bound. This is exactly why WU5 says the current source
compiler cannot satisfy the 2× collision row, and why the replay-index machinery
(`AdmittedCollisionReplayIndex`, 47,253 owner sites) is specced as the optimization *over* the naive
rebuild. Treat that optimization as a separate item; do not fold it into the correctness work.

Corpus first per [`dev-method.md`](../reference/dev-method.md) §1. Beyond the panic shapes above it
must pin: module-scope declarations that must keep **shadowing** rather than merging (`interface
Array<T>` inside a module, a module-local `Date`), fresh names that must stay on the shared delta,
a classifier false-negative mutation routing private rather than shared, opposite input order
producing identical per-source semantics, and no identity of any kind crossing between a private
universe and the shared base.

## Touch points

`src/binder/bind.rs`, `src/binder/namespace.rs`, `src/check/checker/mod.rs`,
`src/library/collision_preflight.rs`, `src/library/base.rs`, `src/driver.rs`,
`tooling/full-lib-bench/workloads/collision/`, `tooling/full-lib-bench/workloads/fanout/`.

<!-- Origin: found 2026-07-26 when the production-shaped CLI was first pointed at the library base
     and two of four benchmark rows exited 101; characterised by the family-1 diagnosis work unit. -->
