# Sprint — development feedback loop (2026-08-21)

**Goal.** Deliver backlog 117 and 120 as one bounded tooling sprint: exact marker-aware focused
conformance, transparent same-tree gate deduplication, and one authenticated release binary shared
by pull-request CI consumers.

**Theme.** These changes shorten feedback without changing checker semantics or weakening closure
evidence. The local path selects less work only through an explicit non-closure entry point; the
verification path executes an explicit caller-supplied gate list and deduplicates only proved
subsumptions; CI consumers run the exact binary built by the existing release-build job.

## Refs re-verified at HEAD (2026-08-21, `6761be8`)

`✔` = confirmed live · `⚠` = constraint or nuance caught before implementation.

- ✔ `tests/conformance.rs:629-664` exposes separate ordinary and ignored full-parity entry points,
  both over `discover_enabled_inventory()` and `run_inventory()`.
- ✔ The frozen inventory authority is 580 fixture projects / 696 source files at
  `tests/conformance.rs:666-725`. Focused execution must not alter those counts, the order
  fingerprint, or either closure entry point.
- ✔ Every discovered project already has a stable `logical_id`; project-shaped fixtures carry all
  recursively discovered sources in one `FixtureProject` (`tests/conformance.rs:602-613,834-851`).
- ⚠ Standard Rust libtest consumes arguments after `--`; it does not offer repository-defined
  `--case`/`--corpus` flags. A separate ignored test entry point with exactly one explicit
  environment selector keeps ordinary `conformance` impossible to filter accidentally.
- ✔ Marker and incomplete-marker tests currently replace the process-global panic hook without
  serialization at `tests/conformance.rs:1634-1642` and its incomplete-marker counterpart.
- ✔ `.github/workflows/ci.yml` builds the same release binary in the `build`, `official-suite`, and
  `differential` pull-request jobs. The weekly `differential-tsc` job has a distinct truth-mode
  lifecycle and may retain its own build.
- ✔ Repository tooling already uses Python's standard-library `unittest`; no new dependency is
  required for a bounded verification helper or workflow contract test.
- ⚠ `actionlint` and `yq` are not installed locally. Workflow acceptance therefore needs a
  repository-owned static contract test plus shell/YAML syntax checks; only GitHub can execute the
  final Actions graph.

## Work units

### WU1 — exact focused conformance entry point (effort S)

- **Problem.** The marker-aware edit loop jumps from manual CLI checks to the full 580-project
  inventory. There is no exact case/corpus route that preserves project grouping and marker
  comparison, and malformed-marker tests mutate the global panic hook unsafely.
- **Verify first.** Pin independent acceptance cases for one enabled flat logical id, one enabled
  multi-file project id, one enabled whole corpus, a disabled corpus/case, an unknown selector,
  conflicting selectors, the focused receipt, and serialized panic-hook restoration.
- **Scope.** Add one ignored `conformance_focused` test. It accepts exactly one of
  `TYPOKAT_CONFORMANCE_CASE` or `TYPOKAT_CONFORMANCE_CORPUS`, selects from the canonical inventory,
  rejects disabled/unknown/ambiguous input deterministically, runs `SharedMarkers`, and prints a
  receipt containing `closure=false`. Case means exact logical project id, so a multi-file project
  stays intact. Corpus means one whole enabled `MILESTONE_DIRS` entry; a mixed/disabled corpus is
  rejected rather than silently running its selected subset. Replace unsynchronized panic-hook
  mutation with one serialized RAII guard or structured errors without weakening malformed-marker
  controls. Document exact commands.
- **Acceptance / witness.** Narrow selector and panic-hook tests pass. A flat case executes `1/1`, a
  project case executes `1/N`, and a corpus receipt matches its inventory slice. Disabled and
  unknown selectors fail before checking. The ordinary exact conformance command still prints
  `580/696/580/0`; inventory fingerprint and full-parity entry points are byte-for-byte unchanged
  except for surrounding helper extraction needed by the focused route.
- **Touch points.** `tests/conformance.rs`, `tests/cases/README.md`.

### WU2 — transparent same-tree verifier (effort M)

- **Problem.** Closure command lists can run package suites and later run workspace `cargo test` on
  the same tree. Exact commands and visible receipts must remain explicit, but provably subsumed
  work should not execute twice.
- **Verify first.** Write standard-library Python tests for exact duplicate removal, the narrow
  workspace-`cargo test` subsumption rule, retained filtered/ignored/nocapture/unknown commands,
  requested-versus-resolved printing, argv execution without a shell, command failure propagation,
  dirty-tree identity, and rejection when a command changes tracked or untracked state.
- **Scope.** Add a small `tooling/verify/` CLI. It accepts repeated explicit gate strings, parses
  them with `shlex`, executes argv directly, prints requested and resolved commands, records HEAD
  plus Git-representable tracked/untracked content identity, and checks that identity around every
  gate. It removes exact duplicates and only plain package
  `cargo test -p <workspace-package>` invocations subsumed by a plain unfiltered workspace
  `cargo test`. It never drops ignored, filtered, visible-receipt, example, feature-specific, or
  unknown commands. Include `--dry-run`; no profiles, resume, cache, inferred scope, shell
  pipelines, mutable gate registry, or general filesystem monitor.
- **Acceptance / witness.** The Python suite passes in temporary Git repositories, negative
  controls prove each unsafe command remains, `--dry-run` prints the stable tree identity and exact
  resolved argv, and one local smoke invocation executes a harmless explicit command.
- **Touch points.** New `tooling/verify/verify.py`, `tooling/verify/test_verify.py`, and
  `tooling/verify/README.md`; `.gitignore` for generated Python bytecode.

### WU3 — authenticated release artifact fanout in CI (effort S)

- **Problem.** Pull requests link the release binary independently in three jobs. Cargo caches do
  not share the final artifact, and consumer jobs can therefore spend runner capacity producing
  binaries they are intended only to exercise.
- **Verify first.** Add a workflow contract test that proves the build job uploads exactly one
  binary plus commit/digest metadata; official-suite and differential depend on build, download and
  verify it, make it executable, and contain no release build; the weekly tsc truth job remains
  independently explicit.
- **Scope.** Have `build` generate a SHA-256 manifest and exact commit identity, then upload the
  three-file artifact with short retention. Make `official-suite` and `differential` declare
  `needs: build`, download to `target/release`, verify both identity fields, and `chmod +x` before
  their unchanged harness commands. Remove Rust toolchain/cache setup from those two consumers only
  when no remaining step uses Cargo. Keep `differential-tsc` unchanged.
- **Acceptance / witness.** The static workflow contract test and shell syntax checks pass; the YAML
  parses with the available local parser; no PR consumer other than `build` contains
  `cargo build --release`; binary path and harness commands stay unchanged. Remote GitHub execution
  remains the final integration witness and is reported honestly rather than simulated locally.
- **Touch points.** `.github/workflows/ci.yml`, new `tooling/ci/test_release_artifact_workflow.py`.

### WU4 — independent review and integrated closure (effort M)

- **Problem.** Unit tests can miss accidental closure filtering, unsafe Cargo subsumption, dirty
  tree gaps, or an Actions graph that downloads an unauthenticated/stale artifact.
- **Verify first.** Freeze each WU diff and hand it to a reviewer different from its implementer.
- **Scope.** Review WU1 for inventory/marker fidelity and accidental ordinary-gate filtering; WU2
  for command-injection, subsumption, and tree-identity false negatives; WU3 for artifact trust,
  job dependency, binary path, permissions, and weekly-mode regressions. Route prescribed fixes
  back to the original implementer once, then rerun affected review/gates.
- **Acceptance / witness.** Three independent PASS verdicts; `git diff --check`; Python unit suites;
  focused flat/project/corpus spot runs; ordinary exact conformance receipt `580/696/580/0`;
  `cargo test`; `cargo clippy --all-targets -- -D warnings`; and unchanged manifest validation.
  Full shared/cold parity and randomized differential are not required because no checker route,
  library lifecycle, inference, contextual typing, relation, or diagnostic semantics change.
- **Touch points.** Read-only review of WU1-WU3, then sprint/backlog closure documents.

## Out of scope (explicit)

- No default-library, checker, binder, frontend, relation, inference, or diagnostic changes.
- No per-corpus metadata migration and no change to enabled fixture registration or frozen counts.
- No mutable verification profiles, automatic gate selection from changed paths, result cache,
  resume mechanism, shell execution, or generic Cargo command planner.
- No filesystem traversal or identity guarantee for FIFO, socket, and other state Git cannot
  represent. The same-tree contract covers tracked state plus Git-discoverable untracked regular
  files and symlinks.
- No CI sharding, workflow trigger changes, full-parity workflow changes, deployment, push, or
  remote GitHub run.
- No cleanup of unrelated source-text architecture guards or large test modules.

## Decisions

- The focused route is a separate ignored test selected by environment, not behavior hidden inside
  ordinary `conformance`; this makes accidental closure filtering unrepresentable.
- Focused case identity is the existing exact logical project id. Source paths inside a multi-file
  project are not separate cases.
- The verifier is an explicit command executor with a tiny proved subsumption rule, not a profile
  authority. Visible receipt commands remain even when their test work overlaps a workspace gate.
- Same-tree identity is intentionally bounded to Git-representable state. A filesystem monitor for
  special objects would add traversal and ignore semantics that this explicit gate helper does not
  need.
- CI artifact trust uses both the workflow commit identity and SHA-256 content identity. The weekly
  tsc truth lifecycle remains independent.

## Sequencing

1. Commit this sprint specification and freeze WU territories.
2. Independent spec writers pin WU1-WU3 executable contracts without implementing behavior.
3. WU1-WU3 implementations may run in parallel: their write territories are disjoint.
4. Cross-rotate independent reviewers, integrate prescribed fixes, then serialize CPU-heavy gates.
5. Commit each verified WU separately. Close by deleting backlogs 117/120, archiving this sprint,
   and refreshing the sprint/backlog indexes.

## Run log

- 2026-08-21: sprint opened from backlogs 117 and 120. No checker-semantic fixture corpus is added;
  executable tooling/config contracts are written by agents independent of implementation before
  each WU starts, while this plan is the separately committed acceptance spec.
- 2026-08-21: WU2 review showed that Git does not discover untracked FIFO/socket objects. The user
  chose the bounded Git-representable contract instead of adding an ignore-aware filesystem walk.
