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
- **Scope.** Keep the model private to `tests/conformance.rs`: one ordered `Inventory` of logical
  fixture projects, a `DiscoveryReceipt` with project/source pairs for the four registration
  classes, a `SharedMarkers` / `SharedColdParity` route mode, and a `RunReceipt` with total project,
  source, shared-execution, and cold-execution counts. Discovery happens once per test and records
  the breakdown while it builds the inventory; both modes consume that inventory without
  rediscovery, path heuristics, filtering, sampling, or reordering.
  Make the default `conformance` test run every marker once through the shared route. Add an
  exact-named ignored `conformance_full_shared_cold_parity` test over the identical inventory and
  ordered `FileReport` comparison. Exact semantic parity includes name, source, diagnostics, parse
  errors, and incomplete records; add cheap comparator negative controls for report order/name,
  source, parse errors, and incompletes. `project_notices` are route-envelope metadata and are
  deliberately not a parity field: `tests/b15_default_module_slots_cli.rs` remains their exact
  route-specific oracle. Keep `tests/wu6_complete_source_cli.rs` unchanged as
  the pull-request ADR-0021 risk matrix unless a missing invariant is proved first. Add a separate
  `.github/workflows/full-parity.yml` with weekly and `workflow_dispatch` triggers, a concurrency
  group distinct from push/PR CI, scheduled cancellation disabled, and a 60-minute timeout.
  Deterministic sharding is deferred from the minimal implementation. Do not cache the cold
  semantic product.
- **Acceptance / witness.** No `.ts` fixture or marker changes. The default gate reports exactly
  567 fixture projects / 683 source files and one shared execution per project. The pull-request
  parity matrix remains green across imports, merges, parse failures, diagnostics, incompletes,
  ordering, collision/global identity, and provider isolation. The named full-parity command covers
  the identical inventory with one shared and one cold execution per project and zero mismatches.
  A cheap `conformance_inventory_matches_frozen_receipt` test pins the 420/420 whole-flat,
  74/184 whole-project, 67/67 selected-flat, 6/12 selected-project, and 567/683 total receipt, plus
  sorted unique logical ids and source paths. The default receipt is 567/683/567/0 and full parity
  is 567/683/567/567. Successful completion of the common exact comparator is the zero-mismatch
  witness; do not add or print a literal mismatch counter.
  On the documented host, three warm runs of
  `cpu-lease run -n 2 -- cargo test --test conformance conformance -- --exact` must have a median at
  or below 135 seconds and no run above 150 seconds. These bounds are based on the current
  567-route corpus; the earlier 60/120-second draft bounds had no like-for-like measurement. The
  unsharded full-parity command must finish
  within 45 minutes under the same lease; its CI job has a 60-minute timeout. Published timings
  change only after both commands finish under these conditions.
- **Touch points.** `tests/conformance.rs` and new `.github/workflows/full-parity.yml`. Treat
  `tests/wu6_complete_source_cli.rs` and `.github/workflows/ci.yml` as read-only controls. All
  command/count documentation belongs to WU1's later documentation commit.

### WU1 — close the high-confidence hygiene batch (effort S)

- **Problem.** Small stale artifacts obscure live ownership and test inventory. The lockfile also
  retains low-reachability `RUSTSEC-2026-0204`, patched in `crossbeam-epoch 0.9.20`.
- **Verify first.** Recheck every comment and call site after WU0. Confirm with `cargo audit --json`
  that this is still the only advisory and that a precise lock update changes only
  `crossbeam-epoch`. Derive counts from WU0 receipts. Measure timings only under `cpu-lease`.
- **Scope.** Land three atomic commits inside the batch: (a) source/test hygiene, (b) the precise
  lockfile patch, and (c) WU0-derived documentation. The documentation commit exclusively owns
  `README.md`, `CLAUDE.md`, `tests/cases/README.md`, and `docs/reference/dev-method.md`; it consumes
  WU0's named commands and measured receipts rather than inventing another authority. Replace four
  ADR-0015 replay-scheduler comments
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
  `tests/b15_default_module_slots_cli.rs`, `Cargo.lock`, `README.md`, `CLAUDE.md`,
  `tests/cases/README.md`, and `docs/reference/dev-method.md`.

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
- **Route notices are not semantic parity.** ADR-0021 and the WU6 oracle compare diagnostics, parse
  failures, and incomplete records. The explicit-file-list route additionally publishes module
  admission notices while the legacy provider route deliberately does not; B15 pins both envelopes.

## Sequencing

1. WU0 blocks semantic cleanup and the move. Its inventory and full-parity command become the gate
   for every later WU.
2. WU0 implementation, independent review, every receipt and leased measurement, and its atomic
   commit complete before any WU1 edit starts. WU1 source/test hygiene, precise lock update, and
   derived documentation then land sequentially as three commits.
3. WU2 and WU3 may prepare their read-only contract/evidence tables in parallel after WU0. In the
   shared worktree their implementation, review, verification, and commit are sequential so every
   gate observes exactly one uncommitted unit. WU2 lands first, then WU3.
4. WU4 starts from the committed post-WU2/post-WU3 HEAD. Its approved symbol/visibility move
   manifest lands as a docs-only seam commit before any source movement; the move is a separate
   commit with a separate
   independent review.
5. Closure runs the normal Rust gates, the named exact full-parity sweep, release official-suite
   ratchet, `cargo audit`, and a zero-diff scoreboard check. Every CPU-heavy command uses
   `cpu-lease`.

The leader alone writes and commits sprint evidence. Preparation agents return WU2 normalization
data and the WU3 operation table read-only; the leader serially appends each frozen seam to this
sprint before its implementation starts. No preparation agent edits the sprint or `.orchestrate/`.

## Frozen implementation seams

### WU2 declaration and canonical-suffix evidence

The post-WU0/WU1 read-only audit froze these five Statement/Declaration pairs:

| Pair | Statement entry | Declaration entry |
|---|---|---|
| selected namespace values | `namespace.rs:5451` | `namespace.rs:5515` |
| ordinary walk | `namespace.rs:5671` | `namespace.rs:5958` |
| header reservation | `namespace.rs:6543` | `namespace.rs:6598` |
| deferred recording | `namespace.rs:6826` | `namespace.rs:7009` |
| type prebind | `bind.rs:2263` | `bind.rs:2403` |

The checker mirror is `namespace_values.rs:2683-2767`: overload prepass `2692-2699`, private
unsupported enum/import gate `2712-2720`, direct declarations `2702-2723`, wrapped declarations
`2724-2747`, and runtime delegation `2748-2762`. Oxc 0.137.0 inherits exactly nine Declaration
variants into Statement: variable, function, class, type alias, interface, enum, module, global,
and import-equals. Export wrappers, ordinary imports, other exports, UMD namespace exports, and
runtime statements are not inherited declarations.

Use this exact prefix-only normalization before and after implementation; do not substitute the
older approximate 309-line audit, which also normalized identifiers and removed headers:

```sh
scratch=$(mktemp -d)
while IFS=: read -r file a b c d label; do
  sed -n "${a},${b}p" "$file" | sed -E 's/(Statement|Declaration)::/__DECL__::/g' > "$scratch/left"
  sed -n "${c},${d}p" "$file" | sed -E 's/(Statement|Declaration)::/__DECL__::/g' > "$scratch/right"
  common=$(diff --unchanged-line-format='%L' --old-line-format='' --new-line-format='' \
    "$scratch/left" "$scratch/right" | wc -l)
  printf '%s\t%s\n' "$label" "$common"
done <<'PAIRS'
crates/typokat-binder/src/binder/namespace.rs:5451:5513:5515:5560:selected-values
crates/typokat-binder/src/binder/namespace.rs:5671:5956:5958:6082:walk
crates/typokat-binder/src/binder/namespace.rs:6543:6596:6598:6638:reserve-header
crates/typokat-binder/src/binder/namespace.rs:6826:7007:7009:7114:deferred
crates/typokat-binder/src/binder/bind.rs:2263:2315:2403:2449:type-prebind
PAIRS
rm -r "$scratch"
```

The frozen result is `9 + 122 + 37 + 89 + 45 = 302` normalized common lines. The matching strict
duplication baseline is:

```sh
scan=$(mktemp -d)
cpu-lease run -n 2 -- npx -y jscpd@5.0.15 \
  crates/typokat-binder/src/binder/namespace.rs \
  crates/typokat-binder/src/binder/bind.rs \
  crates/typokat-check/src/check/checker/namespace_values.rs \
  --min-tokens 50 --min-lines 5 --skip-comments --reporters json --output "$scan"
jq '.statistics.total | {clones,duplicatedLines,duplicatedTokens,lines,percentage,percentageTokens,sources}' \
  "$scan/jscpd-report.json"
rm -r "$scan"
```

It reports 107 clones, 1,139 duplicated lines, 8,163 duplicated tokens, and 23,337 lines
(4.8807% lines / 5.9150% tokens). Nine strict clones are WU2 walker mirrors. The selected-values
wrapper, ordinary walk export context, reserve `explicit`, deferred `exported`, type-prewalk order,
checker overload prepass, private unsupported gate, runtime delegation, and all import/UMD/export
statement policies remain at the Statement layer and in their current order.

`NamespaceTable::classify` has seven suffix collections: `canonical_namespaces`, globals,
deferred modules, source units, deferred children, UMD exports, and export contexts. Each keeps its
library and user key closure explicit (14 typed closures total) while one helper may own only
`clear_local`, rebuilding `base_len()..len()`, `push_local`, and sorting `local_slice_mut`. The
`canonical_namespaces` missing-fragment MAX sentinel is protected. The in-place fragment sort and
merge-row sort remain explicit and outside the helper.

### WU3 semantic-query operation contract

The read-only post-WU1 audit froze this contract before extraction:

| Contract | Assignability (`query/mod.rs:1873`) | Overload compatibility (`:2106`) |
|---|---|---|
| observed roots | source, then target | implementation, then overload |
| publication precheck | `[source, target]`; LIFO observes target first | `[overload, implementation]`; LIFO observes implementation first |
| pre-planner fast paths | completed memo, identity, same-class covariant mismatch | none |
| planner roots | `[source, target]` | `[overload, implementation]` |
| relation attempt | `is_assignable_attempt(source, target)` | `overload_implementation_compatible_attempt(overload, implementation)` |
| `No` shape | detailed `ReasonChain`; fast mismatch is a root leaf | always root leaf `(overload, implementation)` |
| completed memo | assignability only; two-hit failure promotion; suppress on cycle taint | none; do not add an operation kind |
| relation cache | shared cache; promote only decided and clean pending entries | same cache; only nested assignability creates ordinary entries |
| retry | `Needs` expands one; `NeedsBatch` expands every demand, then retries the root | identical |
| planner finish | always finish and advance `next_type_param` | identical |
| planning taint | tainted `Yes` becomes `Exhausted`; tainted `No` stays `No`; no writes commit | identical |
| cycle taint | suppress pending cache and plan writes; suppress completed memo | same cache/plan rule; no completed memo exists |
| exhausted result | never commits | never commits |
| clean commit | projection/evaluator delta, relation cache, next type parameter, then completed memo | projection/evaluator delta, relation cache, next type parameter only |
| final result | exact post-taint `RelationOutcome` | exact post-taint `RelationOutcome` |

Protected surrounding seams are pass-local taint state (`:623-639`), query savepoints
(`:679-715`), relation-cache journaling (`:780-843`), semantic-context invalidation (`:845-870`),
publication traversal (`:3281-3322`), planner demand dispatch (`:2621-2655`), planner finish
(`:2725-2753`), and cycle-tainted commit filtering (`:2433-2449`).

Existing focused witnesses cover overload and identity laziness, publication/poison boundaries,
planning-taint rollback, cycle-tainted cache suppression, completed success/failure admission and
invalidation, detailed assignability reasons, overload `TK2394`, and deferred retry. Before WU3
implementation, a separate test-only spec commit must directly pin the extraction seam still
missing from those aggregate tests: root/observation order, overload root-leaf `No`, repeated
overload calls with zero completed-memo entries, the exact `NeedsBatch` retry branch, and operation
driver outcomes across `Yes`, `No`, `Exhausted`, planning taint, and cycle taint. Do not use
source-text assertions.

## Run log

<!-- Append as work proceeds. Graduate changed rationale to an ADR and future work to backlog. -->

- **2026-08-14 — sprint drafted from audit at `6207565`; no WU started.** Scheduled findings are
  the amplified conformance gate, binder declaration mirrors, canonical suffix construction,
  duplicated semantic-query transaction, and one-root namespace decomposition. Refuted candidates
  and all other oversized roots are explicitly excluded above.
- **2026-08-14 — execution seams frozen at `28eec70`; WU0 remains RED.** Independent decomposition
  review rejected parallel WU2/WU3 implementation in one worktree and assigned all derived command
  docs to one later WU1 commit. The WU0 spec independently derived 567 fixture projects / 683
  sources and 567 shared + 567 cold current executions. No clean behavioral RED can exist before
  the private route-mode/receipt seam: undefined APIs would only break compilation and source-text
  tests would be brittle. The executable spec therefore lands with the implementation as the
  inventory receipt test, comparator negative controls, unchanged default test name, and ignored
  full-parity test defined in WU0. `cargo test --test conformance --no-run` passed in 0.19 s under a
  two-vCPU lease before implementation.
- **2026-08-14 — WU0 verify-first full-parity baseline PASS at `724adf4`.** The unchanged
  `cpu-lease run -n 2 -- cargo test --test conformance conformance -- --exact --nocapture` command
  completed the current combined shared/cold corpus with 1 passed, 0 failed in **1,103.90 s**
  (18 min 23.90 s), within the 45-minute stop budget. No agent wrote to the tree during the run.
  This proves the pre-change parity gate is green and supplies the exact baseline that the new
  ignored full-parity test must preserve.
- **2026-08-15 — WU0 cadence re-gated from measured current routes.** The reviewed shared-only
  implementation passed with receipt 567/683/567/0. Three warm two-vCPU runs took 120.35 s,
  110.62 s, and 111.28 s (median 111.28 s, maximum 120.35 s), a 9.17x reduction from the
  1,103.90-second combined baseline. Temporary route instrumentation, removed after the run, found
  285 shared routes in 0.285 s, 273 private-sparse routes in 104.248 s, and 9 complete-source
  fallbacks in 24.036 s; all preflight work totaled 0.051 s. The measured 135/150-second gate keeps
  bounded headroom without treating private replay as harness overhead.
- **2026-08-15 — first named full-parity run exposed a specification error, not a checker diff.**
  The 1,061.40-second run found five UMD files whose diagnostics, parse errors, and incomplete
  records matched while the explicit cold route additionally returned its documented
  `export-as-namespace` / `export-assignment` project notices. Three independent read-only reviews
  traced the behavior to the pre-WU0 route contract: ADR-0021 and WU6 omit notices, while B15
  explicitly requires notices on `check_project_once` and none on legacy `check_project`. WU0 now
  compares exact semantic output and leaves the route-specific notice envelope under B15.
- **2026-08-15 — WU0 PASS at `1478213`.** Independent final review passed after adding an active
  incomplete-record comparator control. The three focused semantic/UMD/route-envelope witnesses
  passed. The exact ignored sweep then passed all 567 projects and 683 sources with receipt
  567/683/567/567 in 1,075.40 s (wall 1,076.01 s), and the unchanged WU6 pull-request matrix passed
  15/15 in 265.84 s. `cargo fmt --check` and `git diff --check` passed before the atomic test/workflow
  commit. No fixture, production route, existing CI workflow, or WU6 test changed.
- **2026-08-15 — WU1 PASS in three atomic commits.** `f32402e` replaced exactly four stale
  ADR-0015 scheduler comments with their retained ADR-0020 oracle role, corrected the declaration
  library compiler description, and removed only the self-declared obsolete ignored pre-WU6 RED
  test; independent review and the six scheduler plus active B15 route-envelope witnesses passed.
  `001b183` moved only `crossbeam-epoch` 0.9.18 to 0.9.20; the before audit found only
  RUSTSEC-2026-0204, the after audit found zero advisories, and `cargo check --locked --lib` passed.
  `ad784e7` published the frozen inventory authority, exact commands, receipts, measured cadence,
  WU6 matrix, and weekly/manual full-parity workflow across the four owned docs; independent review
  passed after distinguishing the frozen count/order test from receipts that repeat aggregate totals.
- **2026-08-15 — WU2 PASS at `744d5b0`.** Five Statement walkers now delegate inherited
  declarations to their existing Declaration dispatchers, the checker mirror uses one dispatcher
  after its overload/private gates, and one helper rebuilds exactly seven canonical local suffixes.
  Independent review found all nine Oxc variants, wrapper/import/runtime/UMD policy, flags, both
  source orders, 14 typed key closures, MAX sentinel, and explicit fragment/merge sorts preserved.
  The frozen fixed-coordinate LCS output fell from 302 to 46 lines; a symbol-adjusted review scan
  found 58 residual delegation/wrapper lines and no mirrored declaration semantics. Strict jscpd
  fell from 107 clones / 1,139 duplicated lines / 8,163 tokens to 97 / 1,008 / 7,466; no remaining
  clone spans a walker pair or suffix protocol. Binder tests passed 112/112, checker namespace tests
  49/49, targeted clippy was clean, default conformance passed 567/683/567/0 in 111.14 s, and the
  freshly rebuilt release official-suite ratchet reported 0 regressions / 0 progress with the
  scoreboard byte-unchanged.
- **2026-08-15 — WU3 PASS in spec `1bd6602` and implementation `21a23c5`.** The pre-refactor
  characterization pins both operation root/observation orders, overload root-leaf `No` and absent
  completed memo, exact whole-`NeedsBatch` retry order through the real coordinator, `Yes`/`No`/
  `Exhausted`, planning taint, cycle taint, and zero-write rules. Its private `cfg(test)` trace has
  RAII nesting/unwind cleanup and compiles out of production. The implementation leaves exactly one
  two-variant transaction loop while keeping assignability fast paths/memo and overload leaf/no-memo
  policy outside it. Independent review found no cache, taint, root-order, or commit drift. Query
  tests passed 94/94, clippy was clean, default conformance passed 567/683/567/0 in 114.11 s, WU6
  passed 15/15 in 264.53 s, and the freshly rebuilt release official-suite ratchet again reported
  0 regressions / 0 progress with the scoreboard byte-unchanged.
