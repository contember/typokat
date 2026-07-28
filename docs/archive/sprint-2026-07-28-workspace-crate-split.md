> **OUTCOME — shipped 2026-07-28.** The monolithic checker is now ten enforced,
> acyclic workspace members behind the root facade and CLI: core, types, binder,
> relate, diagnostics, surface, frontend, check, library, and driver. Ownership
> follows the architecture, with parsing/import ordering in frontend and
> orchestration/reporting in driver; checker behavior and CLI output remain exact.
>
> **Commit map:** topology `d058383`; pre-split frontend boundary `7f13bc5`; core
> `0c88b25`; types `bd49c56`; binder `daf73d4`; relate `ccc1ebe`; diagnostics
> `109bc27`; surface `dbecdce`; frontend `c342bf6`; check `a27b518`; library
> `2403066`; driver/root facade `21e1f1e`; documentation migration and final
> re-review `11a0da4`.
>
> **Verification:** root facade/CLI plus all ten members passed the bare and
> explicit-workspace gates at 1,290 passed / 14 ignored; the 874-case official
> suite reported zero regressions; the exact 106-file library package, including
> all 82 declarations, verified offline in two clean clones with no source
> mutations; CLI behavior remained exact; the final documentation re-review
> passed.
>
> **Deferrals:** any finer internal split of `typokat-check` remains
> profiling-gated. Publishing the members and public-API stability are out of
> scope.

# Sprint — workspace crate split (2026-07-28)

**Goal.** Turn the single `typokat` library into a root facade over an enforced,
acyclic workspace without changing checker behavior.

**Theme.** The source tree already follows architecture layers, but one crate
cannot enforce them. This sprint cuts the remaining production tangles, moves
each layer to its owning crate, and preserves all behavior and repository gates.

## Refs re-verified at HEAD (2026-07-28)

- ✔ `check`, `binder`, `types`, `relate`, `diagnostics`, and `library` remain the
  dominant source layers — `crates/typokat-check/src/check/`,
  `crates/typokat-binder/src/binder/`, `crates/typokat-types/src/types/`,
  `crates/typokat-relate/src/relate/`, `crates/typokat-diagnostics/src/diagnostics/`,
  `crates/typokat-library/src/`.
- ⚠ The measured idea missed a production `check ⇄ driver` cycle:
  `continue_library_project_binder` calls `run_project_frontend`, while the driver
  imports the checker — `crates/typokat-check/src/check/checker/library_compiler.rs:4131`,
  `crates/typokat-driver/src/driver.rs:6`.
- ✔ The other production tangles remain narrow: the collision capability points
  from `check` to `library`, and library provider/base code names driver-owned
  inputs — `crates/typokat-check/src/check/checker/library_compiler.rs:607`,
  `crates/typokat-library/src/provider.rs:155`, `crates/typokat-library/src/base.rs:939`.
- ⚠ `snapshot_codec` has been removed; the neutral core now consists of source
  identity, spans, the workspace test helper, and other dependency-free records —
  `crates/typokat-core/src/source.rs`, `crates/typokat-core/src/span.rs`.
- ⚠ Source-introspecting tests and path manifests now include replay-index,
  library-package, full-lib benchmark, binder, checker, and library paths in
  addition to the two examples in the idea — `crates/typokat-check/src/check/checker/replay_index.rs`,
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
- **Touch points.** `crates/typokat-frontend/src/frontend.rs`,
  `crates/typokat-driver/src/driver.rs`, `crates/typokat-check/src/check/`,
  `crates/typokat-library/src/`, affected unit specs and `src/lib.rs`.

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
- 2026-07-28: WU2 started bottom-up. `typokat-core` now owns source identity,
  spans, and feature-gated repository test support; its sole normal dependency is
  `oxc_span`. The root remains a real package and aliases the moved modules while
  the other layers are still local. Workspace gate: root 1,218 passed / 13
  ignored, core 9 passed, all integrations and doctests passed, clippy clean.
- 2026-07-28: `typokat-types` now owns the type store, interner, substitution
  engine, and shared class semantics. Its normal dependencies are limited to
  `dragonbox_ecma`, `rustc-hash`, and `smallvec`; test-only instrumentation is
  feature-gated for root invariant specs. Workspace gate: root 1,110 passed / 12
  ignored, types 108 passed / 1 ignored, core 9 passed, all integrations and
  doctests passed, clippy clean.
- 2026-07-28: `typokat-binder` now owns declaration binding, scopes, symbols,
  namespaces, reference records, and binder-owned scaling specs. Its normal
  dependency set is exactly `core`, `types`, Oxc AST/visitor/span, and
  `rustc-hash`; higher-layer test introspection is feature-gated. Workspace gate:
  1,287 tests passed / 16 ignored across unit, integration, and doc targets;
  all-target clippy is clean.
- 2026-07-28: `typokat-relate` now owns structural relations, their transactional
  cache, cycle guards, reason chains, and relation-owned tests. It depends only
  on `types` and `rustc-hash`; higher-layer cache and source-cold measurements
  are feature-gated without enabling relation test modules. Workspace gate stays
  at 1,287 passed / 16 ignored, with all-target clippy clean.
- 2026-07-28: `typokat-diagnostics` now owns structured diagnostics, incomplete
  outcomes, type/reason rendering, and terminal writers. It depends only on
  `core`, `types`, `binder`, `relate`, `codespan-reporting`, and `rustc-hash`;
  the root no longer names the renderer dependency directly. Its 48 owned tests
  moved intact, and the workspace gate remains 1,287 passed / 16 ignored with
  clean all-target clippy.
- 2026-07-28: WU2 closed with `typokat-surface` owning the exhaustive Oxc AST
  classifiers. Its sole dependency is `oxc_ast`; the six root inventory tests
  continue to pin classifier completeness, and active inventory/census paths now
  name the member owner. The full gate remains 1,287 passed / 16 ignored with
  clean all-target clippy.
- 2026-07-28: WU3 started with `typokat-frontend` owning source inputs, parser
  arenas, project programs, import resolution, and dependency ordering. Its
  seven normal dependencies are limited to `core`, `types`, `binder`, and the
  Oxc allocator/AST/parser/span stack; it has no semantic-checker or reporting
  edge. The full gate remains 1,287 passed / 16 ignored with clean clippy.
- 2026-07-28: `typokat-check` now owns all 105 checker/inference/flow Rust files
  and the byte-identical minimal production prelude. The crate depends only on
  lower workspace layers plus its direct Oxc/hash utilities; no library or
  driver edge exists. All 816 original checker test attributes moved intact,
  and a new replay-scanner regression pins exclusion of feature-gated support
  code from the production manifest. Workspace gate: 1,288 passed / 16 ignored,
  unchanged replay/profile digests, clean all-target clippy.
- 2026-07-28: `typokat-library` now owns the default-library compiler, frozen
  base, collision routing support, and the complete pinned TypeScript 6.0.3
  profile. Its normal dependencies are exactly `core`, `binder`, `frontend`,
  `check`, the Oxc parser trio, and `sha2`; test-only lower-layer
  instrumentation remains in dev dependencies. The package gate now verifies
  the exact 106-file member archive in two clean clones and checks each
  extracted crate offline through explicit local patches. Gate: all 82
  declarations and notices byte-verified, 60 member tests passed / 5 ignored
  plus the compile-fail doctest, workspace 1,288 passed / 16 ignored, zero
  build scripts or source mutations, clean all-target clippy.
- 2026-07-28: WU3 closed with the byte-identical 2,203-line driver and all 27
  driver tests owned by `typokat-driver`. Its normal dependencies are exactly
  `check`, `diagnostics`, `frontend`, `library`, `types`, and `rayon`; lower-layer
  test instrumentation remains dev-only. The root now contains only its facade
  and CLI sources, while explicit default members make bare Cargo commands cover
  the root plus all ten members. The enabled metadata witness pins the exact 11
  workspace/default package identities, exact normal graph, downward normal/dev
  edges, zero internal build edges, and an independent acyclic traversal. Both
  bare and explicit-workspace test gates pass at 1,290 passed / 14 ignored;
  all-target all-feature check and clippy are clean.
- 2026-07-28: WU4 documentation review found 217 exact stale references across
  41 living files (31 backlog, 2 reference, 1 idea, 5 active sprints, and 2 test
  docs). Review also repaired pre-existing module-shape or retired-artifact
  references in 8 additional backlog files; README's driver description was
  separately corrected from parse/check to frontend/check ownership, for 50
  changed documents total. The fix migrated all stale references and relabelled
  retired snapshot/artifact modules instead of inventing current files. Parsing,
  import resolution, and dependency ordering remain with `frontend`; driver
  references describe orchestration/reporting. Review evidence: the exact stale
  path and singular-module scans are empty apart from the intentional README
  tree entries; all Markdown links and every repository path introduced on
  changed lines resolve; `docs/decisions/` and `docs/archive/` have zero diff.
- 2026-07-28: Final WU4 independent documentation re-review at `11a0da4` is
  **PASS**: the exact stale-owner scan is empty, all moved references resolve,
  and no immutable history or source file changed.
