# Sprint — maintenance cleanup: gate, dedup, first hotspot split (2026-08-14)

**Goal.** Restore a usable local and pull-request correctness gate, remove the
high-confidence semantic duplication that survived adversarial review, and complete one
behavior-preserving decomposition of a current production hotspot.

**Theme.** A current-HEAD audit found one immediate maintenance blocker: the marker corpus
performs an 82-file complete-source library compilation for every fixture project. That conflates
marker conformance with ADR-0021 route parity and makes the ordinary feedback loop unusable. This
sprint separates their cadence without deleting or sampling either contract, then uses the restored
gate to remove two proven duplication families and split one root. It is not a repo-wide cleanup.

Success means a fast full marker gate, a bounded pull-request route-parity matrix, the exact full
shared/cold sweep retained as a named scheduled and closure gate, zero official-suite movement, and
one independently reviewed namespace-module decomposition.

The unstarted 2026-07-16 namespace plan is
[`archived as superseded`](../archive/sprint-2026-07-16-namespace-binder-refactor.md). Its old line,
clone, consumer, and snapshot measurements are historical only.

## Refs re-verified at HEAD (2026-08-14, `6207565`)

`✔` = confirmed live · `⚠` = drift or constraint caught by the audit/refutation pass.

- ✔ `tests/conformance.rs:687` and `:691` run every flat fixture through both `check_project` and
  `check_project_once`; `:745` and `:749` do the same for every project fixture.
- ✔ The enabled inventory expands to **567 fixture-project executions and 683 TypeScript source
  files**: 420 whole-corpus flat runs, 74 whole-corpus project runs, 67 individually enabled flat
  runs, and 6 individually enabled project runs. The harness schedules 1,134 route checks.
- ✔ `crates/typokat-driver/src/driver.rs:305-318` shows that every cold route supplies the complete
  packaged 82-file profile to `compile_complete_source_project_programs`. The corpus therefore
  schedules 46,494 default-library source inputs before user sources.
- ⚠ The audit's conformance process remained CPU-active for more than 25 minutes and was stopped;
  `CLAUDE.md:41` still describes the whole suite as about 12 seconds. This is not a passing test
  result, and no replacement timing is accepted until WU0 measures it under `cpu-lease`.
- ✔ ADR-0021 requires permanent shared/cold parity over lifecycle-sensitive risk classes, but does
  not require every marker fixture to execute both routes on every pull request. Existing
  `tests/wu6_complete_source_cli.rs:194-646` already covers imports, failures, cycles, the B102/B103
  matrix, both input orders, global/library identity, parse errors, and incomplete records.
- ✔ The marker harness exact-diffs diagnostic code multisets, rendered substrings, and incomplete
  identities. TypeScript fixtures are executable specifications; WU0 changes route cadence, not
  marker semantics or fixture contents.
- ✔ Five binder Statement/Declaration walker pairs contain approximately **309 normalized common
  lines**, plus one checker mirror. The audit measured this with GNU diff LCS after normalizing only
  the `Statement`/`Declaration` variant prefixes. Current entry points are
  `namespace.rs:5451/5515`, `:5671/5958`,
  `:6543/6598`, `:6826/7009`, `bind.rs:2263/2403`, and
  `namespace_values.rs:2692-2724`. Oxc's `Statement::as_declaration()` is already used in
  production.
- ✔ `NamespaceTable::classify` at `namespace.rs:2465` contains seven canonical-id suffix builders.
  All clear and refill only the `LayeredVec` local suffix. The namespace-fragment and merge-row
  in-place sorts have different sentinel and validation contracts and must stay explicit.
- ✔ `SemanticQueryCoordinator::is_assignable` at `query/mod.rs:1873` and
  `overload_implementation_compatible` at `:2106` duplicate 60 normalized lines of planner,
  relater, demand retry, taint, cache-promotion, and plan-commit protocol. Assignability alone owns
  its fast paths, detailed reasons, and completed-relation memo.
- ✔ `crates/typokat-binder/src/binder/namespace.rs` is **12,618 lines**: 7,694 production lines
  before its trailing test module and 4,924 test lines. It contains a 1,781-line
  `NamespaceTable` impl and a 1,146-line `Binder` impl. This is the single decomposition root
  selected for the sprint.
- ✔ The wider audit found 54 of 168 production files over 800 lines and 39 over 1,200. Decomposition
  of `library_compiler.rs`, `checker/mod.rs`, `frontend.rs`, and `decls/mod.rs` remains real work,
  but scheduling all of them here would make review and rollback ineffective.
- ✔ Adversarial refutation rejected four tempting expansions: compact/exact replay construction
  must stay independent as an ADR-0020 oracle; full/partial inference transactions have different
  ownership; sparse replay fallback failures already have sufficient performance and fault
  evidence; and four ignored full-profile tests duplicate stronger active gates.

## Work units

### WU0 — separate marker conformance from ADR-0021 parity cadence (effort M)

- **Problem.** One test owns two valid contracts. The full marker inventory should be the ordinary
  correctness gate, but every fixture also performs a fresh 82-file complete-source compilation to
  prove lifecycle parity. Static expansion gives 567 cold compilations per run.
- **Verify first.** Before changing route selection:
  1. derive and print the enabled project/source inventory from the discovery path the harness uses;
  2. add harness-local per-route counts and elapsed-time receipts without production changes;
  3. run the current `tests/wu6_complete_source_cli.rs` matrix; and
  4. run the existing exact shared/cold sweep once under `cpu-lease run -n 2`. If it fails or does
     not finish within 45 minutes, stop and record the baseline; do not weaken it.
- **Scope.** Extract one deterministic inventory and route-execution mode from
  `tests/conformance.rs`. Make the default test run every marker once through the shared route. Add
  an exact-named ignored full-parity test over the identical inventory and exact `FileReport`
  comparison. Keep `tests/wu6_complete_source_cli.rs` as the pull-request ADR-0021 risk matrix.
  Add a weekly/manual CI gate for the full sweep and require it at sprint/milestone closure. Put the
  long gate in a separate workflow or an event-specific concurrency group: scheduled runs must not
  share the current push group's `cancel-in-progress: true`, and `workflow_dispatch` must be
  available. Deterministic sharding is allowed only by stable logical id and must prove a disjoint
  union equal to the unsharded inventory. Do not cache the cold semantic product.
- **Acceptance / witness.** No `.ts` fixture or marker changes. The default gate reports exactly
  567 fixture projects / 683 source files and one shared execution per project. The pull-request
  parity matrix remains green across imports, merges, parse failures, diagnostics, incompletes,
  ordering, collision/global identity, and provider isolation. The named full-parity command covers
  the identical inventory with one shared and one cold execution per project and zero mismatches.
  On the documented host, three warm runs of
  `cpu-lease run -n 2 -- cargo test --test conformance conformance -- --exact` must have a median at
  or below 60 seconds and no run above 120 seconds. The unsharded full-parity command must finish
  within 45 minutes under the same lease; its CI job has a 60-minute timeout. Published timings
  change only after both commands finish under these conditions.
- **Touch points.** `tests/conformance.rs`, `tests/wu6_complete_source_cli.rs`,
  `.github/workflows/ci.yml`, `tests/cases/README.md`, `docs/reference/dev-method.md`, and
  `CLAUDE.md`.

### WU1 — close the high-confidence hygiene batch (effort S)

- **Problem.** Small stale artifacts obscure live ownership and test inventory. The lockfile also
  retains low-reachability `RUSTSEC-2026-0204`, patched in `crossbeam-epoch 0.9.20`.
- **Verify first.** Recheck every comment and call site after WU0. Confirm with `cargo audit --json`
  that this is still the only advisory and that a precise lock update changes only
  `crossbeam-epoch`. Derive counts from WU0 receipts. Measure timings only under `cpu-lease`.
- **Scope.** Land three atomic commits inside the batch: (a) source/test hygiene, (b) the precise
  lockfile patch, and (c) WU0-derived documentation. Replace four ADR-0015 replay-scheduler comments
  with their ADR-0020 full-index-oracle
  role, while keeping the scheduler. Correct `library_compiler.rs`'s measurement-only module
  description. Delete only the ignored pre-WU6 RED test that declares itself obsolete. Update only
  `Cargo.lock` to `crossbeam-epoch 0.9.20`. Refresh README counts and CLAUDE timings from WU0's
  generated evidence; do not create a second count authority.
- **Acceptance / witness.** No pending-production ADR-0015 wording or obsolete pre-WU6 test remains.
  The replay oracle and both call sites remain. `cargo audit` reports no advisory. The lockfile diff
  contains only the patch upgrade. Every published count and timing names its reproducing command.
- **Touch points.** `crates/typokat-check/src/check/checker/replay_index.rs`,
  `crates/typokat-check/src/check/checker/library_compiler.rs`,
  `tests/b15_default_module_slots_cli.rs`, `Cargo.lock`, `README.md`, and `CLAUDE.md`.

### WU2 — collapse declaration mirrors and canonical-id boilerplate (effort M)

- **Problem.** Five binder walker pairs and one checker path duplicate declaration dispatch in
  Statement and Declaration form. `NamespaceTable::classify` separately repeats a load-bearing
  layered-suffix fill-and-sort protocol seven times. Both findings survived adversarial review.
- **Verify first.** Re-diff every pair and enumerate Oxc's nine inherited Declaration variants.
  Record every wrapper, import/runtime branch, overload prepass, `explicit`/`exported` flag, and
  order constraint. For canonical ordering, record all fourteen key closures. Mark the two in-place
  sorts and `canonical_namespaces` missing-fragment sentinel as protected exceptions. Record the
  exact pair ranges, prefix-normalization script, per-pair LCS counts, and strict jscpd command in
  this sprint's run log before editing; the after-scan must use the same commands.
- **Scope.** Delegate only direct declaration-bearing Statements through
  `Statement::as_declaration()`. Keep wrappers, imports, runtime statements, UMD/global handling,
  overload prepasses, unsupported-member gates, and direct/wrapped flags in their current layer and
  order. Give the checker a small dispatcher after its existing gates. Add one private generic
  helper for the seven canonical-id collections: `clear_local`, rebuild only `base_len..len`,
  `push_local`, and sort only `local_slice_mut`. Keep explicit typed library/user key closures at
  each call. Preserve all sentinels and leave both in-place sorts explicit.
- **Acceptance / witness.** No semantic fixture edits. Focused namespace/binder/checker tests and
  the default marker gate pass. Existing layered and ordering witnesses stay unchanged. A fresh
  duplication scan no longer reports the five walker pairs or seven suffix protocols. The
  official-suite ratchet passes with `scoreboard.txt` unchanged. An independent adversarial review
  targets direct/wrapped declarations, both source orders, sparse suffixes, and sentinel paths.
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs`,
  `crates/typokat-binder/src/binder/bind.rs`, and
  `crates/typokat-check/src/check/checker/namespace_values.rs`.

### WU3 — share the taint-sensitive relation transaction (effort M)

- **Problem.** Assignability and overload-implementation compatibility have different public
  semantics but duplicate coordinator machinery that must evolve in lockstep: planner roots,
  demand retries, `RelationCache` handoff, planning/cycle taint, exhausted-result promotion, and
  projection/evaluation commit.
- **Verify first.** Pin an operation-contract table before editing: root/observation order,
  publication precheck, fast paths, attempt method, diagnostic shape, completed memo behavior,
  relation-cache kind, cycle taint, and final result. Run focused query tests for overload laziness,
  poisoned class boundaries, tainted rollback, cycle-tainted cache suppression, and completed
  relations.
- **Scope.** Extract one private transaction driver selected by a two-variant operation enum, not a
  callback framework. It owns planner construction, demand expansion, relater construction, cache
  promotion, planner finish, tainted-success exhaustion, and plan commit. `is_assignable` keeps its
  fast paths, detailed reasons, completed memo, and cycle-taint admission rules. Overload
  compatibility keeps its root order, leaf diagnostic policy, distinct Relater operation, and no
  completed-operation memo. Do not add an overload `RelationKind` or change nested assignability
  caching.
- **Acceptance / witness.** Public methods and call sites are unchanged and one transaction
  implementation remains. Existing taint, laziness, cache, and completed-relation tests pass. No
  semantic fixture changes. The default marker and pull-request parity gates pass, and the official
  ratchet has zero scoreboard movement. An independent review exercises `Yes`, both `No` shapes,
  `Exhausted`, `NeedsBatch`, both taints, cache promotion, and assignability-only memoization.
- **Touch points.** `crates/typokat-check/src/check/query/mod.rs`; focused query tests only if a
  missing invariant must be pinned.

### WU4 — move-only decomposition of `binder::namespace` (effort L)

- **Problem.** After discounting tests, `namespace.rs` still contains 7,694 production lines and
  multiple phase owners. It is the top measured hotspot and the only root simplified by WU2.
- **Verify first.** At post-WU2 HEAD, regenerate a symbol-to-line map and complete external-consumer
  list. Before implementation, append a durable move manifest to this sprint that maps every moved
  symbol to its destination and lists every permitted visibility change; an independent reviewer
  must approve it. Confirm stable `binder::namespace::*` paths, the test-only glob consumer in
  `binder/references.rs`, and descendant access for tests. Recalculate every proposed child size;
  current line ranges are routing hints, not implementation coordinates.
- **Scope.** Keep `binder/namespace.rs` as the stable facade and create same-crate children under
  `binder/namespace/` for: models/ids, layered table and canonical state, classification, qualified
  lookup, metadata collection, ordinary walking/binding, deferred modules, namespace values, and
  tests. Freeze the exact boundary map before the move. Preserve external paths through facade
  re-exports. Bodies move unchanged; only module declarations, imports, re-exports, and minimum
  visibility preservation are allowed. Do not widen items to `pub`, split crates, redesign state,
  change phase order, rename public items, or combine logic cleanup with the move.
- **Acceptance / witness.** The move commit changes only `namespace.rs` and new files beneath
  `binder/namespace/`. External and test-only imports compile unchanged. An independent reviewer
  compares production bodies against the move manifest; every non-move line is an import, module,
  re-export, or visibility-preservation change. The facade contains only module declarations,
  re-exports, and at most 250 non-test physical lines; no production child exceeds 1,500 physical
  lines. Test modules are excluded from that size cap, and no test names or bodies change. Focused tests,
  `cargo test --no-run`, the default marker gate, the pull-request parity matrix, the exact full
  closure sweep, and the official ratchet all pass with zero scoreboard movement.
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs` and new files under
  `crates/typokat-binder/src/binder/namespace/` only.

## Out of scope (explicit)

- Decomposition of `library_compiler.rs`, `checker/mod.rs`, `frontend.rs`, or `decls/mod.rs`.
  Remeasure after WU4 and create a follow-on sprint for exactly one next root.
- Repo-wide extraction of inline test tails. Namespace tests move only because WU4 changes that
  module tree.
- Compact/exact collision-replay deduplication: the duplicate construction is deliberate ADR-0020
  oracle diversity.
- Full/partial signature-inference deduplication, inference policy, contextual typing, argument
  walking, and overload resolution. The proposed shared preparation object was larger and riskier
  than the duplicate.
- New sparse-replay fallback cause APIs. Existing counters and typed fault evidence already enforce
  the performance contract.
- A new release-libtest CI job for the four ignored full-profile tests. Stronger active replay and
  provider gates cover their claims.
- Removal of namespace metadata that appears externally unused. A move-only WU may not decide
  liveness.
- Frontend current-directory error propagation, typobench cleanup semantics, benchmark host
  provenance, tuple-rest deduplication, major dependency upgrades, or open checker-scaling
  backlogs 85, 86, 89, 94, and 106.
- Any semantic TypeScript fixture, diagnostic baseline, official scoreboard, public API, crate
  graph, serialized format, route behavior, or accepted ADR change.

## Decisions

- **Preserve total coverage; change cadence only.** Every marker remains a pull-request assertion.
  ADR-0021 parity remains a pull-request risk matrix. Complete shared/cold parity remains exact,
  scheduled, and mandatory at sprint/milestone closure.
- **Gate first.** No semantic deduplication or large move starts until WU0 makes the ordinary loop
  usable and proves the named full closure command.
- **One abstraction per duplicate contract.** Statement walkers share Declaration dispatch but keep
  Statement policy. Canonical ordering shares only suffix filling. Relation operations share only
  their transaction, not public semantics or memo policy.
- **One root only.** Namespace is selected because it is the top hotspot, WU2 reduces it first, and
  its public module path can remain stable.
- **No ADR is expected.** Stop and write or supersede an ADR if implementation needs a new semantic
  authority, route contract, crate boundary, or persisted format.
- **Differential boundary.** Inference and contextual typing are untouched, so their special
  reference-binary/tsc differential gate is not added. If implementation reaches them, stop and
  rescope instead of expanding this sprint.

## Sequencing

1. WU0 blocks semantic cleanup and the move. Its inventory and full-parity command become the gate
   for every later WU.
2. WU1 may start its source/test-hygiene and lockfile commits while WU0 is measured, but its
   count/timing documentation is a third commit and waits for WU0.
3. WU2 and WU3 may run in parallel with disjoint source ownership after WU0. Each lands as its own
   atomic commit after independent adversarial review.
4. WU4 starts after WU2 and after WU3 is integrated. It is one move-only commit with a separate
   independent review.
5. Closure runs the normal Rust gates, the named exact full-parity sweep, release official-suite
   ratchet, `cargo audit`, and a zero-diff scoreboard check. Every CPU-heavy command uses
   `cpu-lease`.

## Run log

<!-- Append as work proceeds. Graduate changed rationale to an ADR and future work to backlog. -->

- **2026-08-14 — sprint drafted from audit at `6207565`; no WU started.** Scheduled findings are
  the amplified conformance gate, binder declaration mirrors, canonical suffix construction,
  duplicated semantic-query transaction, and one-root namespace decomposition. Refuted candidates
  and all other oversized roots are explicitly excluded above.
