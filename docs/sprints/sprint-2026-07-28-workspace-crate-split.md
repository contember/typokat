# Sprint — workspace crate split (2026-07-28)

**Goal.** Turn the single `typokat` library into a root facade over an enforced,
acyclic workspace without changing checker behavior.

**Theme.** The source tree already follows architecture layers, but one crate
cannot enforce them. This sprint cuts the remaining production tangles, moves
each layer to its owning crate, and preserves all behavior and repository gates.

## Refs re-verified at HEAD (2026-07-28)

- ✔ `check`, `binder`, `types`, `relate`, `diagnostics`, and `library` remain the
  dominant source layers — `src/check/`, `src/binder/`, `src/types/`,
  `src/relate/`, `src/diagnostics/`, `src/library/`.
- ⚠ The measured idea missed a production `check ⇄ driver` cycle:
  `continue_library_project_binder` calls `run_project_frontend`, while the driver
  imports the checker — `src/check/checker/library_compiler.rs:4131`,
  `src/driver.rs:6`.
- ✔ The other production tangles remain narrow: the collision capability points
  from `check` to `library`, and library provider/base code names driver-owned
  inputs — `src/check/checker/library_compiler.rs:607`,
  `src/library/provider.rs:155`, `src/library/base.rs:939`.
- ⚠ `snapshot_codec` has been removed; the neutral core now consists of source
  identity, spans, the workspace test helper, and other dependency-free records —
  `src/source.rs`, `src/span.rs`.
- ⚠ Source-introspecting tests and path manifests now include replay-index,
  library-package, full-lib benchmark, binder, checker, and library paths in
  addition to the two examples in the idea — `src/check/checker/replay_index.rs`,
  `tooling/library-package/verify.py`, `tooling/full-lib-bench/full_lib_bench.py`.
- ✔ The binding architecture and soundness rules remain unchanged; this refactor
  must not alter semantic-query, relation-cache, publication, event, evaluator, or
  CFG behavior — `docs/reference/invariants.md`.

## Work units

### WU0 — acceptance spec and decision (effort S)

- **Problem.** The idea was exploratory and its target graph omitted the
  project-frontend boundary.
- **Verify first.** `cargo metadata --no-deps --format-version 1` reports only the
  root package; `cargo check --lib` is green.
- **Scope.** Accept ADR-0019, add an ignored topology/layering witness, and
  graduate the idea into this sprint.
- **Acceptance / witness.** The witness compiles but remains ignored until the
  workspace exists; this commit changes no production behavior.
- **Touch points.** `docs/decisions/`, `docs/sprints/`, `docs/ideas/`,
  `tests/workspace_layout.rs`.

### WU1 — cut production and test tangles in one crate (effort L)

- **Problem.** `check ⇄ driver`, `check ⇄ library`, and test-only upward edges
  prevent an acyclic package graph.
- **Verify first.** Pin the current `cargo metadata` graph and grep every
  cross-layer `crate::` path, including inline test modules.
- **Scope.** Introduce a frontend-owned project input/program boundary; move
  parsing, import scanning, and dependency ordering out of driver; sink the
  collision capability into check; relocate wrong-layer tests; centralize the
  workspace-root test helper. Keep this stage inside the root crate.
- **Acceptance / witness.** `cargo test` and clippy pass, and an active layering
  tripwire finds no known upward source edge.
- **Touch points.** `src/frontend.rs`, `src/driver.rs`, `src/check/`,
  `src/library/`, affected unit specs and `src/lib.rs`.

### WU2 — split the lower workspace layers (effort L)

- **Problem.** Module visibility still permits dependency violations.
- **Verify first.** Recompute distinct external paths for each layer after WU1.
- **Scope.** Add workspace dependency/lint tables and move, bottom-up,
  `typokat-core`, `typokat-types` (including class semantics), `typokat-binder`,
  `typokat-relate`, `typokat-diagnostics`, and `typokat-surface`. Curate each
  `lib.rs`; move or re-home member tests without adding dev-dependency cycles.
- **Acceptance / witness.** Each member passes `cargo check -p <member>` and its
  relevant tests; every intermediate commit leaves the workspace green.
- **Touch points.** `Cargo.toml`, `Cargo.lock`,
  `crates/typokat-{core,types,binder,relate,diagnostics,surface}/`, root facade
  modules, affected specs.

### WU3 — split frontend and upper workspace layers (effort L)

- **Problem.** The parser/project bridge and upper layers remain in the root
  compilation unit.
- **Verify first.** Confirm no source or test calls from check into driver or
  library after WU1.
- **Scope.** Move `typokat-frontend`, `typokat-check`, `typokat-library`, and
  `typokat-driver`; move the pinned TypeScript assets with library; update every
  introspection/tooling path; leave root `src/lib.rs` as facade and `src/main.rs`
  as the binary.
- **Acceptance / witness.** Enable `tests/workspace_layout.rs`; it reports the ten
  members and an acyclic layer graph. Root integration tests and tooling continue
  to use repository-relative paths.
- **Touch points.** `crates/typokat-{frontend,check,library,driver}/`, `src/`,
  `tests/`, `tooling/`, manifests and lockfile.

### WU4 — adversarial review and closure (effort M)

- **Problem.** Mechanical moves can silently drop tests, path gates, or
  crate-private invariant enforcement.
- **Verify first.** Review the full diff independently from the implementer.
- **Scope.** Hunt missing test targets, cfg-only failures, wrong-direction
  dependencies, vacuous source scans, asset/package omissions, and any semantic
  drift. Cross-check representative clean/error/project/library programs against
  the pre-split binary and `tsc --strict`.
- **Acceptance / witness.** Independent PASS; full tests, clippy, per-member
  checks, package asset verification, and representative CLI output equivalence
  pass.
- **Touch points.** Whole migration diff and evidence.

## Out of scope (explicit)

- Splitting `typokat-check` internally. It is a real state-boundary refactor and
  remains profiling-gated.
- Claiming or optimizing incremental build time.
- Publishing workspace members or stabilizing their public Rust APIs.
- Checker semantics, inference/contextual typing, relation policy, diagnostics,
  and TypeScript coverage.

## Decisions

- [ADR-0019](../decisions/0019-split-the-checker-into-layered-workspace-crates.md)
  owns the corrected ten-member graph and the dedicated frontend layer.
- The root remains a real package and facade.
- `typokat-library` owns the pinned TypeScript assets.
- Public API compatibility is not a migration constraint.

## Sequencing

WU0 commits alone. WU1 must be green before any crate move. WU2 proceeds
bottom-up. WU3 follows the lower-layer public surfaces. WU4 is performed by an
independent reviewer; fixes return to the implementation agent before closure.

## Run log

- 2026-07-28: Re-verification found the proposal's missed production
  `check ⇄ driver` edge. User approved a clean architecture without public-API
  compatibility; the dedicated frontend crate is now the accepted boundary.
- 2026-07-28: WU1 cut every known upward source edge while still in one crate.
  Project inputs, parsing, import resolution, source identity, and dependency
  ordering now belong to `frontend`; the collision capability belongs to
  `check`. Wrong-layer specs moved to their consumers, and an active source scan
  pins the boundary. Gate: 1,227 libtests passed, 13 ignored; every integration
  test and doctest passed; all-target clippy is warning-free.
