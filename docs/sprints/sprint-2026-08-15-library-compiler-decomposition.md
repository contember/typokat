# Sprint — library compiler decomposition (2026-08-15)

**Goal.** Decompose `checker::library_compiler` behind its stable facade without changing behavior,
public paths, library lifecycle semantics, or test coverage.

**Theme.** The post-maintenance audit leaves one dominant source-layout hotspot:
`crates/typokat-check/src/check/checker/library_compiler.rs`. This sprint changes only that root.
It first freezes an item-level move manifest and source-equivalence contract, then performs one
mechanical split, subjects the frozen diff to independent review, and closes the same library-route
gates that protect ADR-0020 and ADR-0021. It does not combine decomposition with duplication
cleanup, semantic refactoring, or another oversized-file split.

Success means a facade of at most 250 physical lines, no production child above 1,500 physical
lines, the same fully qualified reachable item paths and visibility, a complete-token partition of
the original source with only manifest-approved module/source-path adjustments, all 93 inline tests
retained as whole items, exact shared/cold parity, and zero official-suite scoreboard movement.

## Refs re-verified at HEAD (2026-08-21, `b0dbc96`)

`✔` = confirmed live · `⚠` = constraint or nuance caught by the audit.

- ✔ `library_compiler.rs` is **14,009 physical lines**, still the largest tracked Rust source file.
  Its trailing `#[cfg(test)] mod tests` begins at `library_compiler.rs:9782`; the pre-test region is
  9,781 lines and the trailing test module is 4,228 lines with 93 `#[test]` entries.
- ⚠ The old 251-item lexical census is stale after canonical declaration inventory work added new
  test-only sparse-selection ownership and changed compile-route bodies. The 114 declarations
  beginning with `pub`, `pub(crate)`, `pub(super)`, or another restricted `pub(...)` form are
  unchanged. WU0 must regenerate the complete item census rather than translating the old one.
- ✔ `checker/mod.rs:83` exposes `pub mod library_compiler`; the same **25** tracked Rust consumer
  files refer to `library_compiler` paths, including `examples/one_pass_probe.rs`. A facade can preserve those
  paths, but an internal-only module split is not allowed to leak into consumers.
- ✔ The next deferred candidates are materially smaller: `checker/mod.rs` is 9,032 lines,
  `frontend.rs` is 7,331, and `checker/decls/mod.rs` is 6,293. They are not part of this sprint.
- ✔ The completed namespace split provides a current local control for the facade shape:
  `binder/namespace.rs` is 36 lines and its production children are 303–1,313 lines. This is a
  layout precedent only; namespace ownership is not reopened here.
- ⚠ `forbidden_projection_callsite_audit_for_test` at `library_compiler.rs:382` reads the complete
  monolith through `include_str!("library_compiler.rs")` and asserts exact token counts across
  compiler and replay-index source. A split must replace the compiler input with an exhaustive,
  explicit `concat!(include_str!(...))` covering the facade, every production child, and
  `tests.rs`, each exactly once. It must retain the same token counts; omitting the facade or tests,
  duplicating a child, narrowing, or sampling the source set would weaken the oracle.
- ⚠ `run_library_release_probe` at `library_compiler.rs:9615` embeds the release workload at
  `:9714` through a
  path relative to the current file. Moving that body requires one manifest-approved relative-path
  adjustment. The embedded workload and probe body otherwise stay unchanged.
- ⚠ `OwnedLibraryRuntimeState` is declared near `library_compiler.rs:500`, while its large inherent
  impl begins at `:1658`. The proposed size cap favors separate model and operation children, but
  that boundary may require exact private-field `pub(super)` changes. WU0 must either prove each
  sibling access and list it or co-locate the affected items while retaining the 1,500-line cap.
- ⚠ Four external source guards encode the monolith's path and must move with exact reasons:
  `tests/type_tag_exhaustiveness.rs:337` keys the second `TypeTag` match and its enum-call-signature
  rationale; `collision_replay_index_spec.rs:1196-1240` proves the binder checkpoint constructor and
  continuation provenance from checker source; `collision_preflight_spec.rs:837-843` proves the
  sealed capability has exactly one lower-checker owner and issuer; and
  `replay_index.rs:4264,4669` freezes the production replay source set and the two allowed snapshot
  structural-validation accesses. WU0 must name each new destination/path, ordinal/count, and
  unchanged reason. No assertion may be deleted or broadened to make the move pass.
- ⚠ Four live backlog documents name the monolith or an old line directly: backlog 85
  (`terminal_expression_owners`), 94 (an exact source line), 98 (`canonical_library_evidence`), and
  113 (the lifecycle seam). Their path-only references must follow the moved owner; their scope and
  decisions remain unchanged.
- ✔ The proposed body regions have natural ownership boundaries: collision/replay coordination,
  owned runtime model and operations, continuation witnesses, source-profile evidence, canonical
  serialization, terminal validation, collision-plan construction, complete-source admission,
  generic compilation, binder continuation, and release probes.
- ✔ No proposed move reaches inference, contextual typing, argument walking, overload resolution,
  the relation engine, TypeScript fixtures, or diagnostic expectations. The randomized
  differential gate is therefore outside this move-only sprint unless implementation crosses one
  of those boundaries.

## Proposed routing map to freeze in WU0

The ranges below are routing evidence, not implementation coordinates. Attributes, helper types,
and impl blocks move with their owning symbols. WU0 replaces this table with an exact symbol-level
manifest before any production edit.

| Destination | Current body region / ownership | Approximate moved body lines |
|---|---|---:|
| `library_compiler/replay_control.rs` | lines 122–1241, excluding the owned runtime model | at most 1,120 before exact partition |
| `library_compiler/runtime_model.rs` | `OwnedLibraryRuntimeState`, product model, freeze and validation | 478 |
| `library_compiler/runtime_state.rs` | lines 1705–2566, owned-state operations | 862 |
| `library_compiler/runtime_witness.rs` | lines 2567–3374 | 808 |
| `library_compiler/profile.rs` | lines 3375–4121 | 747 |
| `library_compiler/canonical_bytes.rs` | lines 4122–4742 | 621 |
| `library_compiler/terminal_validation.rs` | lines 4743–5441 | 699 |
| `library_compiler/collision_plan.rs` | lines 5442–6084 | 643 |
| `library_compiler/complete_source.rs` | lines 6085–6836 | 752 |
| `library_compiler/compile_api.rs` | lines 6837–7543 | 707 |
| `library_compiler/compile_engine.rs` | lines 7544–8537 | 994 |
| `library_compiler/continuation.rs` | lines 8538–9120 | 583 |
| `library_compiler/probes.rs` | lines 9121–9781 | 661 |
| `library_compiler/tests.rs` | lines 9782–14009 | 4,228; excluded from production cap |

Imports and exact symbol ownership can change these estimates. The frozen manifest must recalculate
every destination with its real imports and reject a production child projected above 1,500 lines.

## Path-sensitive oracle migration contract

WU0 must replace destination placeholders below with the exact frozen child paths and occurrence
ordinals. WU1 may then make only these external-oracle changes:

- `tests/type_tag_exhaustiveness.rs:337-338`: replace the monolith path and `#2` occurrence key with
  the exact child path and child-local occurrence ordinal of `match store.tag(ty)`. Preserve the
  reason verbatim: the test probe returns no call signatures for enum nodes.
- `crates/typokat-library/src/collision_replay_index_spec.rs:1196-1240`: make
  `private_compiler` read or concatenate exactly the child source files that own
  `LibraryBinderCheckpoint::new` and `continue_library_project_binder`. Preserve the assertions that
  there is exactly one checkpoint construction, that continuation takes the opaque checkpoint, and
  that it cannot call `resume_frozen_library`. Do not concatenate unrelated files to dilute those
  provenance claims.
- `crates/typokat-library/src/collision_preflight_spec.rs:837-843`: read the exact child that owns
  `CollisionFreeUserDeltaCapability`; retain counts one for the constructor declaration and one for
  `pub fn issue() -> Self`. The reason remains that the sealed capability has one lower-checker
  owner while `collision_preflight.rs` is its sole issuer.
- `crates/typokat-check/src/check/checker/replay_index.rs:4264`: replace the single monolith entry in
  `EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS` with the facade and every production child exactly once;
  register `library_compiler/tests.rs` once in `TEST_ONLY_REPLAY_SOURCE_PATHS`, because it contains
  only the original trailing `#[cfg(test)]` module. At `:4669`, move the allowance for
  `parts.runtime.class_parents.iter()` to its exact child path while retaining count two and reason
  `snapshot structural validation`.

The internal `library_compiler.rs:383` concat and release-workload path at `:9714` remain separately
authorized seams because their edits land inside the decomposed root rather than these four
external guards.

## Work units

### WU0 — freeze the move manifest and equivalence contract (effort M)

- **Problem.** Physical line ranges are insufficient authority for a public, lifecycle-sensitive
  module with hundreds of pre-test items, restricted visibility, sibling-module references, and source-text
  oracles. Starting the move from ranges alone would make omissions and accidental API widening
  difficult to detect.
- **Verify first.** On the refreshed sprint baseline tree, where the census is revalidated:
  1. lex the complete file and partition its full Rust token stream in source order, including the
     module docs/imports, every item's attached docs/attributes/visibility/signature/body, impl
     items, macros, constants/statics, and every whole test item—not only bodies, names, or asserts;
  2. prove the partition is complete and non-overlapping by matching the normalized concatenation
     and token count/digest back to the original file, with no unowned inter-item token;
  3. enumerate all 25 external and checker-internal consumer files, including
     `examples/one_pass_probe.rs`, and record every imported or qualified path under default,
     `cfg(test)`, and `feature = "test-utils"` reachability;
  4. record each item's exact fully qualified pre-move path, visibility, signature, cfg reachability,
     and whether it is private implementation, restricted-public, or externally reachable;
  5. enumerate all `super::`, `include_str!`, `include_bytes!`, and other file-relative expressions;
  6. freeze the five path-sensitive source-oracle sites at `library_compiler.rs:383`,
     `tests/type_tag_exhaustiveness.rs:337`, `collision_replay_index_spec.rs:1196`,
     `collision_preflight_spec.rs:837-843`, and `replay_index.rs:4264,4669`, including their exact
     token counts/ordinals and reasons;
  7. calculate destination sizes from the complete partition, including imports and facade
     re-exports, with physical `wc -l` receipts; and
  8. record the existing exact counts used by `forbidden_projection_callsite_audit_for_test`; and
  9. enumerate the four live backlog path references that must follow moved symbol owners without
     changing backlog scope.
- **Scope.** Append an item-by-item source-to-destination manifest to this sprint. For every item,
  state its destination, exact post-move fully qualified path, cfg reachability, and whether it
  stays private, is re-exported at the facade, or needs the minimum `pub(super)` visibility for a
  proved sibling. Child modules themselves remain private `mod`, never `pub mod`; facade re-exports
  preserve every pre-move reachable path. List every permitted token adjustment explicitly:
  module declarations, imports, re-exports, qualified module paths whose depth changes, the
  exhaustive source-audit `concat!`, the release-workload relative path, and the four external
  source-guard path/manifest updates with their unchanged reasons. Freeze commands and
  machine-readable full-token receipts so the after-scan uses the same lexer and partition. Count
  physical lines on complete files; do not satisfy caps by hiding production or `test-utils` code
  under `cfg(test)`, moving it into `tests.rs`, generated includes, dense one-line formatting, or
  otherwise gaming LOC. Only the original trailing `#[cfg(test)] mod tests` may populate
  `tests.rs`. Have an independent reviewer approve completeness, destination sizes, exact path and
  reachability preservation, and every source-text seam before implementation. Commit this
  manifest/spec separately.
- **Acceptance / witness.** Exactly one destination exists for every token-bearing source unit and
  attached attribute; the partition digest and counts prove no missing, duplicated, or unowned
  token; all 93 complete test items appear once; all 25 consumers and every pre-move fully qualified
  reachable path/visibility/cfg mode have an exact post-move witness; all five source-oracle sites
  have one approved update with their reasons intact; no production child is projected above 1,500
  honest physical lines; the facade is projected at or below 250; and the independent reviewer
  returns PASS. Implementation does not begin on a partial or unreviewed manifest.
- **Touch points.** This sprint document only. All production Rust remains read-only in WU0.

### WU1 — perform the single-writer move-only split (effort L)

- **Problem.** `library_compiler.rs` mixes fourteen source-layout owners in one 14,009-line file.
  They cannot be extracted safely by multiple concurrent writers because every extraction edits
  the same source/facade seam and shared imports.
- **Verify first.** Re-run WU0's manifest and fingerprints immediately before editing. Confirm the
  committed WU0 manifest still matches HEAD. Run a path-scoped dirty check over
  `library_compiler.rs`, its future child directory, `tests/type_tag_exhaustiveness.rs`,
  `crates/typokat-library/src/collision_replay_index_spec.rs`,
  `crates/typokat-library/src/collision_preflight_spec.rs`, and
  `checker/replay_index.rs`; no overlapping edit may exist. If source, a consumer, or an oracle
  drifted, regenerate and re-review the manifest instead of adapting the move ad hoc.
- **Scope.** Assign the complete Rust move to exactly one implementation subagent. That agent owns
  `library_compiler.rs`, every new `library_compiler/*.rs` file, and the four external source-guard
  files for the duration of WU1; no other agent may write those paths concurrently. Keep
  `library_compiler.rs` as a stable facade containing private module declarations, internal aliases
  needed by descendants, and exact re-exports. Move the complete token partition according to the
  manifest. Preserve every reachable fully qualified path, signature, generic bound, cfg attribute,
  declaration order where observable, and existing restricted visibility. Use only the manifest's
  minimum sibling visibility changes. Replace the monolithic source-text audit with an exhaustive
  concat covering facade, every production child, and `tests.rs` exactly once while preserving each
  frozen token count. Apply only the manifest-authorized path/ordinal/source-set/allowance updates
  in the four external guards and the recorded release-workload path. Do not rename, deduplicate,
  simplify, reformat unrelated items, alter a whole test item, or move production/test-utils code
  into the cap-exempt test file.
- **Acceptance / witness.** WU0's after-scan reconstructs the same complete item/attribute token
  streams after removing only the exact approved seam tokens; it reports one copy of every item,
  all 93 whole test items, and no missing, duplicate, or unowned token. Every pre-move fully
  qualified reachable path, visibility, and cfg mode resolves unchanged through private children
  and facade re-exports. The source-text oracle covers facade plus every child including `tests.rs`
  exactly once and retains its frozen counts. Every external source guard retains its assertion,
  ordinal/count, and reason at the manifest-authorized new path. The facade is at most 250 honest
  physical lines; every production child is at most 1,500. `cargo fmt --check`,
  `cpu-lease run -n 2 -- cargo check --workspace --all-targets --all-features`,
  `cpu-lease run -n 2 -- cargo test -p typokat-check`,
  `cpu-lease run -n 2 -- cargo test -p typokat-library`,
  `cpu-lease run -n 2 -- cargo test -p typokat-driver`, and
  `cpu-lease run -n 2 -- cargo test --no-run` pass through the leader's serialized Cargo queue
  before review. The leader freezes this exact uncommitted diff for WU2; the implementation commit
  waits for the independent PASS.
- **Touch points.** `crates/typokat-check/src/check/checker/library_compiler.rs`, new files under
  `crates/typokat-check/src/check/checker/library_compiler/`,
  `tests/type_tag_exhaustiveness.rs`,
  `crates/typokat-library/src/collision_replay_index_spec.rs`,
  `crates/typokat-library/src/collision_preflight_spec.rs`, and
  `crates/typokat-check/src/check/checker/replay_index.rs`, plus path-only updates in backlogs 85,
  94, 98, and 113.

### WU2 — independently review bodies, visibility, and lifecycle seams (effort M)

- **Problem.** Compilation proves that the new module graph is legal, not that a body, cfg gate,
  source-text oracle, public path, or lifecycle-sensitive ordering was preserved.
- **Verify first.** Freeze the exact WU1 implementation diff. Reviewers consume that diff and the
  committed WU0 manifest without implementation context. They do not edit code or run concurrent
  Cargo jobs.
- **Scope.** Dispatch independent read-only reviews over disjoint evidence territories:
  1. replay control, runtime model/state, and runtime witnesses;
  2. profile evidence, canonical serialization, terminal validation, and collision planning; and
  3. complete-source admission, compile routes/engine, continuation, probes, facade, and tests.
  Each reviewer compares complete item/attribute token streams, exact fully qualified paths,
  visibility, cfg reachability, ordering, imports, and source-relative expressions against pre-move
  HEAD. Review of the third territory includes the four external source guards and all 25 consumers.
  A final independent reviewer reconciles all packets, proves the complete-token partition and
  honest physical-size receipts, rechecks exact path/reachability and exhaustive source-audit
  receipts, and returns PASS or FAIL.
  On FAIL, route prescribed fixes back to the sole WU1 implementation agent, then repeat the
  affected independent review. Review agents remain read-only so the shared worktree has one
  writer and one integration authority.
- **Acceptance / witness.** Every source token and manifest item is independently accounted for;
  no whole item, attribute, or whole test item changed beyond the exact approved seam tokens; no
  visibility widened beyond a proved sibling need; private children remain unreachable as public
  modules; all 25 pre-move consumers resolve their exact paths without edits; every external oracle
  retains its path-sensitive reason/count/ordinal; source-audit coverage includes facade plus every
  child exactly once; and the final independent verdict is PASS with no unresolved finding.
- **Touch points.** Read-only review of WU1 paths and their existing consumers. Any correction is
  made only by the WU1 implementation agent in the WU1 paths.

### WU3 — run serialized closure gates and publish receipts (effort L)

- **Problem.** The module owns default-library compilation, collision replay, complete-source
  checking, and continuation. A move-only diff still requires the full lifecycle gates; narrow
  compile success cannot expose route or ordering drift.
- **Verify first.** Confirm the reviewed implementation commit is the checked-out tree,
  `tooling/official-suite/scoreboard.txt` has no diff, no TypeScript fixture changed, and the WU0
  after-scan still passes. Reserve CPU before every CPU-heavy command. One leader owns and
  serializes all Cargo invocations.
- **Scope.** Run, in order:
  1. `cargo fmt --check` and `git diff --check`;
  2. `cpu-lease run -n 2 -- cargo check --workspace --all-targets --all-features`;
  3. `cpu-lease run -n 2 -- cargo test -p typokat-check`, then
     `cpu-lease run -n 2 -- cargo test -p typokat-library`, then
     `cpu-lease run -n 2 -- cargo test -p typokat-driver`;
  4. `cpu-lease run -n 2 -- cargo clippy --all-targets -- -D warnings`;
  5. `cpu-lease run -n 2 -- cargo test`;
  6. `cpu-lease run -n 2 -- cargo test --test conformance conformance -- --exact --nocapture`,
     expecting receipt `580/696/580/0`;
  7. `cpu-lease run -n 2 -- cargo test --test wu6_complete_source_cli`, expecting 15/15;
  8. `cpu-lease run -n 2 -- cargo test --test conformance \
     conformance_full_shared_cold_parity -- --ignored --exact --nocapture`, expecting receipt
     `580/696/580/580`;
  9. `cpu-lease run -n 2 -- cargo test --test manifest`, expecting 11/11;
  10. `cpu-lease run -n 2 -- cargo build --release`; and
  11. `(cd tooling/official-suite && \
      cpu-lease run -n 2 -- python3 tsofficial.py run --check)`, using that current release binary
      and the complete pinned corpus, expecting 874 tests / 379 in scope, followed by
      `git diff --exit-code -- tooling/official-suite/scoreboard.txt` from the repository root.
  Record command, exit status, test/receipt counts, leased wall time for the long gates, scoreboard
  state, and final file sizes in the run log. Update only moved-symbol path references because
  behavior and architecture remain unchanged. If implementation crossed inference/contextual
  typing despite scope, stop and rescope before claiming closure; do not silently add a differential
  run to bless an expanded change.
- **Acceptance / witness.** Every command passes, both conformance receipts are exact, WU6 is 15/15,
  the official ratchet reports zero regressions and zero scoreboard movement, no semantic fixture
  or API consumer changed, all external source-guard changes match their WU0 authorization, and the
  final complete-token/size/path/reachability receipts match WU0. The sprint can then be stamped
  with its closure record and move to `docs/archive/` with the normal index updates.
- **Touch points.** This sprint's run log and normal close/archive navigation files only. Production
  changes remain limited to WU1 paths.

## Out of scope (explicit)

- Any behavior, performance, allocation, replay-policy, collision-plan, compilation-order, cache,
  diagnostic, incomplete-surface, or route change. This is source layout only.
- Any duplication cleanup, including the `src/main.rs` work shipped separately in the archived
  [`CLI/frontend ownership sprint`](../archive/sprint-2026-08-21-cli-frontend-owned-seams.md).
  Duplication and decomposition require different evidence and rollback boundaries.
- Decomposition or cleanup of `checker/mod.rs`, `checker/decls/mod.rs`, `frontend.rs`,
  `binder/bind.rs`, `checker/calls.rs`, `library/base.rs`, `checker/replay_index.rs`, or any other
  oversized root.
- Deduplication of compact/exact collision replay construction. ADR-0020 deliberately keeps the
  exact full-index path as an independent oracle.
- Moving items between crates, changing the crate graph, introducing a new public submodule API,
  renaming public items, or changing serialized/canonical bytes.
- Removing apparently unused test utilities, probes, counters, cfg gates, ignored tests, or public
  exports. Liveness decisions are not part of a move-only sprint.
- Changes to TypeScript fixtures, marker expectations, divergence records, official-suite corpus or
  scoreboard, differential repros, packaged library sources, benchmark workloads, dependencies,
  or CI cadence.
- Inference, contextual typing, argument walking, overload resolution, relation/query semantics,
  checker-scaling backlogs, and the bytecode VM proposal.
- General import cleanup, naming cleanup, comment rewriting, rustfmt churn outside touched module
  seams, or opportunistic visibility tightening.

## Decisions

- **One root only.** `library_compiler.rs` has the largest pre-test and physical footprint and a
  facade-compatible public module boundary. All other hotspots wait for a later measurement.
- **The manifest is the spec.** A mechanical move needs complete item-and-attribute token streams,
  exact fully qualified paths, visibility, cfg reachability, whole tests, and source-text
  equivalence rather than new TypeScript behavior fixtures. It lands before code.
- **One implementation writer.** The shared source/facade seam makes concurrent extraction unsafe.
  One subagent owns all WU1 writes; independent subagents parallelize only read-only review after
  the diff is frozen.
- **Stable facade, private children.** Existing `checker::library_compiler::*` fully qualified paths
  remain the authority under every current cfg mode. Child modules use private `mod`; they are
  implementation details and do not become reachable public API.
- **Minimum visibility only.** A private item may become `pub(super)` solely when the manifest proves
  a sibling consumer. No item widens to unrestricted `pub` as a convenience.
- **Source-text oracles stay exhaustive.** The current monolith include becomes an explicit concat
  over the facade, every production child, and `tests.rs`, each exactly once, with unchanged exact
  token counts. Sampling or excluding test text is not equivalent.
- **Full parity is mandatory.** Library lifecycle files move, so the exact shared/cold sweep is a
  closure gate even though behavior is intended to remain unchanged.
- **No ADR is expected.** Stop and write or supersede an ADR if the split requires a new semantic
  authority, lifecycle contract, crate boundary, canonical format, or public API decision.

## Sequencing

1. WU0 completes, receives independent PASS, and lands as its own manifest/spec commit.
2. One implementation subagent performs WU1. No other writer touches the facade, source, or child
   paths until the implementation diff is frozen and the narrow gates pass.
3. WU2 read-only reviewers may run in parallel over their disjoint evidence territories. The final
   reviewer integrates findings; only the original implementation agent writes any prescribed fix.
4. The leader commits the reviewed move, then runs WU3's CPU-heavy gates serially under
   `cpu-lease` and records receipts.
5. Closure documentation and archive/index updates happen only after every receipt is green.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
