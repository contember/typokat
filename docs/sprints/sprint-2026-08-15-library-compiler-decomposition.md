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
- ✔ WU0's exact AST census finds 334 pre-test top-level items: 59 imports and 275 declarations,
  impls, or macros. The 114 declarations beginning with `pub`, `pub(crate)`, `pub(super)`, or
  another restricted `pub(...)` form are unchanged.
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

- **2026-08-21 — prerequisite refresh (`e3789a6`).** Revalidated the plan against production
  baseline `b0dbc96`, updated the 14,009-line / 93-test census, current route ranges, conformance
  receipts `580/696`, manifest 11/11, and official-suite 874/379 authority. No production file
  changed.
- **2026-08-21 — WU0 baseline.** Parent commit `e3789a6c3e70bb4a844708828f9c40de58664408`,
  tree `93b941c36cee24d416456cbdd61dec7c5d7d8827`, Rust 1.95.0, profile
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`, official corpus
  `050880ce59e30b356b686bd3144efe24f875ebc8` / 874 tests / 379 in scope. The source, consumer,
  route, visibility, test, and oracle receipts below are the WU1 authority.
- **2026-08-21 — WU0 independent review PASS.** A read-only reviewer reproduced the source,
  generator, complete 335-item and 93-test manifests, destination/action counts, visibility
  whitelist, 25-consumer census, and path-sensitive oracle migration contract. No HIGH or MEDIUM
  finding remains. Compilation and migrated-oracle execution intentionally remain WU1/WU2 gates.
- **2026-08-21 — WU1 split and narrow gates PASS.** One writer moved the frozen partition into a
  176-line facade, thirteen production children of 493–1,107 lines, and a 4,224-line test child.
  The reconstructed receipts remain 73,657 normalized tokens / `134b61cd…ceaf92` and 120 test
  items / 93 tests / 21,272 tokens / `396db127…a4b6`. Workspace all-target/all-feature check,
  `typokat-check` 1,110/0/6, `typokat-library` 109/0/5 plus eight doctests, `typokat-driver` 56/0,
  and all-target test compilation passed under serialized two-vCPU leases.
- **2026-08-21 — WU1 manifest corrections.** Compilation exposed the minimum sibling visibility
  omitted by WU0: two replay-control thread locals, six `CANONICAL_FRONTEND_*` thread locals,
  `USER_DELTA_FORKS`, and `PrivateReplayScaleControlForTest` plus its `trace` field. The moved
  source audit also requires `include_str!("../replay_index.rs")`. Cfg-split facade aliases and
  globs keep default, test, and `test-utils` builds warning-clean without widening child modules.
  No module-level formatter skip remains; two production item-level skips preserve frozen tokens,
  while the eight test-item skips already present in the monolith remain unchanged.
- **2026-08-21 — WU2 independent review PASS.** Three disjoint read-only reviews and a fourth
  reconciler accounted for all 335 top-level items, 120 nested test items, 25 consumers, visibility
  and cfg seams, source-audit counts, external oracles, and path-only backlog edits. An initial
  MEDIUM finding against module-level formatter skips was fixed and re-reviewed. Final frozen
  implementation digest `c2eb63b91007d11911ffcca1105a1f592b8f4b07903e962b1beb108561fcf3d9`
  has no unresolved HIGH, MEDIUM, or LOW finding. Post-review format, diff, and workspace check pass.

## WU0 frozen move manifest

### Source and parser receipts

The audit parsed the unchanged monolith with `syn 2.0.119` and `proc-macro2` span locations. Each
top-level AST item owns the raw bytes from the preceding item's end through its own end; the final
item also owns the trailing suffix. This partitions all 546,133 bytes without overlap. Normalized
tokens are `quote::ToTokens` output with whitespace collapsed to one ASCII space. WU1 must run the
same parser immediately before editing and compare these receipts:

| Receipt | Frozen value |
|---|---|
| Source bytes | `546133` |
| Source SHA-256 | `7de437ab3bdeb4d35fcf929b8f87dc64a810bed381b89dae53a43b5536ebcc53` |
| Top-level AST items | `335` = 59 imports + 275 declarations/impls/macros + one tests module |
| Normalized token count | `73657` |
| Normalized token SHA-256 | `134b61cd429a39d443429871b27893dc35b576ae8e57bd6e9e41b47f0cceaf92` |
| Tests-module AST items | `120` |
| Complete `#[test]` functions | `93` |
| Tests normalized token count | `21272` |
| Tests normalized SHA-256 | `396db1271f9fd649880e34427bff791f12bf62ef8724232cedcd93ad1885a4b6` |
| Generator `Cargo.toml` SHA-256 | `73c492cf75b01b4840b5564da17e644bdb3bb699cdae47769c451abc0c1a1f49` |
| Generator `Cargo.lock` SHA-256 | `1a305808e038aa073bb0c5942015e8b7a7ce2ceb6786e3172d6f2081953852b6` |
| Generator `src/main.rs` SHA-256 | `46b26855cb242c4925dc0b6846567d58750c46fb3601f6ebc9b3e3da73d3e218` |
| Complete TSV SHA-256 | `93dd0e4c3c8213280f55e6bd9ead06765ece7e1ab3e0a8e3bf5ce81a6b68c18e` |

The frozen command was:

```sh
cpu-lease run -n 2 -- cargo run --locked --offline --quiet -- \
  "$REPOSITORY/crates/typokat-check/src/check/checker/library_compiler.rs" > manifest.tsv
```

The generator source and complete TSV rows are embedded below. WU1's after-scan parses the facade
and every child separately, drops only new module/import/re-export seam items, restores only the
approved visibility and relative-path token substitutions listed below, orders rows by frozen
ordinal, and requires the same per-item token hashes, aggregate token count/digest, test-item
count/digest, and 93 individual test hashes. Any other token difference fails before compilation.

### Destination and size contract

All current imports and the module doc stay in the facade. Each child starts with `use super::*;`.
Private child modules remain implementation details. A child that owns an existing exported item
is glob-re-exported by the facade; effective Rust visibility still caps each item at its original
visibility. Children with no exported item use a private glob import only when another child needs
one of their minimum `pub(super)` items.

| Destination | Frozen ordinals | Items | Moved span lines | Projected complete lines |
|---|---:|---:|---:|---:|
| Facade | 0–58 plus new seam items | 59 | 120 | at most 250 |
| `replay_control.rs` | 59–84, 86–140 | 81 | 1,011 | 1,012 plus approved path edits |
| `runtime_model.rs` | 85, 141–145 | 6 | 486 | 487 plus visibility edits |
| `runtime_state.rs` | 146–148 | 3 | 857 | 858 plus visibility edits |
| `runtime_witness.rs` | 149–187 | 39 | 770 | 771 plus visibility edits |
| `profile.rs` | 188–233 | 46 | 701 | 702 plus visibility edits |
| `canonical_bytes.rs` | 234–248 | 15 | 607 | 608 plus visibility edits |
| `terminal_validation.rs` | 249–261 | 13 | 686 | 687 plus visibility edits |
| `collision_plan.rs` | 262–267 | 6 | 636 | 637 plus visibility edits |
| `complete_source.rs` | 268–278 | 11 | 737 | 738 plus visibility edits |
| `compile_api.rs` | 279–300 | 22 | 690 | 691 plus visibility edits |
| `compile_engine.rs` | 301 | 1 | 993 | 994 plus visibility edits |
| `continuation.rs` | 302–314 | 13 | 570 | 571 plus visibility edits |
| `probes.rs` | 315–333 | 19 | 642 | 643 plus path edits |
| `tests.rs` | body of 334 | 120 nested items | 4,226 body lines | cap-exempt |

Definitions stay in source order within each destination. The only non-contiguous extraction is
`runtime_model.rs`; its six exact owners are `OwnedLibraryRuntimeState`,
`OwnedLibraryRuntimeProductParts`, `CompiledLibraryRuntimeProduct`,
`freeze_library_runtime_product`, `validate_library_source_prefix`, and
`validate_owned_library_product_parts`.

### Facade and path-preservation contract

- Add private `mod` declarations for the thirteen production children and `#[cfg(test)] mod tests;`.
- Keep all 59 existing imports in the facade. Add only the exact checker-parent aliases required by
  preserved `super::...` tokens in moved bodies: `AuthoritativeProjectBinderFinish`,
  `BoundProjectBinder`, `BoundUserBase`, `CheckResult`, `FrozenCheckerRuntimeSnapshotParts`,
  `ModuleDeclarationSpans`, `Pass`, `ProjectModuleAdmission`, `ProjectSourceBindingRow`,
  `UserReportingOwner`, `ValidatedDefaultModulePlan`,
  `bind_authoritative_project_core_with_module_admission`,
  `bind_library_checkpoint_project_programs`, `build_validated_default_module_plan`,
  `build_validated_source_reexport_plan`, `check_project_programs_with_owned_library`, `context`,
  `decls`, `emit_test_incomplete`, `events_library`, `library_identities`,
  `record_continuation_project_binding_consumed_for_test`, `replay_index`,
  `seed_module_placeholder_errors`, and `type_groups`.
- Re-export every existing `pub`, `pub(in crate::check::checker)`, and `pub(super)` item through the
  facade under the same cfg. Use one facade glob per owning child; do not name or expose a child
  module publicly. The per-item rows below are the exact 114-item export inventory.
- `record_collision_manifest_bytes` changes at its definition from old-module `pub(super)` to
  `pub(in crate::check::checker)` so the facade can restore its original checker-visible
  `library_compiler::record_collision_manifest_bytes` path. No other existing exported item's
  definition visibility changes.
- Private item/member visibility may change only where the minimum sibling-use manifest below names
  the exact consumer. No blanket `pub(super)` conversion is allowed.

### Approved source-relative and oracle edits

1. In `replay_control.rs`, replace the monolith audit input with one explicit `concat!` containing
   `../library_compiler.rs`, all thirteen production children, and `tests.rs`, each exactly once.
   Preserve exact counts 2 / 4 / 4 / 4 / 6 / 3 / 2 for the seven compiler tokens and 1 for the
   replay-index token.
2. In `probes.rs`, change only the release workload include from
   `../../../../../tooling/...` to `../../../../../../tooling/...`.
3. In `tests/type_tag_exhaustiveness.rs`, replace
   `library_compiler.rs#2:match store.tag(ty)` with
   `library_compiler/probes.rs#1:match store.tag(ty)`; preserve the existing reason verbatim.
4. In `collision_replay_index_spec.rs`, read only `library_compiler/continuation.rs` for both the
   one `LibraryBinderCheckpoint::new(` construction and the continuation body. Preserve its opaque
   checkpoint and no-`resume_frozen_library` assertions.
5. In `collision_preflight_spec.rs`, read only `library_compiler/replay_control.rs`; retain one
   `CollisionFreeUserDeltaCapability(())`, one `pub fn issue() -> Self`, and sole library-layer
   issuer `collision_preflight.rs`.
6. In `replay_index.rs`, replace the monolith production source entry with the facade and all
   thirteen production children exactly once; add `tests.rs` exactly once to the test-only source
   list. Move the two-count `parts.runtime.class_parents.iter()` allowance to `runtime_model.rs`
   with reason `snapshot structural validation` unchanged.
7. Update only owner paths in backlogs 85 (`terminal_validation.rs`), 94 (`collision_plan.rs`), 98
   (`canonical_bytes.rs`), and 113 (the decomposed checker children). Do not change their scope,
   measurements, dependencies, or decisions.

Frozen source identities for the five code/oracle files are respectively
`7de437ab3bdeb4d35fcf929b8f87dc64a810bed381b89dae53a43b5536ebcc53`,
`754fe058fe42fbfa8a61484a35379b842992aec79cd4a53fed26e1ccdd409c40`,
`290286d525a6b5b5103268dddf33f6a45dfdeffbf9cf4fe57575cc27f26a75f2`,
`89c6a6fd59e3853e2b467a34726c29442ad3740081438a6e1f12022b32ecc38e`, and
`d8990dd8511d75fef4720c8df1522bfa2c2c98c07af9608b7572dfe188b32a5b`.
The sorted 25-consumer list SHA-256 is
`d92cfac07b90e4a98116087d9933622ae2c45f4b239316351c89f64b165f3f72`.

### Minimum sibling visibility changes

Only the entries below may widen from inherited private visibility to `pub(super)`. Line evidence
refers to the frozen monolith. Existing public/restricted items keep their visibility except the
separately authorized `record_collision_manifest_bytes` definition adjustment above.

| Owner | Private top-level item | Consuming children |
|---|---|---|
| `replay_control` | both cfg definitions of `record_private_replay_scheduler_work_for_test` | `runtime_state` |
| `replay_control` | `CollisionPlanForbiddenWork` | `canonical_bytes`, `terminal_validation`, `collision_plan`, `compile_engine` |
| `replay_control` | `IndependentEventOwnerSiteOracle` | `profile` |
| `replay_control` | `ForcedCollisionPlanFailureScope` | `compile_api` |
| `replay_control` | `CollisionPlanForbiddenWorkScope` | `compile_engine` |
| `replay_control` | `record_collision_plan_forbidden_work` | `canonical_bytes`, `terminal_validation`, `collision_plan`, `compile_engine` |
| `replay_control` | `snapshot_namespace_terminals_for_replay` | `compile_engine` |
| `replay_control` | `ExplicitSparseSelectionScopeForTest` | `tests` |
| `replay_control` | `with_private_replay_scale_control_for_test` | `runtime_state` |
| `replay_control` | `record_private_replay_scale_hook_for_test` | `runtime_state` |
| `runtime_model` | `validate_owned_library_product_parts` | `runtime_state` |
| `runtime_witness` | `store_prefix_digest` | `continuation`, `tests` |
| `runtime_witness` | `FinalIdentityInspection` | `continuation` |
| `runtime_witness` | `classify_interner_live_reference` | `tests` |
| `runtime_witness` | `classify_binder_live_reference` | `tests` |
| `runtime_witness` | `final_reference_summary` | `continuation` |
| `runtime_witness` | `final_identity_witness` | `continuation` |
| `profile` | `record_complete_source_route_work_for_test` | `runtime_state` |
| `profile` | `record_complete_source_bind_for_test` | `complete_source` |
| `profile` | `record_complete_source_evidence_for_test` | `compile_engine` |
| `profile` | `CanonicalInput` | `canonical_bytes`, `collision_plan`, `complete_source`, `compile_api`, `compile_engine`, `probes` |
| `profile` | `independent_event_owner_sites_for_oracle` | `compile_engine` |
| `profile` | `verify_independent_event_owner_sites` | `compile_engine` |
| `profile` | `CanonicalLibraryFrontend` | `complete_source`, `compile_api`, `compile_engine`, `continuation` |
| `profile` | `seed_trusted_library_markers` | `compile_engine` |
| `profile` | `certify_library_values` | `compile_engine` |
| `canonical_bytes` | `ParserExportClaim` | `complete_source`, `probes`, `tests` |
| `canonical_bytes` | `CanonicalBytes` | `runtime_witness`, `tests` |
| `canonical_bytes` | `encode_store_row` | `runtime_witness` |
| `canonical_bytes` | `encode_declared_recipe_row` | `runtime_witness` |
| `canonical_bytes` | `canonical_input_for_record` | `collision_plan` |
| `canonical_bytes` | `canonical_record_bytes` | `collision_plan` |
| `canonical_bytes` | `full_structured_record_baseline` | `collision_plan` |
| `terminal_validation` | `ReplayTerminalValidationInputs` | `collision_plan`, `tests` |
| `terminal_validation` | `TerminalClassDependencyValidationWork` | `tests` |
| `terminal_validation` | `require_terminal_class_dependency_closure` | `tests` |
| `terminal_validation` | `validate_terminal_class_dependencies` | `collision_plan`, `tests` |
| `terminal_validation` | `validate_root_census` | `collision_plan`, `tests` |
| `collision_plan` | `apply_merge_root_semantics` | `tests` |
| `collision_plan` | `build_collision_replay_plan` | `compile_engine` |
| `collision_plan` | `build_collision_replay_index` | `compile_engine` |
| `complete_source` | `with_canonical_library_frontend` | `compile_api`, `continuation`, `tests` |
| `compile_api` | `CanonicalLibraryEvidenceCapture` | `complete_source`, `compile_engine` |
| `compile_api` | `intern_compiled_global_object` | `compile_engine` |
| `compile_api` | `InjectedCompileRoute` | `compile_engine` |
| `compile_api` | `compile_owned_injected_frontend` | `complete_source` |
| `compile_api` | `validate_complete_source_user_units` | `compile_engine`, `tests` |
| `compile_api` | `complete_source_user_file_ordinals_for_test` | `compile_engine` |
| `compile_api` | `assemble_complete_source_user_results` | `compile_engine`, `tests` |
| `compile_engine` | `compile_owned_injected_frontend_for_route` | `compile_api` |
| `continuation` | `build_library_binder_checkpoint` | `runtime_state`, `complete_source` |
| `continuation` | `check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix` | `tests` |
| `probes` | `validate_parser_export_claims` | `complete_source` |
| `probes` | `match_parser_export_claims` | `tests` |
| `probes` | `canonical_inputs` | `complete_source` |
| `probes` | `collect_type_probes` | `compile_engine` |
| `probes` | `type_probe` | `tests` |
| `probes` | `collect_value_probes` | `compile_engine` |
| `probes` | `release_phase_line` | `tests` |
| `probes` | `ReleaseOutcomeLine` | `tests` |

The following private associated methods alone widen to `pub(super)`:

- `IndependentEventOwnerSiteOracle::{new, select_file, reserve_exact_site, finish}`;
- `CollisionPlanForbiddenWorkScope::{start, finish}`;
- `PrivateCollisionEpoch::{owner_sites_for, baselines_for}`;
- `ExplicitSparseSelectionScopeForTest::start`;
- `OwnedLibraryRuntimeState::{install_complete_source_replay_plan_with_permit,
  fork_user_delta_for_test, shares_checker_base_with, final_base_family_clone_counts_for_test,
  final_local_rows_written_for_test}`;
- `LibraryPhaseTimings::measured_total`;
- `CanonicalBytes::{domain, usize, finish, property_key}`; and
- `ReleaseOutcomeLine::render`.

The following private fields alone widen to `pub(super)`:

- all ten fields of `CollisionPlanForbiddenWork`;
- `PrivateCollisionEpoch::_permit`;
- all fields of `OwnedLibraryRuntimeState`;
- all fields of `FinalIdentityInspection`;
- `ValueProbe::visible_type`;
- `InjectedProfileRun::{user_results, ordered_user_records, global_types, module_types,
  global_values, semantic_identities}`;
- all fields of `CanonicalInput` and `CanonicalLibraryFrontend`;
- both fields of `ParserExportClaim`;
- all fields of `ReplayTerminalValidationInputs` and `TerminalClassDependencyValidationWork`; and
- all ten fields of `ReleaseOutcomeLine`.

No private top-level constant or static crosses a child boundary. Tuple fields of
`CanonicalBytes`, `CollisionPlanForbiddenWorkScope`, and the public scope/capability wrappers stay
private. Trait implementation methods on `LegacyLibraryRoute` and `CompleteSourceRoute` stay
private; the `InjectedCompileRoute` trait visibility is sufficient.

### Item manifest

Each TSV row records frozen ordinal, destination, kind/identity, original visibility, cfg
reachability, source span, raw byte partition, normalized token SHA-256, preserved post-move path,
and exposure action. `keep-private` rows may be replaced only by the exact minimum `pub(super)`
exceptions frozen in the preceding subsection.

```text
ordinal	destination	item	visibility	cfg	span	raw-bytes	token-sha256	post-move-path	action
000	facade	use:super :: classes :: application :: ClassTypeParameterDefault	private	default	3:0..3:59	0..156	caefe9e60663c8c5af1c43fcaa0f11f2ccce1f6d81935b88a327b0f988926dbc	checker::library_compiler::<import>	keep-facade-import
001	facade	use:super :: context :: { CertifiedLibraryValues , CheckerRecordBatch , DeclTypes }	private	default	4:0..4:76	156..233	e4166dcaa40025bf48e0b3aae210f458cb0ddfe79fa07f6668120a515d3db254	checker::library_compiler::<import>	keep-facade-import
002	facade	use:super :: decls :: TypeDeclReservationReceipt	private	default	5:0..5:45	233..279	b0eda048069bf6292457d055e86c675a054450de7ea3aebd2cd8b4f331181dde	checker::library_compiler::<import>	keep-facade-import
003	facade	use:super :: events :: { CandidateEffects , EventStore , UserRecordTicket }	private	default	6:0..6:68	279..348	954ea00ad4b5524362dfa7400fed1b0409b6d8add6f34d9c0349a5d4cffa0b74	checker::library_compiler::<import>	keep-facade-import
004	facade	use:super :: events_library :: { library_record_ticket_key , LibraryEventKey , LibraryEventLedger , LibraryEventLedgerError , LibraryRecordTicket , LibrarySemanticReportingAdapter , }	private	default	7:0..10:2	348..531	de3a333566637926e89a8453706fe406b5c64d86106db607cb7a9ff90c1c4eae	checker::library_compiler::<import>	keep-facade-import
005	facade	use:super :: lexical_events :: LexicalReservationAllocator	private	# [cfg (any (test , feature = "test-utils"))]	11:0..12:55	531..629	25c8ad819133a3e0116bf3b4298ad7a77f9b3343759ef683eed84942641e6c32	checker::library_compiler::<import>	keep-facade-import
006	facade	use:super :: lexical_events :: LexicalReservations	private	default	13:0..13:47	629..677	b4740fb15a0af02e40468606d38963377bdc7f4c1f16d9995c10de6b9e06d3b0	checker::library_compiler::<import>	keep-facade-import
007	facade	use:super :: lexical_events_library :: library_unit	private	default	14:0..14:48	677..726	99458cfb2fa3956ca545d68ecfc9c85c41d74a25f22cea73cb75aeebab236a96	checker::library_compiler::<import>	keep-facade-import
008	facade	use:super :: lexical_events_library :: ExactUnit	private	# [cfg (any (test , feature = "test-utils"))]	15:0..16:45	726..814	996edef8f7d6ef4551a71cdeaeac5523161f90c84b31a37aa5e49ef2aca0bce8	checker::library_compiler::<import>	keep-facade-import
009	facade	use:super :: library_identities :: LibraryIdentityTerminal	private	default	17:0..17:55	814..870	89387237f75caee238d71c2d5f048485816c04582d948508c65f26644bc83e81	checker::library_compiler::<import>	keep-facade-import
010	facade	use:super :: library_identities :: { LibrarySemanticIdentities , NativeArrayGroups }	private	default	18:0..18:78	870..949	1472c173c7f8024627affba0e3918712594057e985ae48a08c43f99e301bd75f	checker::library_compiler::<import>	keep-facade-import
011	facade	use:super :: library_reporting :: independent_library_reporting_site_descriptors	private	# [cfg (any (test , feature = "test-utils"))]	19:0..20:77	949..1069	2580c73d26d556c5a1c611d361d6432abc6c4c179f8f072338e59c3f6c1d84aa	checker::library_compiler::<import>	keep-facade-import
012	facade	use:super :: library_reporting :: LibraryReportingConsumer	private	default	21:0..21:55	1069..1125	be9ba0d40db4d35a610aaa0f79e32bde1b1fd806db02b6923355bae4ac3a1541	checker::library_compiler::<import>	keep-facade-import
013	facade	use:super :: library_reporting :: LibraryReportingFamily	private	# [cfg (test)]	22:0..23:53	1125..1192	931487c83c2ab80f93f2d98886d25471e3ad246cecc6e5f0dcd0dfb377cd2e49	checker::library_compiler::<import>	keep-facade-import
014	facade	use:super :: library_reporting :: LibraryReportingReceipt	private	# [cfg (any (test , feature = "test-utils"))]	24:0..25:54	1192..1289	1ba528440a406462c51dde722ae79c6edf5a68852899796fd3ece3422bdb6e37	checker::library_compiler::<import>	keep-facade-import
015	facade	use:super :: namespace_values :: NamespaceValueRegistry	private	# [cfg (any (test , feature = "test-utils"))]	26:0..27:52	1289..1384	735f4b326b35e66fb76181a35dcaa54289aef1ccd935987e89e1ed07a1e9cdc0	checker::library_compiler::<import>	keep-facade-import
016	facade	use:super :: namespace_values :: { FrozenNamespaceValueTerminalSnapshot , FrozenNamespaceValueTerminalSnapshotRow , FrozenNamespaceValueTerminals , }	private	default	28:0..31:2	1384..1536	f299a1086f0aac018c37325d8a9d1bf941e4921534464f4c68a724f1db876346	checker::library_compiler::<import>	keep-facade-import
017	facade	use:super :: replay_index :: ReplayReverseEdge	private	# [cfg (test)]	32:0..33:43	1536..1593	2dc2b0c5e97be0c02a9cd2b0d1f26365ab0ab9d8bc04add4ada34cf3e2143c21	checker::library_compiler::<import>	keep-facade-import
018	facade	use:super :: replay_index :: { admit_generated_collision_replay_index , baseline_record , AdmittedCollisionReplayIndex , CollisionReplayConstructionEvidence , CollisionReplayIndex , CollisionReplayOwnerSite , CollisionReplayPlan , OwnerSiteStorageMode , ReplayDependencyTrace , ReplayIndexAdmissionError , ReplayIndexAdmissionLimits , ReplayIndexGenerationError , ReplayOwner , ReplayOwnerSite , ReplayRootSlot , }	private	default	34:0..40:2	1593..2010	06d279907467b331492f640b575b88920e2f1e458894eddc9042bc61445294af	checker::library_compiler::<import>	keep-facade-import
019	facade	use:super :: replay_index :: { canonicalize_collision_replay_owner_sites , CollisionReplayEventPhase , CollisionReplaySiteProvenance , }	private	# [cfg (any (test , feature = "test-utils"))]	41:0..45:2	2010..2191	65a76df4753bcef4ebb151228b2e4de39cde660bf5fc89b1f266cee4ecc0c994	checker::library_compiler::<import>	keep-facade-import
020	facade	use:super :: reporting_record :: CheckerRecord	private	default	46:0..46:43	2191..2235	63548daa1af246387b6eebe3b181141405980121555c750d4c747e990df61eee	checker::library_compiler::<import>	keep-facade-import
021	facade	use:super :: type_groups :: { PublishedTypeEnvironment , PublishedTypeGroupSurface , PublishedTypeGroupTerminal , PublishedTypeParameterDefault , }	private	default	47:0..50:2	2235..2384	dff1bb068183a16a505a9c766997aec388559ba228cec931bbb9184921ccfad3	checker::library_compiler::<import>	keep-facade-import
022	facade	use:super :: { attach_class_bindings_from_inventory , attach_type_decl_owners , build_pass_with_tickets , enqueue_ambient_context_diagnostics , enqueue_enum_merge_diagnostics , enqueue_local_ambient_export_alias_diagnostics , enqueue_namespace_placement_diagnostics , finish_semantic_effects , private_combined_record_ticket_key , reserve_type_decls_for_combined_library_from_inventory , reserve_type_decls_for_combined_user_from_inventory , reserve_type_decls_from_inventory , FrozenCheckerRuntimeMetadata , FrozenCheckerRuntimeSnapshotParts , PassReporting , PassReportingPlan , PrivateCombinedRecordTicket , }	private	default	51:0..60:2	2384..3011	afec8dd8c9d41dd4a517399c640098c9e3632f73b2df9d2d7f4161e51253d132	checker::library_compiler::<import>	keep-facade-import
023	facade	use:super :: { check_bound_user_program_with_final_identity_inspector , BoundUserBase }	private	# [cfg (any (test , feature = "test-utils"))]	61:0..62:83	3011..3137	e021005360963aa5331585e76549cb53d2a307c016015e215414832eb877507d	checker::library_compiler::<import>	keep-facade-import
024	facade	use:crate :: binder :: bind :: LibraryBinderCheckpointEnds	private	# [cfg (any (test , feature = "test-utils"))]	63:0..64:53	3137..3233	95f12bacc39b2df68d58380ef16fb2536b49a15484238fdd0b52667c03017793	checker::library_compiler::<import>	keep-facade-import
025	facade	use:crate :: binder :: bind :: { ImportPlaceholder , LibraryBinderCheckpoint , LibraryBinderUnit , ProjectBinderBuilder , }	private	default	65:0..67:2	3233..3352	27715347bc44600e5882fd328a249cec50722ecaca129e21a51f5124af59e8f5	checker::library_compiler::<import>	keep-facade-import
026	facade	use:crate :: binder :: declaration :: TypeFragmentKind	private	# [cfg (any (test , feature = "test-utils"))]	68:0..69:49	3352..3444	9c15946760f1a9d5127336cad3d082abb5239e3e9aac8ef9f3f4dc6138850db8	checker::library_compiler::<import>	keep-facade-import
027	facade	use:crate :: binder :: declaration :: { source_global_binding_census_with_provenance , SourceBindingSlot , SourceDeclarationManifest , SourceGlobalContributorKind , TypeGroupId , ValueStorageId , }	private	default	70:0..73:2	3444..3639	acca9c3a416db65bfdec1ea6ed1eccd05c901ef42a7900b21b01e20b0494c858	checker::library_compiler::<import>	keep-facade-import
028	facade	use:crate :: binder :: declaration :: { DeclId , DeclarationKind }	private	# [cfg (any (test , feature = "test-utils"))]	74:0..75:58	3639..3740	f7c253afd42a29e178dbaa9a72022cea65e1b3f18a77f2bbd0ab0b399d46cd1c	checker::library_compiler::<import>	keep-facade-import
029	facade	use:crate :: binder :: namespace :: MergeDeclarationKind	private	# [cfg (any (test , feature = "test-utils"))]	76:0..77:51	3740..3834	7b51269d5cb1d3c2b9ea853adaccefec66232e096d3da3e13d59159a6a8d1b8d	checker::library_compiler::<import>	keep-facade-import
030	facade	use:crate :: binder :: namespace :: { exact_key , source_file_kind , CompilationUnit , ExactKey , ExportContextKind , ExportSyntaxDisposition , ModuleBindingContext , NamespaceId , SourceFileKind , }	private	default	78:0..81:2	3834..4028	7af9f200e38f31f75be2da39baf3629ef282cb89d0c6c785424f5eca9b4bc704	checker::library_compiler::<import>	keep-facade-import
031	facade	use:crate :: binder :: roots :: { collect_root_rows , LibraryRootProjection , RootNameRow }	private	default	82:0..82:82	4028..4111	be8ba7a36a56aab50aab8cea8f15284cdc8deda210f9eb9f40d9a7faec66a014	checker::library_compiler::<import>	keep-facade-import
032	facade	use:crate :: binder :: scope :: ScopeId	private	default	83:0..83:34	4111..4146	8da1a98753180d6bb49995f60c2a2ff0d8c7925d050a752fd402ec6722ab8e61	checker::library_compiler::<import>	keep-facade-import
033	facade	use:crate :: binder :: symbol :: SymbolId	private	# [cfg (any (test , feature = "test-utils"))]	84:0..85:36	4146..4225	7e077636052a4bea41ebf0bcfec0b6ff82bcc32be8201de740304f19fa3e244a	checker::library_compiler::<import>	keep-facade-import
034	facade	use:crate :: binder :: Binder	private	default	86:0..86:26	4225..4252	ec0fc023397ec192ec0f0de7d9b10e9cc08beaf07cd38dcbfad2bcc284b63ab6	checker::library_compiler::<import>	keep-facade-import
035	facade	use:crate :: class_semantics :: CanonicalPublishedClassTerminal	private	default	87:0..87:60	4252..4313	a931e14ac5564b82f9d60201e96ec4e3ed7256fbf2c5c8bb25d613e0752ffde9	checker::library_compiler::<import>	keep-facade-import
036	facade	use:crate :: class_semantics :: DemandOutcome	private	# [cfg (any (test , feature = "test-utils"))]	88:0..89:42	4313..4398	b2df76229510649fbfc70591d87292ef56d15997081a0a2c238534bcf0e13b2c	checker::library_compiler::<import>	keep-facade-import
037	facade	use:crate :: class_semantics :: OwnedPublishedClassTerminal	private	default	90:0..90:56	4398..4455	20bb9c73dcd719eb7fadce288428eeb738322bfac98f9555620e1fc8a92cd532	checker::library_compiler::<import>	keep-facade-import
038	facade	use:crate :: diagnostics :: render_type	private	# [cfg (any (test , feature = "test-utils"))]	91:0..92:36	4455..4534	4d6a901cd7e21019024021456a5e87cbe09ad15cc73402356084c6c5edac923b	checker::library_compiler::<import>	keep-facade-import
039	facade	use:crate :: diagnostics :: { render_to_writer_with_format , DiagnosticFormat }	private	default	93:0..93:73	4534..4608	7b37aa6df7f9bf9bc1d27e60c1e878ff2ccdc0aeb542245c41bcb88a749acb8b	checker::library_compiler::<import>	keep-facade-import
040	facade	use:crate :: source :: { CompilationOrigin , LibraryFileOrdinal , ModuleOrdinal , SourceOrdinal , SourceUnit , UnitSlot , }	private	default	94:0..96:2	4608..4727	be10fd59e21671df72f1da0a609737ab9c076a75f87c9aadbc863049b6bc9b22	checker::library_compiler::<import>	keep-facade-import
041	facade	use:crate :: span :: Span	private	default	97:0..97:22	4727..4750	e83a9993fcc7233cfbb76ff2bb7664969506ea60b65691ea5c45ce0be6f3f338	checker::library_compiler::<import>	keep-facade-import
042	facade	use:crate :: types :: repr :: ClassId	private	default	98:0..98:32	4750..4783	c0d01eea6f509a88fc14117d60e7be0904cd6016112d3083500093155978332d	checker::library_compiler::<import>	keep-facade-import
043	facade	use:crate :: types :: repr :: TypeParamId	private	default	99:0..99:36	4783..4820	86e6876cb87f6d33d545bbcfc5238fee5739993eb93d8148100aacf2a98c11b6	checker::library_compiler::<import>	keep-facade-import
044	facade	use:crate :: types :: repr :: { EnumValue , IntrinsicKind , LiteralValue , ModifierOp , PropertyKey , TypeTag , Visibility , WellKnownSymbol , }	private	# [cfg (any (test , feature = "test-utils"))]	100:0..104:2	4820..5002	fcf1768dfd52a39af874da4da1198d28d16714ca2cf8d83312a58a6f7d44bd53	checker::library_compiler::<import>	keep-facade-import
045	facade	use:crate :: types :: repr :: { ObjectType , PropertyType }	private	default	105:0..105:51	5002..5054	55df098b8c4f8b0b22f2a2fe82fb2fbe42467811e96ec52905b230070d836d0f	checker::library_compiler::<import>	keep-facade-import
046	facade	use:crate :: types :: store :: Store	private	# [cfg (any (test , feature = "test-utils"))]	106:0..107:31	5054..5128	8e3ca6ad45b21585b8b95f2d5292fc1878d177333e72587214a0f3c3802094e1	checker::library_compiler::<import>	keep-facade-import
047	facade	use:crate :: types :: store :: TypeId	private	default	108:0..108:32	5128..5161	e3151523491c0a543186b8ed571d201d02c1182fcc63323373892cdeca728ef4	checker::library_compiler::<import>	keep-facade-import
048	facade	use:crate :: types :: Interner	private	default	109:0..109:27	5161..5189	28b89208227b703591b34a98e88f9470e245389f623c021f6b5e3f390b509b54	checker::library_compiler::<import>	keep-facade-import
049	facade	use:oxc_allocator :: Allocator	private	default	110:0..110:29	5189..5219	d2412c91f40744e7a4655e20e959134af95a0eb3babd670648f0a170a1513a9c	checker::library_compiler::<import>	keep-facade-import
050	facade	use:oxc_ast :: ast :: Program	private	default	111:0..111:26	5219..5246	fc4d772f90254e37f0271d67a1655d728a83f453f2d973a092b49ef6d1306d5b	checker::library_compiler::<import>	keep-facade-import
051	facade	use:oxc_parser :: Parser	private	default	112:0..112:23	5246..5270	dccbf5b3dbd074ac73402bd7d1277c7a7b9012d4b9d1db964a1292919e364430	checker::library_compiler::<import>	keep-facade-import
052	facade	use:oxc_span :: SourceType	private	default	113:0..113:25	5270..5296	1571f4ec36c29128dcd7a368417339ee7db1b14a7a56ff56e6c34f0aad754604	checker::library_compiler::<import>	keep-facade-import
053	facade	use:sha2 :: { Digest , Sha256 }	private	# [cfg (any (test , feature = "test-utils"))]	114:0..115:27	5296..5366	1b3516da781a0c34b1cd20f22823af16cc6df6681d557f9413059b77315bccaa	checker::library_compiler::<import>	keep-facade-import
054	facade	use:std :: cell :: Cell	private	default	116:0..116:20	5366..5387	6cd6cdc4cdaca8b2014c82648029f80fcc1ad33dd889bac6edb048fbfd8d895d	checker::library_compiler::<import>	keep-facade-import
055	facade	use:std :: cell :: RefCell	private	# [cfg (any (test , feature = "test-utils"))]	117:0..118:23	5387..5453	55bf7f52c9a3c95d0b7280d1b95b12aef80e9f252c099a08b4b2083e447a84e3	checker::library_compiler::<import>	keep-facade-import
056	facade	use:std :: collections :: { BTreeMap , BTreeSet }	private	default	119:0..119:43	5453..5497	005d4e7de5fa46792e77ea07c549a92cfcaae95e3561eef471d1f9ea8711e788	checker::library_compiler::<import>	keep-facade-import
057	facade	use:std :: sync :: { Condvar , Mutex }	private	default	120:0..120:32	5497..5530	5e7a9c5b170d3352403baccdca9c6e5ab0f5cb95421d8bc0f6a8e93e114bc3ed	checker::library_compiler::<import>	keep-facade-import
058	facade	use:std :: time :: { Duration , Instant }	private	# [cfg (any (test , feature = "test-utils"))]	121:0..122:35	5530..5608	15c6b5bc9b4b6aabb94245a039a980b85ac62ad568ac2e40c7742506233b5246	checker::library_compiler::<import>	keep-facade-import
059	replay_control	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	124:0..128:1	5608..5818	72789471a9acc81f0ec66f778f780c339b88e44a9386ce73306b1a5e4aaaf6ce	checker::library_compiler::replay_control::thread_local	keep-private
060	replay_control	const:PRIVATE_REPLAY_SCHEDULER_FAMILIES	private	# [cfg (any (test , feature = "test-utils"))]	130:0..142:2	5818..6133	a8ceaf8c74d4babc137eae64fda85f86e7dd2c4d50e1c118c8ce1bdb1d2a427c	checker::library_compiler::replay_control::PRIVATE_REPLAY_SCHEDULER_FAMILIES	keep-private
061	replay_control	fn:record_private_replay_scheduler_work_for_test	private	# [cfg (any (test , feature = "test-utils"))]	144:0..154:1	6133..6576	864049c468f062141dda786e341f32f6b2cfb7559a7c0e44f12348c1f1211fd5	checker::library_compiler::replay_control::record_private_replay_scheduler_work_for_test	widen-sibling
062	replay_control	fn:record_private_replay_scheduler_work_for_test	private	# [cfg (not (any (test , feature = "test-utils")))]	156:0..157:89	6576..6714	f5e5c49dd9355f09482b79ec2611e180d02a48481171192a8fbc24e001bb9b09	checker::library_compiler::replay_control::record_private_replay_scheduler_work_for_test	widen-sibling
063	replay_control	struct:PrivateReplaySchedulerWorkScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	159:0..160:50	6714..6808	871fa5fc910a5c921ac7eb1795e25c70e27cedbd9c3ebcd8c274bf4ca2248a9c	checker::library_compiler::PrivateReplaySchedulerWorkScopeForTest	glob-reexport
064	replay_control	impl:inherent for PrivateReplaySchedulerWorkScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	162:0..185:1	6808..7570	661eeefd6d30faf68f78db82a5428e32c0f4bfdab597d65f36afe36fae914a3a	checker::library_compiler::replay_control::inherent for PrivateReplaySchedulerWorkScopeForTest	keep-private
065	replay_control	fn:calibrate_private_replay_scheduler_work_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	187:0..217:1	7570..9183	f21cf7a96390da92047ca44b6b17388888457313a75ee3970cb73a71efc5704d	checker::library_compiler::calibrate_private_replay_scheduler_work_for_test	glob-reexport
066	replay_control	struct:CollisionPlanForbiddenWork	private	default	219:0..231:1	9183..9643	71d8b36489c98e44de214c85cdc657e76bb18b57c1293a08f0f07c9aabef48ba	checker::library_compiler::replay_control::CollisionPlanForbiddenWork	widen-sibling
067	replay_control	macro:thread_local	private	default	233:0..242:1	9643..10136	a046e7f3322e58465facaf8e6dca25ef483ef3ff6d311b8a68a35bbcbdf8ac2e	checker::library_compiler::replay_control::thread_local	keep-private
068	replay_control	struct:FullCollisionPlanOracleForTest	pub	# [cfg (any (test , feature = "test-utils"))]	244:0..250:1	10136..10458	4e21ee084801afb58d9de8611aeee136effc383ee249a1aee4b6f2aabc382386	checker::library_compiler::FullCollisionPlanOracleForTest	glob-reexport
069	replay_control	struct:IndependentEventMeta	private	# [cfg (any (test , feature = "test-utils"))]	252:0..259:1	10458..10675	766828bd99540c5c27a671f6209c6648768895db919d536daf5060dc57b5b5c2	checker::library_compiler::replay_control::IndependentEventMeta	keep-private
070	replay_control	struct:IndependentEventOwnerSiteOracle	private	# [cfg (any (test , feature = "test-utils"))]	261:0..269:1	10675..11035	90561fa85daa05a9c3bba2beaf32f136693dad2664270e9e43ecb3cc0735bddf	checker::library_compiler::replay_control::IndependentEventOwnerSiteOracle	widen-sibling
071	replay_control	impl:inherent for IndependentEventOwnerSiteOracle	private	# [cfg (any (test , feature = "test-utils"))]	271:0..302:1	11035..12155	ef096a979126315620ba10f28c7df4c78e35e40a1b077c03c661ef100417ff06	checker::library_compiler::replay_control::inherent for IndependentEventOwnerSiteOracle	keep-private
072	replay_control	impl:LexicalReservationAllocator for IndependentEventOwnerSiteOracle	private	# [cfg (any (test , feature = "test-utils"))]	304:0..374:1	12155..14556	286e791046ac3acdda67656f43e14e73217bf94040c94bd018203b2983008b2e	checker::library_compiler::replay_control::LexicalReservationAllocator for IndependentEventOwnerSiteOracle	keep-private
073	replay_control	fn:take_full_collision_plan_oracle_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	376:0..379:1	14556..14768	908febf43c9bc6eacd2f7911799653bc7775cc2d28750619406c056eca0e860e	checker::library_compiler::take_full_collision_plan_oracle_for_test	glob-reexport
074	replay_control	fn:forbidden_projection_callsite_audit_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	381:0..397:1	14768..15531	af74b7eecb687a6b8ebb5284001fe3c08cd236c9cd16848ebe62d0aea13d219d	checker::library_compiler::forbidden_projection_callsite_audit_for_test	glob-reexport
075	replay_control	enum:ForcedCollisionPlanFailure	pub	# [cfg (any (test , feature = "test-utils"))]	399:0..407:1	15531..15788	2e51e646699d94d9036645f8530fd9fba6fc8789a48713c58466769089be0f3d	checker::library_compiler::ForcedCollisionPlanFailure	glob-reexport
076	replay_control	struct:ForcedCollisionPlanFailureScope	private	# [cfg (any (test , feature = "test-utils"))]	409:0..410:39	15788..15871	f2fb72a4a3934b30879859589849faba70e80ace3cde0b2154f537b0b137a424	checker::library_compiler::replay_control::ForcedCollisionPlanFailureScope	widen-sibling
077	replay_control	impl:Drop for ForcedCollisionPlanFailureScope	private	# [cfg (any (test , feature = "test-utils"))]	412:0..417:1	15871..16044	e526636ad5c965a6d7fb8a7489a8b4a475b66edf0fa1aec469211cd6bb7aaa48	checker::library_compiler::replay_control::Drop for ForcedCollisionPlanFailureScope	keep-private
078	replay_control	struct:CollisionPlanForbiddenWorkScope	private	default	419:0..419:67	16044..16113	15f832d9cbc3825b7168f702a24695d09ba86b13822b4eca343c44e1e565c634	checker::library_compiler::replay_control::CollisionPlanForbiddenWorkScope	widen-sibling
079	replay_control	impl:inherent for CollisionPlanForbiddenWorkScope	private	default	421:0..461:1	16113..17970	c1466501169fead2dbbbfbc671d5f331030a94a262cfadc53ce01c5f9b81ae83	checker::library_compiler::replay_control::inherent for CollisionPlanForbiddenWorkScope	keep-private
080	replay_control	fn:record_collision_plan_forbidden_work	private	default	463:0..469:1	17970..18217	7cf127dddde09e80f19a2afa644c1dfd11552438d4d002de50806f2ab12712de	checker::library_compiler::replay_control::record_collision_plan_forbidden_work	widen-sibling
081	replay_control	fn:record_collision_manifest_bytes	pub (super)	default	471:0..477:1	18217..18494	6c530c595e46156aa58029d80aad57fcd96978f8acbcb35fbd4339722b7247b6	checker::library_compiler::record_collision_manifest_bytes	widen-checker+glob-reexport
082	replay_control	fn:snapshot_namespace_terminals_for_replay	private	default	479:0..491:1	18494..19041	5814d277ec002bf746a557867243f46b357ad8334b87efeec112fc8b966d1ff8	checker::library_compiler::replay_control::snapshot_namespace_terminals_for_replay	widen-sibling
083	replay_control	struct:CollisionFreeUserDeltaCapability	pub	default	493:0..494:48	19041..19162	c084e4673da7f8affb25013f1b081ab0387c71c578cb812d43b9f70c133cdd17	checker::library_compiler::CollisionFreeUserDeltaCapability	glob-reexport
084	replay_control	impl:inherent for CollisionFreeUserDeltaCapability	private	default	496:0..500:1	19162..19257	35e5e4e31d69c5a5732664aa4c129b24b5c31414e97fd092fc2f5ecbfc37bfc8	checker::library_compiler::replay_control::inherent for CollisionFreeUserDeltaCapability	keep-private
085	runtime_model	struct:OwnedLibraryRuntimeState	pub	default	502:0..515:1	19257..19766	a5e1b1efdb766e29b03d488178ba1c54fa0b8f182729d6983b7e4b3d12527b78	checker::library_compiler::OwnedLibraryRuntimeState	glob-reexport
086	replay_control	struct:PrivateCollisionReplaySeed	pub	default	517:0..524:1	19766..19963	b5f82d29879feb82f26f3d53b678ad8d8d4ba91a1101c9c94943644fe558d4bc	checker::library_compiler::PrivateCollisionReplaySeed	glob-reexport
087	replay_control	struct:PrivateCollisionReplaySource	pub	default	526:0..531:1	19963..20135	b55530dbc4ab7095aaa063e9f8c4e471aa77b5d897c501c837d254306ae87fce	checker::library_compiler::PrivateCollisionReplaySource	glob-reexport
088	replay_control	struct:PrivateCollisionReplaySourceRegistry	pub	default	533:0..536:1	20135..20275	1ea28bc545ec40289f6d799d57f3debe60911d90ea445ae7bcc02b7dd41b5e79	checker::library_compiler::PrivateCollisionReplaySourceRegistry	glob-reexport
089	replay_control	impl:std :: fmt :: Debug for PrivateCollisionReplaySourceRegistry	private	default	538:0..545:1	20275..20591	b088c8803cb22098f155a3468f34732c147545c466526cf44aa4b324132b748f	checker::library_compiler::replay_control::std :: fmt :: Debug for PrivateCollisionReplaySourceRegistry	keep-private
090	replay_control	impl:inherent for PrivateCollisionReplaySourceRegistry	private	default	547:0..575:1	20591..21534	9f4461455f42a7a41cb8fda4e9fcfef6a08241669c6cd9d2b656cdc9044264db	checker::library_compiler::replay_control::inherent for PrivateCollisionReplaySourceRegistry	keep-private
091	replay_control	struct:PrivateCollisionEpoch	pub (in crate :: check :: checker)	default	577:0..592:1	21534..22485	fd8c089a0bda92c4cfafda7af8e4e9e3acfe8e12a4799646bf7c27a43a1b783f	checker::library_compiler::PrivateCollisionEpoch	glob-reexport
092	replay_control	struct:PrivateCollisionStateDropWitnessForTest	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	594:0..596:77	22485..22623	90a2777d64b8083e34cbf745e16f8498ee7d3d0c0d709a66da556f9917fa37c4	checker::library_compiler::PrivateCollisionStateDropWitnessForTest	glob-reexport
093	replay_control	impl:Drop for PrivateCollisionStateDropWitnessForTest	private	# [cfg (any (test , feature = "test-utils"))]	598:0..605:1	22623..22883	4e0d9429706a4c72bdd816cb39b5aac4f045e9c3547b80d5717cb64f5929d491	checker::library_compiler::replay_control::Drop for PrivateCollisionStateDropWitnessForTest	keep-private
094	replay_control	impl:inherent for PrivateCollisionEpoch	private	default	607:0..727:1	22883..27557	8c7768bfb596454a95cb46ecc22f02f0f697b5df5e9d2926b68b592bb7f5baeb	checker::library_compiler::replay_control::inherent for PrivateCollisionEpoch	keep-private
095	replay_control	enum:PrivateReplayScheduleFailure	pub (in crate :: check :: checker)	default	729:0..733:1	27557..27763	6ff1a835ac99588dd99548e64577eae1296928ed8c838385f6faaa967dfe6808	checker::library_compiler::PrivateReplayScheduleFailure	glob-reexport
096	replay_control	struct:PrivateCollisionReplayPermit	private	default	735:0..739:1	27763..27872	f24c1c4427a3a8edcbca0b334a3fde7fc9485b55dd86e5f350c98fbcbd48f7a0	checker::library_compiler::replay_control::PrivateCollisionReplayPermit	keep-private
097	replay_control	struct:PrivateReplayScaleSharedForTest	private	# [cfg (any (test , feature = "test-utils"))]	741:0..750:1	27872..28279	2163d07f167d457bbda6fb859f1d7f34fa052e78549c5165e5bbe8a23391c8b9	checker::library_compiler::replay_control::PrivateReplayScaleSharedForTest	keep-private
098	replay_control	fn:update_scale_max_for_test	private	# [cfg (any (test , feature = "test-utils"))]	752:0..763:1	28279..28720	d666c03c5ae41e4f150ea3f6e9e805279a975a386e74fe83d8efe68d160305c4	checker::library_compiler::replay_control::update_scale_max_for_test	keep-private
099	replay_control	struct:PrivateReplayScaleRouteTraceForTest	pub	# [cfg (any (test , feature = "test-utils"))]	765:0..780:1	28720..29334	98cd5120efbe6ac15d82f57cbd536cd59542dd719188dcfc4fb8bf72f1a38ef9	checker::library_compiler::PrivateReplayScaleRouteTraceForTest	glob-reexport
100	replay_control	struct:PrivateReplayScaleControlForTest	private	# [cfg (any (test , feature = "test-utils"))]	782:0..790:1	29334..29707	46e168f945c4f6434da3455f3ffc995716c94c65dc593c8116e622a23fc34cf5	checker::library_compiler::replay_control::PrivateReplayScaleControlForTest	keep-private
101	replay_control	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	792:0..796:1	29707..29921	a4e64cc98d0fc70aba645400ba316978de019153b756c60127728a340b03f7be	checker::library_compiler::replay_control::thread_local	keep-private
102	replay_control	macro:thread_local	private	# [cfg (test)]	798:0..802:1	29921..30101	115da8aafc820f4240576743675ad0c08e6a2eaca84af4c9262aff72962bfc60	checker::library_compiler::replay_control::thread_local	keep-private
103	replay_control	fn:explicit_sparse_selection_for_test	private	# [cfg (test)]	804:0..807:1	30101..30257	18f524089da1bb22ca96cc362eaeadac30416c416e005b61e41caeb8f97c8441	checker::library_compiler::replay_control::explicit_sparse_selection_for_test	keep-private
104	replay_control	struct:ExplicitSparseSelectionScopeForTest	private	# [cfg (test)]	809:0..810:43	30257..30315	026104c451a2555a66a1bd356d574ca6b395be338f127fd8e69f4fa87f4e287a	checker::library_compiler::replay_control::ExplicitSparseSelectionScopeForTest	widen-sibling
105	replay_control	impl:inherent for ExplicitSparseSelectionScopeForTest	private	# [cfg (test)]	812:0..824:1	30315..30702	803e8fc696d110ff2d200e1435cada1d5bdafb39a94aad8cefd6390eefeb0829	checker::library_compiler::replay_control::inherent for ExplicitSparseSelectionScopeForTest	keep-private
106	replay_control	impl:Drop for ExplicitSparseSelectionScopeForTest	private	# [cfg (test)]	826:0..833:1	30702..30910	8d33fd9d68ae319354fb5fe7c39f646a5aa4c4ed51df742210b17ec207b0ffb6	checker::library_compiler::replay_control::Drop for ExplicitSparseSelectionScopeForTest	keep-private
107	replay_control	struct:PrivateReplayScaleRunForTest	pub	# [cfg (any (test , feature = "test-utils"))]	835:0..839:1	30910..31082	6b62ebee3015a7fb7cdf4672e5bd5c4d3c6d71024d1ce7cef85b69e9f576191a	checker::library_compiler::PrivateReplayScaleRunForTest	glob-reexport
108	replay_control	impl:inherent for PrivateReplayScaleRunForTest	private	# [cfg (any (test , feature = "test-utils"))]	841:0..872:1	31082..31916	140453e024a7f367dc410a69c0d41ed6ae9170682505aa1ee1c2a96ac57573eb	checker::library_compiler::replay_control::inherent for PrivateReplayScaleRunForTest	keep-private
109	replay_control	struct:PrivateReplayScaleRouteScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	874:0..875:47	31916..32007	9f85bc8aa7be52f18ef00472ba93335daebc5803b587f6c2031924cde512ca99	checker::library_compiler::PrivateReplayScaleRouteScopeForTest	glob-reexport
110	replay_control	impl:inherent for PrivateReplayScaleRouteScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	877:0..910:1	32007..33356	e7f581045f7cc37d6f72d9030bc4cca656b4d4db19797224451f308b4184604c	checker::library_compiler::replay_control::inherent for PrivateReplayScaleRouteScopeForTest	keep-private
111	replay_control	fn:inject_checker_full_base_scan_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	912:0..919:1	33356..33657	8ce779be314da3954d46ee8dff6c0e8729752a064df4d16cc50edcef8ce53995	checker::library_compiler::inject_checker_full_base_scan_for_test	glob-reexport
112	replay_control	fn:inject_checker_full_plan_scan_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	921:0..928:1	33657..33958	761d54466d5448ff0a3355e84a2bf2e1465d233bde7270fc3565decd12aed032	checker::library_compiler::inject_checker_full_plan_scan_for_test	glob-reexport
113	replay_control	fn:inject_checker_full_source_registry_scan_for_test	private	# [cfg (any (test , feature = "test-utils"))]	930:0..937:1	33958..34251	170933776d0c7436a758bea46a75858a6b861a0ee4b840ffe36d62f61fac570e	checker::library_compiler::replay_control::inject_checker_full_source_registry_scan_for_test	keep-private
114	replay_control	fn:with_private_replay_scale_control_for_test	private	# [cfg (any (test , feature = "test-utils"))]	939:0..948:1	34251..34565	9e0c0dbdeccbd79efed3cfcbe18f053aeb8c83573397eb4a9b0a8a60c61985dc	checker::library_compiler::replay_control::with_private_replay_scale_control_for_test	widen-sibling
115	replay_control	fn:record_private_replay_scale_hook_for_test	private	# [cfg (any (test , feature = "test-utils"))]	950:0..965:1	34565..35150	06c3797a410925746a07ed744b5531516faa5ed9163e20fe6e02019b772e67e5	checker::library_compiler::replay_control::record_private_replay_scale_hook_for_test	widen-sibling
116	replay_control	fn:record_private_replay_route_invocation_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	967:0..973:1	35150..35445	2b7fe1450136386eccfda78496f56b1bdf39a1822f56e9c720f3ba9dc388644f	checker::library_compiler::record_private_replay_route_invocation_for_test	glob-reexport
117	replay_control	fn:record_private_replay_fallback_invocation_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	975:0..983:1	35445..35778	65135239f69209a1bcb6d99af4ea5448c4be8552428c066c98884ca5084a32cd	checker::library_compiler::record_private_replay_fallback_invocation_for_test	glob-reexport
118	replay_control	fn:record_private_replay_visibility_query_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	985:0..993:1	35778..36122	5baa13ab29b186addedbdfaa695b8826b2caae0b0004c4dfd76be68e982f1b26	checker::library_compiler::record_private_replay_visibility_query_for_test	glob-reexport
119	replay_control	fn:record_private_replay_full_base_scan_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	995:0..1003:1	36122..36477	b1e2f65205a2946c16029a25cd23e97a3f01252f4f240a16d4325f25467690bd	checker::library_compiler::record_private_replay_full_base_scan_for_test	glob-reexport
120	replay_control	fn:private_replay_scale_contender_entered_for_test	private	# [cfg (any (test , feature = "test-utils"))]	1005:0..1015:1	36477..36892	aee84ec9f9762cca467dec6fe8d2b90dc8878af84801843927488a89c85f0e71	checker::library_compiler::replay_control::private_replay_scale_contender_entered_for_test	keep-private
121	replay_control	fn:private_replay_scale_permit_acquired_for_test	private	# [cfg (any (test , feature = "test-utils"))]	1017:0..1048:1	36892..38060	61a96ab3cc89f7f944787b4f5adb1d3de5608b0b8c6c9e2d162001de163576e3	checker::library_compiler::replay_control::private_replay_scale_permit_acquired_for_test	keep-private
122	replay_control	fn:private_replay_scale_permit_released_for_test	private	# [cfg (any (test , feature = "test-utils"))]	1050:0..1070:1	38060..38788	08f8888c65617f17d392c02af6daeda2e1753db081ccf4345c6db063ac113e87	checker::library_compiler::replay_control::private_replay_scale_permit_released_for_test	keep-private
123	replay_control	impl:inherent for PrivateCollisionReplayPermit	private	default	1072:0..1119:1	38788..40678	9b70f0247cf9f74cd6b48b50b4c3ca944a4b1fea996e6958d5379fc638e60657	checker::library_compiler::replay_control::inherent for PrivateCollisionReplayPermit	keep-private
124	replay_control	struct:PrivateCollisionReplayPermitToken	pub	default	1121:0..1124:1	40678..40815	c85b7e9054b60e14918748f0db88434340f76d207afcb0c8c378d423401d6b51	checker::library_compiler::PrivateCollisionReplayPermitToken	glob-reexport
125	replay_control	struct:PrivateCollisionReplayPermitLease	private	default	1126:0..1131:1	40815..41000	5dc0962d6678701e41f2386c0db1f3331d27fc0cfb58b4c505e764e73355dd4a	checker::library_compiler::replay_control::PrivateCollisionReplayPermitLease	keep-private
126	replay_control	impl:Drop for PrivateCollisionReplayPermitLease	private	default	1133:0..1149:1	41000..41693	b9e4929a5947097e71d10d6747260a8f2ac299402e4ec5e0f8db3ed914c1f453	checker::library_compiler::replay_control::Drop for PrivateCollisionReplayPermitLease	keep-private
127	replay_control	static:PRIVATE_COLLISION_REPLAY_EPOCHS	private	# [cfg (any (test , feature = "test-utils"))]	1151:0..1153:43	41693..41853	e546a444e314a0518cb4f6181b8a6b590f75e64df2ad7e27f69c4248ccf43efd	checker::library_compiler::replay_control::PRIVATE_COLLISION_REPLAY_EPOCHS	keep-private
128	replay_control	static:PRIVATE_COLLISION_REPLAY_LIFECYCLE_EVENTS	private	# [cfg (any (test , feature = "test-utils"))]	1154:0..1156:41	41853..42018	37198ff1ae8c8b5730ebae6d37272f7be67d15ed57a0d7d2b5716122368ca832	checker::library_compiler::replay_control::PRIVATE_COLLISION_REPLAY_LIFECYCLE_EVENTS	keep-private
129	replay_control	static:PRIVATE_COLLISION_REPLAY_LAST_RELEASE_EVENT	private	# [cfg (any (test , feature = "test-utils"))]	1157:0..1159:41	42018..42185	9a6a4d0821970ff4fa3a6f4dac0a4dd4df7eb710de9cd371029731f070a422a6	checker::library_compiler::replay_control::PRIVATE_COLLISION_REPLAY_LAST_RELEASE_EVENT	keep-private
130	replay_control	static:PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS	private	# [cfg (any (test , feature = "test-utils"))]	1160:0..1162:41	42185..42350	f58b8d497d0ece18b054045472e20ab6171e1b6f8f813bd6f74075afa82e8eec	checker::library_compiler::replay_control::PRIVATE_COLLISION_REPLAY_HOOK_INVOCATIONS	keep-private
131	replay_control	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	1164:0..1168:1	42350..42534	a1e1baf46243f7dffdffe1457cb3d2a68b5f7ef925ee447b403b754c108d7260	checker::library_compiler::replay_control::thread_local	keep-private
132	replay_control	struct:PrivateReplayPermitAcquisitionScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	1170:0..1172:59	42534..42652	8c83fe5af84c6a40103cf9a3485f819ade9e8762eff788368cacfdf3c0e77bfd	checker::library_compiler::PrivateReplayPermitAcquisitionScopeForTest	glob-reexport
133	replay_control	impl:inherent for PrivateReplayPermitAcquisitionScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	1174:0..1185:1	42652..42995	fda82c07f439089f5d120d54680d70e6120b17fc0835d1de3290441ca07e67ca	checker::library_compiler::replay_control::inherent for PrivateReplayPermitAcquisitionScopeForTest	keep-private
134	replay_control	impl:inherent for PrivateCollisionReplayPermitToken	private	# [cfg (any (test , feature = "test-utils"))]	1187:0..1196:1	42995..43296	5d3abe9d2933070ac55d09fa09c227e7ca5267948be02a0618c06c0f5da5d9ef	checker::library_compiler::replay_control::inherent for PrivateCollisionReplayPermitToken	keep-private
135	replay_control	fn:next_private_replay_lifecycle_event_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	1198:0..1203:1	43296..43537	6f52bccb7108ba78eca748b84d5473a449cf95dd19287dd9e30107a202d2a118	checker::library_compiler::next_private_replay_lifecycle_event_for_test	glob-reexport
136	replay_control	fn:private_replay_last_release_event_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	1205:0..1208:1	43537..43734	a4faf9afd6e45363c5094d97c389872aff27ee3895df9d171310f3d36c960856	checker::library_compiler::private_replay_last_release_event_for_test	glob-reexport
137	replay_control	fn:private_replay_hook_invocations_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	1210:0..1213:1	43734..43927	3bdb017f2990f5e53a1c07627d5a58c41ef5621980e2b902609371c6d8a8940f	checker::library_compiler::private_replay_hook_invocations_for_test	glob-reexport
138	replay_control	fn:private_replay_epoch_count_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	1215:0..1218:1	43927..44107	e8fd1750e563d3e35c345d27d9dc927c7de2e192c679d19da59210c4e1a604b5	checker::library_compiler::private_replay_epoch_count_for_test	glob-reexport
139	replay_control	static:PRIVATE_COLLISION_REPLAY_PERMIT	private	default	1220:0..1221:40	44107..44220	8a372bb1a601f0924e7ed4023c8dd33a9cf95d57763912024aec25738e2a5880	checker::library_compiler::replay_control::PRIVATE_COLLISION_REPLAY_PERMIT	keep-private
140	replay_control	fn:acquire_private_collision_replay_permit	pub	default	1223:0..1226:1	44220..44380	45b0f99224383d69f1e32ae1848705566ac0702aeb3d76136282a1ef93dfac80	checker::library_compiler::acquire_private_collision_replay_permit	glob-reexport
141	runtime_model	struct:OwnedLibraryRuntimeProductParts	pub (in crate :: check :: checker)	default	1228:0..1240:1	44380..45132	37ae12fe40fe6300fbcc77234c97b9cbcb998d5ce6f2064f2e58b266b85ff9f1	checker::library_compiler::OwnedLibraryRuntimeProductParts	glob-reexport
142	runtime_model	struct:CompiledLibraryRuntimeProduct	pub	default	1242:0..1245:1	45132..45306	b55069a48dda771ee0b586c48620ff4493828e33f765eb59065e4b1088730a93	checker::library_compiler::CompiledLibraryRuntimeProduct	glob-reexport
143	runtime_model	fn:freeze_library_runtime_product	pub	default	1247:0..1260:1	45306..45763	5f7cfaeef6b7e3acf98b15a3f644e51ae99341913ef2f6c0bcfa81cd12de6c29	checker::library_compiler::freeze_library_runtime_product	glob-reexport
144	runtime_model	fn:validate_library_source_prefix	private	default	1262:0..1301:1	45763..47142	d4b7249f335b49faf21ca0ec534b83ffb2da4d404161564c8e8a8e5a1551e17c	checker::library_compiler::runtime_model::validate_library_source_prefix	keep-private
145	runtime_model	fn:validate_owned_library_product_parts	private	default	1303:0..1703:1	47142..62754	8df1ea1387c71d60d702398046b26784398b8de891919489d8c6043f817d0382	checker::library_compiler::runtime_model::validate_owned_library_product_parts	widen-sibling
146	runtime_state	impl:inherent for OwnedLibraryRuntimeState	private	default	1705:0..2533:1	62754..97113	2b7d36f0ee2937f1dbb750a26e410e05e419dea0e6dde3e5a43b8533cbcb3018	checker::library_compiler::runtime_state::inherent for OwnedLibraryRuntimeState	keep-private
147	runtime_state	fn:compile_synthetic_padding_base_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	2535:0..2558:1	97113..98019	98dbfc0095af2416c2268d1101e6b1f4f064f5be5e726400147615a06e9c075d	checker::library_compiler::compile_synthetic_padding_base_for_test	glob-reexport
148	runtime_state	fn:with_forced_collision_identity_selection_pending	pub	# [cfg (any (test , feature = "test-utils"))]	2560:0..2563:1	98019..98240	abe763c04a0811c4f51a8548439d98a7559c1a3b965a8426f41a040f06afa136	checker::library_compiler::with_forced_collision_identity_selection_pending	glob-reexport
149	runtime_witness	struct:OwnedBaseUserTimings	pub	# [cfg (any (test , feature = "test-utils"))]	2565:0..2571:1	98240..98441	c853a4f694be34b31f873e6f01035598bc0456c59a96980672bc33cdf6510776	checker::library_compiler::OwnedBaseUserTimings	glob-reexport
150	runtime_witness	struct:OwnedBaseUserRun	pub	# [cfg (any (test , feature = "test-utils"))]	2573:0..2579:1	98441..98693	a12a51d175631cd6f49581166d31cc08a602235983a5bc5f9c07c2a66a1573c7	checker::library_compiler::OwnedBaseUserRun	glob-reexport
151	runtime_witness	struct:OwnedBaseUserProjectRun	pub	# [cfg (any (test , feature = "test-utils"))]	2581:0..2586:1	98693..98939	75ed80a1778faf27bf96a45004fff6cbb6cb699bd01ee6c2ad2b9052ba87c60c	checker::library_compiler::OwnedBaseUserProjectRun	glob-reexport
152	runtime_witness	struct:OwnedBaseCrossFileWitness	pub	# [cfg (any (test , feature = "test-utils"))]	2588:0..2595:1	98939..99248	ef8ff5879efe81e9bef8248c9b95b6bd51aab5ca420608e03a94affa303e2159	checker::library_compiler::OwnedBaseCrossFileWitness	glob-reexport
153	runtime_witness	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	2597:0..2606:1	99248..99848	4e8fd155267b827f592ed0b7eca7ba3d9644fe1c3cd68b450fc01ef1e1b2e1b0	checker::library_compiler::runtime_witness::thread_local	keep-private
154	runtime_witness	enum:PrivateReplayProductionFaultForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2608:0..2616:1	99848..100193	ee3473b537b83f828ff3d41f34c63f440f9a2a79b191892b86b6be20873fd596	checker::library_compiler::PrivateReplayProductionFaultForTest	glob-reexport
155	runtime_witness	struct:PrivateReplayProductionTraceForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2618:0..2645:1	100193..101548	59fa5a7929b40b5311e98bc12a02f59d56bf07140fea9cf8dacfac193efc87ff	checker::library_compiler::PrivateReplayProductionTraceForTest	glob-reexport
156	runtime_witness	enum:PrivateReplayProductionFailureForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2647:0..2654:1	101548..101811	4dada49749f23c1ad8708913109b5c166cdc464bddef256fe693187e0030350e	checker::library_compiler::PrivateReplayProductionFailureForTest	glob-reexport
157	runtime_witness	struct:PrivateReplayProductionControlForTest	private	# [cfg (any (test , feature = "test-utils"))]	2656:0..2661:1	101811..102020	3fde8f9a689cfdb80621c9bf7a1b470bbdb6543c4d48a28b5e346f2254011ef4	checker::library_compiler::runtime_witness::PrivateReplayProductionControlForTest	keep-private
158	runtime_witness	struct:PrivateReplayProductionScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2663:0..2664:47	102020..102111	f47e51e49f96e73018bbc3fd0efd7313de9c4745606b5cf040b61915567a495d	checker::library_compiler::PrivateReplayProductionScopeForTest	glob-reexport
159	runtime_witness	impl:inherent for PrivateReplayProductionScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	2666:0..2690:1	102111..103066	e180a97df8e5165a713f202d685c00cac6a6982d0fbf4a09d004258b7b4f73c0	checker::library_compiler::runtime_witness::inherent for PrivateReplayProductionScopeForTest	keep-private
160	runtime_witness	fn:with_private_replay_production_control_for_test	private	# [cfg (any (test , feature = "test-utils"))]	2692:0..2701:1	103066..103395	db9adf9a55911c445e189462687ef62d090e147c9fe33128acee4cbce254d54d	checker::library_compiler::runtime_witness::with_private_replay_production_control_for_test	keep-private
161	runtime_witness	fn:private_replay_fault_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	2703:0..2713:1	103395..103770	88b79e3ee7bdd493933ada32cab026cc33c464572a32c9a8ce4ec3d245658731	checker::library_compiler::private_replay_fault_for_test	glob-reexport
162	runtime_witness	fn:private_replay_production_trace_active_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	2715:0..2718:1	103770..103985	395ce0afb19ee81d452c87cdc3fab082fa8585e4e41ce004b01e2159c736f88a	checker::library_compiler::private_replay_production_trace_active_for_test	glob-reexport
163	runtime_witness	fn:record_private_replay_trace_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	2720:0..2728:1	103985..104367	21bfc0181f5987e95a439b28028538dbec7f6d315767f5d8736461b3d0a14723	checker::library_compiler::record_private_replay_trace_for_test	glob-reexport
164	runtime_witness	struct:UserDeltaForkScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2730:0..2731:42	104367..104453	2b9639196af999df420317adf98aff6049d0902dd6523badb24204569756e750	checker::library_compiler::UserDeltaForkScopeForTest	glob-reexport
165	runtime_witness	impl:inherent for UserDeltaForkScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	2733:0..2742:1	104453..104697	a528f088bc8a4619fd74393e3f73364ab43108aa332801cac6754acaf114581b	checker::library_compiler::runtime_witness::inherent for UserDeltaForkScopeForTest	keep-private
166	runtime_witness	struct:UserSourceWorkForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2744:0..2750:1	104697..104893	b896147a0a451a398ed0d857bf16944f9ec5ed366c01db1324963192225b29bf	checker::library_compiler::UserSourceWorkForTest	glob-reexport
167	runtime_witness	fn:user_source_work_for_test	private	# [cfg (any (test , feature = "test-utils"))]	2752:0..2759:1	104893..105169	340119e118681d2fa79a3bf5e226e39048f0a038e61b3c4c194a36b3796ff5ff	checker::library_compiler::runtime_witness::user_source_work_for_test	keep-private
168	runtime_witness	fn:record_user_source_parses_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	2761:0..2768:1	105169..105434	6e02996d4f4deb92119159fdfa9039ac4c26b4c31c54501246141dd36ae117c2	checker::library_compiler::record_user_source_parses_for_test	glob-reexport
169	runtime_witness	fn:record_user_source_binds_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	2770:0..2777:1	105434..105696	e4060531cbc550e9208670d30e765914e356dba912b9effa975ff10fe48e0681	checker::library_compiler::record_user_source_binds_for_test	glob-reexport
170	runtime_witness	fn:record_user_source_checks_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	2779:0..2786:1	105696..105961	b41418429fa1ace29516a3bfc99e708021500e3798dbae3ed1bad5587c7086a7	checker::library_compiler::record_user_source_checks_for_test	glob-reexport
171	runtime_witness	struct:UserSourceWorkScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	2788:0..2789:61	105961..106066	af83397cea15c954fa47adf7644b45e354e69b78926942ec304b429d2b10392d	checker::library_compiler::UserSourceWorkScopeForTest	glob-reexport
172	runtime_witness	impl:inherent for UserSourceWorkScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	2791:0..2805:1	106066..106552	d8d204249aa064d038518017b9e09f40ce3691cd87d2a096cd5c969db7dcd3b4	checker::library_compiler::runtime_witness::inherent for UserSourceWorkScopeForTest	keep-private
173	runtime_witness	struct:OwnedBaseFinalIdentityWitness	pub	# [cfg (any (test , feature = "test-utils"))]	2807:0..2818:1	106552..107057	548b1ed8675c869864b95df49d870d16a35bf44b7f010ec6c1bb1fe160217826	checker::library_compiler::OwnedBaseFinalIdentityWitness	glob-reexport
174	runtime_witness	struct:OwnedBaseActualIds	pub	# [cfg (any (test , feature = "test-utils"))]	2820:0..2832:1	107057..107463	66534a1dc15d7314eeb650cf011b4fa36f62702f51fae1b3af96fc504849fa0c	checker::library_compiler::OwnedBaseActualIds	glob-reexport
175	runtime_witness	struct:OwnedBaseReferenceSummary	pub	# [cfg (any (test , feature = "test-utils"))]	2834:0..2840:1	107463..107680	09c67857cbd5cd64056a8ff4757df6a533f66cb1c806f5b9b196b1eca63b9a74	checker::library_compiler::OwnedBaseReferenceSummary	glob-reexport
176	runtime_witness	struct:OwnedBaseFinalIdentityEnds	pub	# [cfg (any (test , feature = "test-utils"))]	2842:0..2856:1	107680..108102	b4bdffa8e47161bfaab683baf976b725252cea0d392de6af455c68c0f20846b8	checker::library_compiler::OwnedBaseFinalIdentityEnds	glob-reexport
177	runtime_witness	struct:OwnedBaseReusedShapeWitness	pub	# [cfg (any (test , feature = "test-utils"))]	2858:0..2863:1	108102..108280	291ebe8157aaa28cc627d5ca62fb7c64ae1c45accb572582787cca9e2577fee1	checker::library_compiler::OwnedBaseReusedShapeWitness	glob-reexport
178	runtime_witness	struct:OwnedBaseContinuationWitness	pub	# [cfg (any (test , feature = "test-utils"))]	2865:0..2879:1	108280..108780	4466c5a7acf41a554fbd47e35206cf81f394a33bae0a01f6a44f6e4877b9692b	checker::library_compiler::OwnedBaseContinuationWitness	glob-reexport
179	runtime_witness	fn:store_prefix_digest	private	# [cfg (any (test , feature = "test-utils"))]	2881:0..2903:1	108780..109712	e0e5ef5561330cbbd15681d3283eba2da1a72cee849e7353fdbdcd3fdcdebb8f	checker::library_compiler::runtime_witness::store_prefix_digest	widen-sibling
180	runtime_witness	struct:FinalIdentityInspection	private	# [cfg (any (test , feature = "test-utils"))]	2905:0..2921:1	109712..110261	42985cdaecd1f86be08c643ed40d89943337ad9ee2329443c4fbe33af6990a37	checker::library_compiler::runtime_witness::FinalIdentityInspection	widen-sibling
181	runtime_witness	fn:common_base_domain_limit	private	# [cfg (any (test , feature = "test-utils"))]	2923:0..2937:1	110261..110761	074147b4c28f04808acc53232ab53fc2308ff3da50193cbbed748155649a7a92	checker::library_compiler::runtime_witness::common_base_domain_limit	keep-private
182	runtime_witness	fn:classify_live_reference_with_limits	private	# [cfg (any (test , feature = "test-utils"))]	2939:0..2959:1	110761..111530	4d5b21aa1be40c8cf2139940d9866b6b9700d9f391617b3c5eace0b85b5b75f4	checker::library_compiler::runtime_witness::classify_live_reference_with_limits	keep-private
183	runtime_witness	fn:classify_live_reference	private	# [cfg (any (test , feature = "test-utils"))]	2961:0..2977:1	111530..111974	a0ea7a5ee0edde808811c4757b11162ec86c7857bbc2d879e28818a80b1273e9	checker::library_compiler::runtime_witness::classify_live_reference	keep-private
184	runtime_witness	fn:classify_interner_live_reference	private	# [cfg (any (test , feature = "test-utils"))]	2979:0..3001:1	111974..112607	e3927a8eda924d70a9128a4d4f8e901ea873e5ed6963b41da27c165922bc24ff	checker::library_compiler::runtime_witness::classify_interner_live_reference	widen-sibling
185	runtime_witness	fn:classify_binder_live_reference	private	# [cfg (any (test , feature = "test-utils"))]	3003:0..3025:1	112607..113226	a5d3ad4c99d1c963ccbc21430595eb1e51a66c0b299795cd8afe40f0497ee6ca	checker::library_compiler::runtime_witness::classify_binder_live_reference	widen-sibling
186	runtime_witness	fn:final_reference_summary	private	# [cfg (any (test , feature = "test-utils"))]	3027:0..3188:1	113226..119814	f39a29af896ea858ecd9ea53b688291213d6686c83b4c48d80c078f4e00d7f5a	checker::library_compiler::runtime_witness::final_reference_summary	widen-sibling
187	runtime_witness	fn:final_identity_witness	private	# [cfg (any (test , feature = "test-utils"))]	3190:0..3372:1	119814..127014	0d9dd8d45d730dac5453904de7027d2e457a7d03c5650602ca49ebba5c66b767	checker::library_compiler::runtime_witness::final_identity_witness	widen-sibling
188	profile	struct:InjectedLibrarySource	pub	default	3374:0..3379:1	127014..127191	bbb6ece5dac852ecf5f50119e8d176c9fbcdc3c79f369d9a5fc7a23880dee26c	checker::library_compiler::InjectedLibrarySource	glob-reexport
189	profile	enum:InjectedProfileError	pub	default	3381:0..3402:1	127191..127831	8c00dc173e04a4e01b7a44c539b46d5d94715bfadb64f5af2a877b1444e320cc	checker::library_compiler::InjectedProfileError	glob-reexport
190	profile	struct:LibraryPhaseCounts	pub	default	3404:0..3412:1	127831..128111	c67f54c3dbddc96c197a961715dd05570a114bc9bf3b066651b8e6f0b63802b6	checker::library_compiler::LibraryPhaseCounts	glob-reexport
191	profile	struct:LibraryPhaseTimings	pub	# [cfg (any (test , feature = "test-utils"))]	3414:0..3422:1	128111..128395	3173bb398a47787b2ffc12fc9b265aed766b3ac6a1939183c0d2cd0222b5c45c	checker::library_compiler::LibraryPhaseTimings	glob-reexport
192	profile	impl:inherent for LibraryPhaseTimings	private	# [cfg (any (test , feature = "test-utils"))]	3424:0..3433:1	128395..128668	b533425b55f7a40c7cb5ae547f6c50d0b36ee52116bee10b1a710b99abd8f6b1	checker::library_compiler::profile::inherent for LibraryPhaseTimings	keep-private
193	profile	struct:TypeProbe	pub	# [cfg (any (test , feature = "test-utils"))]	3435:0..3442:1	128668..128947	bd2cd2320808b277dc08e65c864f65364132c2a326ebc450e242cde52bb7f8c5	checker::library_compiler::TypeProbe	glob-reexport
194	profile	struct:SignatureProbe	pub	# [cfg (any (test , feature = "test-utils"))]	3444:0..3449:1	128947..129126	fd91a7bd4d568f856779621d1f27e2bb1e971c99da8de950b5a3c13676e25da4	checker::library_compiler::SignatureProbe	glob-reexport
195	profile	struct:CallableMemberProbe	pub	# [cfg (any (test , feature = "test-utils"))]	3451:0..3461:1	129126..129470	b9e4281ec7ce0d86fd44636e1a1a6810bf689a055daa59e09bb0e13aca29cbcd	checker::library_compiler::CallableMemberProbe	glob-reexport
196	profile	struct:ValueProbe	pub	# [cfg (any (test , feature = "test-utils"))]	3463:0..3473:1	129470..129879	4357b178157fe513b874e613f27051cba0e42386d1069b0a14c36d61e144cb82	checker::library_compiler::ValueProbe	glob-reexport
197	profile	struct:InjectedProfileRun	pub	default	3475:0..3513:1	129879..131684	04c8c4f200b8233bc8d540318a602b4ae5fdd2be151a332d8777bec5849de2be	checker::library_compiler::InjectedProfileRun	glob-reexport
198	profile	struct:CanonicalLibraryEvidence	pub	# [cfg (any (test , feature = "test-utils"))]	3515:0..3526:1	131684..132231	72214a7065a4a5a86199507e2c6903a14216f11d9556bb6800b382c22dca3c51	checker::library_compiler::CanonicalLibraryEvidence	glob-reexport
199	profile	struct:CompleteSourceRouteWorkForTest	pub	# [cfg (any (test , feature = "test-utils"))]	3528:0..3541:1	132231..132740	179ccf2fb2c52b8b99c7ff9edf104e9524968ea92e6e69f5916ef7c5155ed0b4	checker::library_compiler::CompleteSourceRouteWorkForTest	glob-reexport
200	profile	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	3543:0..3548:1	132740..133031	c9fb2f4362843a12cb6b1f9f1e92ceef74e24eaf0d3a26d4b668f22980540c6d	checker::library_compiler::profile::thread_local	keep-private
201	profile	fn:record_complete_source_route_work_for_test	private	# [cfg (any (test , feature = "test-utils"))]	3550:0..3555:1	133031..133265	b125049289b69704b4c82fd04614b7ce0b915aaa430e245d58a0e09a103e3f02	checker::library_compiler::profile::record_complete_source_route_work_for_test	widen-sibling
202	profile	fn:record_complete_source_profile_load_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	3557:0..3562:1	133265..133497	2ebd32880899e00315358df2d0dbad6206e6ee3172d603ac8cf9623cb6999754	checker::library_compiler::record_complete_source_profile_load_for_test	glob-reexport
203	profile	fn:record_complete_source_auxiliary_parse_work_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	3564:0..3573:1	133497..133923	7d8d7bbe0c7ac6bd5637b77f16b5c86bcf1f978b580519fc7fac2eeb5a0deb97	checker::library_compiler::record_complete_source_auxiliary_parse_work_for_test	glob-reexport
204	profile	fn:record_complete_source_bind_for_test	private	# [cfg (any (test , feature = "test-utils"))]	3575:0..3585:1	133923..134406	dfeddf327395612c6213616542f7c313bb2e18fa7c31c4b0920b3e812bf3ea7d	checker::library_compiler::profile::record_complete_source_bind_for_test	widen-sibling
205	profile	fn:record_complete_source_publication_entry_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	3587:0..3592:1	134406..134685	12a0a5a7fd906bbd95c02db44bd26955eee8965c2f664bd8aee71927e9e8fc90	checker::library_compiler::record_complete_source_publication_entry_for_test	glob-reexport
206	profile	fn:record_replay_trace_construction_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	3594:0..3599:1	134685..134966	33555471e9735e52f514ec89d4f30e7fdb8d56331b66db56d05abe8f89609de9	checker::library_compiler::record_replay_trace_construction_for_test	glob-reexport
207	profile	fn:record_replay_plan_construction_for_test	pub (in crate :: check :: checker)	# [cfg (any (test , feature = "test-utils"))]	3601:0..3606:1	134966..135244	a2d1760687b157556e6c1a250124296f2d3bdc791419d1f40be6d1bd48dc75ce	checker::library_compiler::record_replay_plan_construction_for_test	glob-reexport
208	profile	fn:record_complete_source_evidence_for_test	private	# [cfg (any (test , feature = "test-utils"))]	3608:0..3613:1	135244..135581	4c0f39b77360019a3621b7b4fa191deba488e394ffc8e4baee19fc6212ced6dd	checker::library_compiler::profile::record_complete_source_evidence_for_test	widen-sibling
209	profile	fn:record_complete_source_profile_load_for_test	pub	# [cfg (not (any (test , feature = "test-utils")))]	3615:0..3616:56	135581..135686	1d85abea4a1a94505d29ae8ca5f6b46a79f010697be858dbb1ce72a499f62892	checker::library_compiler::record_complete_source_profile_load_for_test	glob-reexport
210	profile	fn:complete_source_route_thread_receipt_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	3618:0..3621:1	135686..135888	a1588f65a376dea091a81fb40c968a141c5304f1b4f73c337a396098436971b7	checker::library_compiler::complete_source_route_thread_receipt_for_test	glob-reexport
211	profile	fn:merge_complete_source_route_thread_receipt_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	3623:0..3657:1	135888..137502	df1c8ce83d3aefbbe3ed47aef285f6fa3abd2f867990674802f800099c01cc98	checker::library_compiler::merge_complete_source_route_thread_receipt_for_test	glob-reexport
212	profile	struct:CompleteSourceRouteWorkScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	3659:0..3663:1	137502..137669	9e088430681e7f73732b2bc859e324157186fa1abb8a9bae32f174e9bb05ad2d	checker::library_compiler::CompleteSourceRouteWorkScopeForTest	glob-reexport
213	profile	impl:inherent for CompleteSourceRouteWorkScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	3665:0..3707:1	137669..139585	957ee6f56358f9eb4babdfaeaabd4fc97073d512947fdfff3db7147650af814b	checker::library_compiler::profile::inherent for CompleteSourceRouteWorkScopeForTest	keep-private
214	profile	impl:inherent for InjectedProfileRun	private	default	3709:0..3738:1	139585..140524	0210b4778a710ab41c676fde7c9f6c6aa7c369f4509354504452b7c6ecdbce4a	checker::library_compiler::profile::inherent for InjectedProfileRun	keep-private
215	profile	struct:CanonicalInput	private	default	3740:0..3746:1	140524..140707	fb1181fc0cf3914854c625fcecf3b1083c4e2d58a504c7c928527d25a3cfdde1	checker::library_compiler::profile::CanonicalInput	widen-sibling
216	profile	fn:independent_event_owner_sites_for_oracle	private	# [cfg (any (test , feature = "test-utils"))]	3748:0..3772:1	140707..141901	6ec60f8eb2dbfbf7171df967eeab2ec390632ae323f25d08a9b67a88f72328e2	checker::library_compiler::profile::independent_event_owner_sites_for_oracle	widen-sibling
217	profile	fn:verify_independent_event_owner_sites	private	# [cfg (any (test , feature = "test-utils"))]	3774:0..3792:1	141901..142542	cebfc8e7a868c624b3522a14c7aa679db650d92362d01f95ff2afc6cf9082109	checker::library_compiler::profile::verify_independent_event_owner_sites	widen-sibling
218	profile	struct:CanonicalLibraryFrontend	private	default	3794:0..3812:1	142542..143350	915547648c02f20f6257389925a2282bb77833fa329896a35ba273be0f3a5552	checker::library_compiler::profile::CanonicalLibraryFrontend	widen-sibling
219	profile	enum:TrustedLibraryMarkerShape	private	default	3814:0..3819:1	143350..143471	749edf4fecc89fc116a1c3e3f2164950c6383b01fab5ae98b620c7181b8b8c97	checker::library_compiler::profile::TrustedLibraryMarkerShape	keep-private
220	profile	fn:exact_type_parameter	private	default	3821:0..3841:1	143471..144186	681b0adb48d82483862427a682ab6ac11a9786f8af368a81725b0c15d8e24fbc	checker::library_compiler::profile::exact_type_parameter	keep-private
221	profile	fn:is_string_keyword	private	default	3843:0..3845:1	144186..144310	79b20b887427867f18e33b1de23a098b1d9ab14941f1d2013c7abaa6ead00ca4	checker::library_compiler::profile::is_string_keyword	keep-private
222	profile	fn:exact_plain_type_reference	private	default	3847:0..3856:1	144310..144687	ab4339fc14b4abba74283bba22003cf0aa5b9b15c9b8d19b98aad5f2f8ae251b	checker::library_compiler::profile::exact_plain_type_reference	keep-private
223	profile	fn:exact_unary_type_reference	private	default	3858:0..3875:1	144687..145264	bfe42dc096535eb216be9a9cf6136375af2be527d7b8b1608a9e0d44b652ce3d	checker::library_compiler::profile::exact_unary_type_reference	keep-private
224	profile	fn:exact_infer_type	private	default	3877:0..3888:1	145264..145669	77a25d212ae86dd56700fc0fef0b11d76b73664d684c60d000deca7be34ed0ad	checker::library_compiler::profile::exact_infer_type	keep-private
225	profile	fn:exact_rest_function	private	default	3890:0..3916:1	145669..146674	81cb0bb3864c5b3971ee8e77bc4496f06e272167ccdca26ec46f423b4b7fe18c	checker::library_compiler::profile::exact_rest_function	keep-private
226	profile	fn:exact_omit_this_parameter_body	private	default	3918:0..3945:1	146674..147893	c8223b11f54099c5706e924603d894b47a552c7a6a7a7f95781daae70ef54dff	checker::library_compiler::profile::exact_omit_this_parameter_body	keep-private
227	profile	fn:seed_trusted_library_markers	private	default	3947:0..4050:1	147893..151247	5a826ff4d5068926d161e7f22cd24274dfe11d3fc18ff615a808ea91e33ca4e4	checker::library_compiler::profile::seed_trusted_library_markers	widen-sibling
228	profile	fn:certify_library_values	private	default	4052:0..4060:1	151247..151584	a508bb2b28589a3319bfac4d0818c0b25aed679a23ee4d1dd6a19089eaae2263	checker::library_compiler::profile::certify_library_values	widen-sibling
229	profile	macro:thread_local	private	# [cfg (any (test , feature = "test-utils"))]	4062:0..4070:1	151584..152257	40e9b9a7b0e896d0636c937a995871d9cf727b7d1af893fc546de891278b6aa9	checker::library_compiler::profile::thread_local	keep-private
230	profile	struct:CanonicalLibraryFrontendWorkForTest	pub	# [cfg (any (test , feature = "test-utils"))]	4072:0..4081:1	152257..152592	85dc991b38868fe1863dca34e2db4a25df5c9e4aa91645647e1402d03647845c	checker::library_compiler::CanonicalLibraryFrontendWorkForTest	glob-reexport
231	profile	fn:canonical_library_frontend_work_for_test	private	# [cfg (any (test , feature = "test-utils"))]	4083:0..4093:1	152592..153163	823787dc9b840d739b7294956b32e29154f8593ecee0659cb0e627ca70f0f06d	checker::library_compiler::profile::canonical_library_frontend_work_for_test	keep-private
232	profile	struct:CanonicalLibraryFrontendWorkScopeForTest	pub	# [cfg (any (test , feature = "test-utils"))]	4095:0..4096:89	153163..153296	c1b0ff11a2dfdd997517e20a778133c8892bc3d95715f25e09e64f9a5a4d3707	checker::library_compiler::CanonicalLibraryFrontendWorkScopeForTest	glob-reexport
233	profile	impl:inherent for CanonicalLibraryFrontendWorkScopeForTest	private	# [cfg (any (test , feature = "test-utils"))]	4098:0..4119:1	153296..154294	b2946cd2e9b9bba9d0483cdaad80b6da28a29ea44b4e443ab3b9dc1b87fa72f9	checker::library_compiler::profile::inherent for CanonicalLibraryFrontendWorkScopeForTest	keep-private
234	canonical_bytes	struct:ParserExportClaim	private	default	4121:0..4125:1	154294..154423	ec294a0d07d1ec6a728590544002956f127298308d3f4c6228f1fa2151b84ef4	checker::library_compiler::canonical_bytes::ParserExportClaim	widen-sibling
235	canonical_bytes	struct:CanonicalBytes	private	default	4127:0..4127:31	154423..154456	8af06826799e61c1c620c340e31c0b914fa1d64f54da553663ee530002498c6e	checker::library_compiler::canonical_bytes::CanonicalBytes	widen-sibling
236	canonical_bytes	impl:inherent for CanonicalBytes	private	default	4129:0..4208:1	154456..156643	b393f945ae1fdd291eeb6062089000fae60a3247a0c8f20ea5282834903014d3	checker::library_compiler::canonical_bytes::inherent for CanonicalBytes	keep-private
237	canonical_bytes	fn:intrinsic_code	private	# [cfg (any (test , feature = "test-utils"))]	4210:0..4233:1	156643..157476	6ba19166a917fe5308a8e6f52b31cc16a7c020bfe3e2ed95a295ff28e67f705e	checker::library_compiler::canonical_bytes::intrinsic_code	keep-private
238	canonical_bytes	fn:visibility_code	private	# [cfg (any (test , feature = "test-utils"))]	4235:0..4242:1	157476..157704	e64cf0ebc986b5ef25bdc2464266a630ff0ff4ab92a58926ed4e26498185c2b3	checker::library_compiler::canonical_bytes::visibility_code	keep-private
239	canonical_bytes	fn:well_known_symbol_code	private	# [cfg (any (test , feature = "test-utils"))]	4244:0..4260:1	157704..158322	e1ece44052333d4d9313ccdcc9783000ebd0f7d027cbdae2c1e505633172eeae	checker::library_compiler::canonical_bytes::well_known_symbol_code	keep-private
240	canonical_bytes	fn:modifier_code	private	# [cfg (any (test , feature = "test-utils"))]	4262:0..4269:1	158322..158535	f54d9770141a2437db01feb9661d442f68d504dcfb107b57e39138faf7efebc3	checker::library_compiler::canonical_bytes::modifier_code	keep-private
241	canonical_bytes	fn:encode_store_row	private	# [cfg (any (test , feature = "test-utils"))]	4271:0..4497:1	158535..168471	bcc051677204e5ffd73f2bb506b282311efa18ecbde4da020272c7d33ee38e89	checker::library_compiler::canonical_bytes::encode_store_row	widen-sibling
242	canonical_bytes	fn:encode_enum_value	private	# [cfg (any (test , feature = "test-utils"))]	4499:0..4516:1	168471..168949	6f1edb2e0e6a887b6f52eb102e2eda78e86ae605fb9e98b33f47d7690669b676	checker::library_compiler::canonical_bytes::encode_enum_value	keep-private
243	canonical_bytes	fn:encode_declared_recipe_row	private	# [cfg (any (test , feature = "test-utils"))]	4518:0..4575:1	168949..170770	86fa690dfd10f0ca68f943e9345903e02346556a53434c6479ec6fbfcca6d789	checker::library_compiler::canonical_bytes::encode_declared_recipe_row	widen-sibling
244	canonical_bytes	fn:canonical_input_for_record	private	default	4577:0..4589:1	170770..171189	f497e8134b2a34eb903fb0c45b34f1e2789ae2e56d6b1131bd4b902c81d7b2e9	checker::library_compiler::canonical_bytes::canonical_input_for_record	widen-sibling
245	canonical_bytes	fn:encode_library_key	private	# [cfg (any (test , feature = "test-utils"))]	4591:0..4601:1	171189..171518	e0e5dd71f276720171523e75a501bd947c626c50b2e7075c6b9040c690a9919e	checker::library_compiler::canonical_bytes::encode_library_key	keep-private
246	canonical_bytes	fn:canonical_record_bytes	private	default	4603:0..4638:1	171518..172886	810ab59a767a284cb2918d7a3a113986dcebccd39c0e51cf7245fb7b51affbdf	checker::library_compiler::canonical_bytes::canonical_record_bytes	widen-sibling
247	canonical_bytes	fn:full_structured_record_baseline	private	# [cfg (any (test , feature = "test-utils"))]	4640:0..4695:1	172886..174987	9ec19f98e9c5a9b6eb3eea14857302db206de8b01ff500bcfc13c1a24ab29eb4	checker::library_compiler::canonical_bytes::full_structured_record_baseline	widen-sibling
248	canonical_bytes	fn:canonical_library_evidence_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	4697:0..4741:1	174987..176962	c543ffbe3549240aa3ef69343989def4caa12dd1d9995ba28e6754e81c2b684a	checker::library_compiler::canonical_library_evidence_for_test	glob-reexport
249	terminal_validation	struct:ReplayTerminalValidationInputs	private	default	4743:0..4750:1	176962..177297	770847ff277b967019690b09b325ae6d4378898041806c0e4f3c7e3422674891	checker::library_compiler::terminal_validation::ReplayTerminalValidationInputs	widen-sibling
250	terminal_validation	struct:TerminalClassDependencyValidationWork	private	# [cfg (any (test , feature = "test-utils"))]	4752:0..4761:1	177297..177628	e893c592b51c77c9b56e194b8e1390878e7a2921bc20f529dbf0c51ceb318af3	checker::library_compiler::terminal_validation::TerminalClassDependencyValidationWork	widen-sibling
251	terminal_validation	fn:intern_terminal_semantic_node	private	default	4763:0..4779:1	177628..178131	d4d53306cbf3921ff6db8a72f42406f416b34e82d0283780163c0524d5a9d4a0	checker::library_compiler::terminal_validation::intern_terminal_semantic_node	keep-private
252	terminal_validation	enum:TerminalOwnerExpression	private	default	4781:0..4784:1	178131..178212	b6cf5b34f419ccbe644734c9e39bb7d021d80ef353b9eff6a966de0f50b58ea7	checker::library_compiler::terminal_validation::TerminalOwnerExpression	keep-private
253	terminal_validation	fn:terminal_expression_owners	private	default	4786:0..4822:1	178212..179977	628aeb3c34fb7ab7620e9640b6ae058fa63c9a250abd9109670b2605ed0921ae	checker::library_compiler::terminal_validation::terminal_expression_owners	keep-private
254	terminal_validation	fn:record_terminal_semantic_nodes	private	# [cfg (any (test , feature = "test-utils"))]	4824:0..4834:1	179977..180344	8f5342d5aee95245cd05c15a6684d4412f6fa91e50f92137245aeabe4e673d04	checker::library_compiler::terminal_validation::record_terminal_semantic_nodes	keep-private
255	terminal_validation	fn:record_terminal_semantic_edges	private	# [cfg (any (test , feature = "test-utils"))]	4836:0..4846:1	180344..180711	40f483ce5cc2dd6e4a2c2e012aee53fb70187ea725305c9702e1843a1f12f342	checker::library_compiler::terminal_validation::record_terminal_semantic_edges	keep-private
256	terminal_validation	fn:record_terminal_class_edges	private	# [cfg (any (test , feature = "test-utils"))]	4848:0..4858:1	180711..181069	f33d2d0fb5d5fd00219a9a364f960d9991c267cfe42c5618abdfc2c147c0ca13	checker::library_compiler::terminal_validation::record_terminal_class_edges	keep-private
257	terminal_validation	fn:record_terminal_class_summary_items	private	# [cfg (any (test , feature = "test-utils"))]	4860:0..4870:1	181069..181439	8b3e61b98678c1c94d01a4b8e9dc4e1fdc2119bc9c087d44e7cb6139661a1539	checker::library_compiler::terminal_validation::record_terminal_class_summary_items	keep-private
258	terminal_validation	fn:record_terminal_owner_expression_visits	private	# [cfg (any (test , feature = "test-utils"))]	4872:0..4882:1	181439..181821	642cf9d961bed75ba7062cda647f1719791abf8427c3f1a3aceba870c1ad4cf3	checker::library_compiler::terminal_validation::record_terminal_owner_expression_visits	keep-private
259	terminal_validation	fn:require_terminal_class_dependency_closure	private	default	4884:0..5159:1	181821..192343	baecee4dad2c67cb6bc451a30149fb5060e9f7a45450e9be4cde6e914aed2725	checker::library_compiler::terminal_validation::require_terminal_class_dependency_closure	widen-sibling
260	terminal_validation	fn:validate_terminal_class_dependencies	private	default	5161:0..5398:1	192343..202084	b548887de90815d3fc8857355c118e9e95de4fffb39fd0739e9745c7dfade91f	checker::library_compiler::terminal_validation::validate_terminal_class_dependencies	widen-sibling
261	terminal_validation	fn:validate_root_census	private	default	5400:0..5440:1	202084..203584	42e6dc7af2abb749daba581ff4a89cf7257fdfff326236242f1392a4ef62621f	checker::library_compiler::terminal_validation::validate_root_census	widen-sibling
262	collision_plan	struct:NormalizedSourceRootCandidates	private	default	5442:0..5445:1	203584..203765	132c03625bc6de68d1b835771cdfd0d7d08f21285130454261b8d195aea15421	checker::library_compiler::collision_plan::NormalizedSourceRootCandidates	keep-private
263	collision_plan	fn:apply_merge_root_semantics	private	default	5447:0..5458:1	203765..204202	0d299024fd3dd17f95053f0037fed6291156a6dd75afcc390965ef1f65915e9e	checker::library_compiler::collision_plan::apply_merge_root_semantics	widen-sibling
264	collision_plan	fn:normalize_source_root_candidates	private	default	5460:0..5521:1	204202..206678	ac57bf03a5924f6f4b63b45f942747f0b9ca8f7b0feb101ca3fa9ab5cec83943	checker::library_compiler::collision_plan::normalize_source_root_candidates	keep-private
265	collision_plan	fn:build_collision_replay_plan	private	default	5523:0..5644:1	206678..211950	86fe7f74ce4a60d01057490f1b1929d4bb0633cb43d676de4b2fc708f6ae11c2	checker::library_compiler::collision_plan::build_collision_replay_plan	widen-sibling
266	collision_plan	fn:build_collision_replay_index	private	default	5646:0..6057:1	211950..228655	ae3a9422e0c36de7b443c054f3b4eae3cdbcc818e51d4fb9d633eff52d390e6c	checker::library_compiler::collision_plan::build_collision_replay_index	widen-sibling
267	collision_plan	fn:collision_declaration_provenance	private	# [cfg (any (test , feature = "test-utils"))]	6059:0..6082:1	228655..229579	a979eec83e23a53d6a8876282c3db5425de99b3c41f7a0cc8073bcba75a00f9e	checker::library_compiler::collision_plan::collision_declaration_provenance	keep-private
268	complete_source	fn:run_injected_profile	pub	# [cfg (any (test , feature = "test-utils"))]	6084:0..6089:1	229579..229816	f48c10247f0582f2be43f7a2cc9e5c13cb6c15e0f14187e645fdc516a5592ea2	checker::library_compiler::run_injected_profile	glob-reexport
269	complete_source	fn:compile_complete_combined_profile_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	6091:0..6106:1	229816..230436	aa685d2f4136f9d5995c74daab8f1cf7e75a0cf78966eed1bc785a95283f7be8	checker::library_compiler::compile_complete_combined_profile_for_test	glob-reexport
270	complete_source	fn:compile_complete_source_project_programs	pub	default	6108:0..6115:1	230436..230905	91f17cbb96a974ae610f371c58c8b914993e90558785e4863464a50f72f8596f	checker::library_compiler::compile_complete_source_project_programs	glob-reexport
271	complete_source	fn:compile_complete_source_project_programs_with_source_reexports	pub	default	6117:0..6131:1	230905..231515	97231eb69e0f62d055fddc81d1d6bd7d93c11f8c25ae6e3a2b8a3803707bf1b3	checker::library_compiler::compile_complete_source_project_programs_with_source_reexports	glob-reexport
272	complete_source	fn:compile_complete_source_project_programs_with_default_modules	pub	default	6133:0..6148:1	231515..232206	5b70a0d88e0a21a17d0058e45fc38b81278b450c1386a1b6679297d5d183b663	checker::library_compiler::compile_complete_source_project_programs_with_default_modules	glob-reexport
273	complete_source	fn:compile_complete_source_project_programs_core	private	default	6150:0..6182:1	232206..233536	46f4c63bbc28006ca7b0a78826085a038b815e483b3f803d51856540e142e95f	checker::library_compiler::complete_source::compile_complete_source_project_programs_core	keep-private
274	complete_source	fn:complete_source_project_frontend	private	default	6184:0..6487:1	233536..245818	38c98594c9369d05726e3fcd1328b273e81af53415f37fd56266423c7e1e8264	checker::library_compiler::complete_source::complete_source_project_frontend	keep-private
275	complete_source	struct:CompleteCombinedOracleForTest	pub	# [cfg (any (test , feature = "test-utils"))]	6489:0..6495:1	245818..246106	041730b17842138089fa1dcc6e45e545f6bf6933d1f6ad4590b955607bafe399	checker::library_compiler::CompleteCombinedOracleForTest	glob-reexport
276	complete_source	fn:compile_complete_combined_oracle_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	6497:0..6543:1	246106..248125	b3b2e0c70f128a031401163e854726641ac8e1f3d9a3b6446bd5c757ec9d268b	checker::library_compiler::compile_complete_combined_oracle_for_test	glob-reexport
277	complete_source	fn:with_canonical_library_frontend	private	default	6545:0..6552:1	248125..248457	93618db2db8ca3aa2f360a2950462b2922d871af07b0b329b2ac9717f989669c	checker::library_compiler::complete_source::with_canonical_library_frontend	widen-sibling
278	complete_source	fn:with_canonical_frontend	private	default	6554:0..6830:1	248457..260064	c7dba356870ba150229ae3adca42189c55640fc9fac3b9b4dcedb57ce30d78a2	checker::library_compiler::complete_source::with_canonical_frontend	keep-private
279	compile_api	enum:ReplayIndexPlan	pub	default	6832:0..6844:1	260064..260621	29c6ad590ce89aebc4573073ecc841eb4fff36c8b37f292124aa7a24196e12d9	checker::library_compiler::ReplayIndexPlan	glob-reexport
280	compile_api	enum:LibraryRecordRetention	pub	default	6846:0..6858:1	260621..261338	85205c6e0a5635c036110347c8b8fd0552b72f148d980d9bfc339c4ad0455b0e	checker::library_compiler::LibraryRecordRetention	glob-reexport
281	compile_api	enum:CanonicalLibraryEvidenceCapture	private	default	6860:0..6864:1	261338..261448	737d935ebbba39d9806ecfe2341881afb6ad054a77dd3b3057939c0e258099b3	checker::library_compiler::compile_api::CanonicalLibraryEvidenceCapture	widen-sibling
282	compile_api	fn:compile_owned_injected_profile	pub	default	6866:0..6880:1	261448..262069	ce3b61bfc0fdf872dcb7327491c50cd107ce79ebe0dfa8ade4a691ecc2c19e5c	checker::library_compiler::compile_owned_injected_profile	glob-reexport
283	compile_api	fn:compile_complete_source_replay_runtime_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	6882:0..6895:1	262069..262736	ad08dd6ea02bf85e46f69908d2815d12c59135d0caec8fbd257475aa69064ae1	checker::library_compiler::compile_complete_source_replay_runtime_for_test	glob-reexport
284	compile_api	fn:compile_complete_source_replay_runtime	pub	default	6897:0..6914:1	262736..263540	f873d71fc8247dcdfafc2213e09601475581332b2532798b1f17b3a1f7ab43b8	checker::library_compiler::compile_complete_source_replay_runtime	glob-reexport
285	compile_api	fn:compile_owned_injected_base_profile	pub	default	6916:0..6924:1	263540..264093	dcea482b8341462c2bb0a17958b257403e234ad49a36f0e69d73b0177b6fe233	checker::library_compiler::compile_owned_injected_base_profile	glob-reexport
286	compile_api	fn:compile_owned_injected_base_profile_with_plan	pub	default	6926:0..6952:1	264093..264970	1bad815885f81331e2bbe2727df67660ee39eb142a79bcda571505710ece2f2d	checker::library_compiler::compile_owned_injected_base_profile_with_plan	glob-reexport
287	compile_api	fn:compile_owned_injected_base_profile_with_ordered_owner_sites_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	6954:0..6981:1	264970..265916	169dc863241ad8964a6e8d278bbb69c40abdbb38ea1462742c31a0d5b0362ca4	checker::library_compiler::compile_owned_injected_base_profile_with_ordered_owner_sites_for_test	glob-reexport
288	compile_api	fn:compile_owned_injected_base_profile_with_nested_owner_sites_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	6983:0..7010:1	265916..266860	07ccf31e43c37d9593f7932babcd0f45eb7ea468221f835ba69f2040357a9878	checker::library_compiler::compile_owned_injected_base_profile_with_nested_owner_sites_for_test	glob-reexport
289	compile_api	fn:force_collision_plan_failure_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	7012:0..7059:1	266860..268769	582ae7f95e2588d31e4ae22922f6d3b1aac4367acb9069cbb88ae83b0de769ab	checker::library_compiler::force_collision_plan_failure_for_test	glob-reexport
290	compile_api	fn:compile_owned_injected_records	pub	default	7061:0..7078:1	268769..269534	f55741d98e3db0578b282f8ad188b01375e4c7406743a3f60143bafa6f38d455	checker::library_compiler::compile_owned_injected_records	glob-reexport
291	compile_api	fn:intern_compiled_global_object	private	default	7080:0..7120:1	269534..270962	1961a82518cf1ecc47178df2ae749b8e7394f9e32f94b9a5f6beb453acdb7da2	checker::library_compiler::compile_api::intern_compiled_global_object	widen-sibling
292	compile_api	trait:InjectedCompileRoute	private	default	7122:0..7172:1	270962..272994	d54305dd82c2810a66d8b2fbf6a364b6e7b83186d46fa92919522c373fec2d60	checker::library_compiler::compile_api::InjectedCompileRoute	widen-sibling
293	compile_api	struct:LegacyLibraryRoute	private	default	7174:0..7174:26	272994..273022	9b91e121c4088fe4003753bac080f7bba09453fac786dcf34a15c63917957b7b	checker::library_compiler::compile_api::LegacyLibraryRoute	keep-private
294	compile_api	impl:InjectedCompileRoute for LegacyLibraryRoute	private	default	7176:0..7266:1	273022..276274	ad857d869d117fe665607dada1d8f24e25f7f02db27b221b057e2c7d6ec58b82	checker::library_compiler::compile_api::InjectedCompileRoute for LegacyLibraryRoute	keep-private
295	compile_api	struct:CompleteSourceRoute	private	default	7268:0..7268:27	276274..276303	a219e8f45992f44ead4298e2c144d57c440a6976f8a2ba533ff4ceb316791fb6	checker::library_compiler::compile_api::CompleteSourceRoute	keep-private
296	compile_api	impl:InjectedCompileRoute for CompleteSourceRoute	private	default	7270:0..7383:1	276303..280421	31f7ad29a0f2c55683383e8beeb66338cd88953ef352b4448487dea100048fc9	checker::library_compiler::compile_api::InjectedCompileRoute for CompleteSourceRoute	keep-private
297	compile_api	fn:compile_owned_injected_frontend	private	default	7385:0..7416:1	280421..281396	6a0131684d312c435d1fe26ecee0381ba42318c0674b95fce5ff04aa442867f9	checker::library_compiler::compile_api::compile_owned_injected_frontend	widen-sibling
298	compile_api	fn:validate_complete_source_user_units	private	default	7418:0..7439:1	281396..282226	a08b546d47cd524b610f0c992f0b2bb1da2094ffbdedb1833472632534363d0e	checker::library_compiler::compile_api::validate_complete_source_user_units	widen-sibling
299	compile_api	fn:complete_source_user_file_ordinals_for_test	private	# [cfg (any (test , feature = "test-utils"))]	7441:0..7512:1	282226..285313	87b939c09dd76abdb9601b6e0681ef926678244660eb028bed4d06755518d8e9	checker::library_compiler::compile_api::complete_source_user_file_ordinals_for_test	widen-sibling
300	compile_api	fn:assemble_complete_source_user_results	private	default	7514:0..7542:1	285313..286501	c46151e45876b16ecab1de4cbfed6b3c9461c3bfd3cb1f7ee03d9083ddaf6f94	checker::library_compiler::compile_api::assemble_complete_source_user_results	widen-sibling
301	compile_engine	fn:compile_owned_injected_frontend_for_route	private	default	7544:0..8536:1	286501..327634	318287f72a45fee88ed5bb3b472c8cef3172d1f089f28e5487f5e0df6f6a6cfa	checker::library_compiler::compile_engine::compile_owned_injected_frontend_for_route	widen-sibling
302	continuation	fn:compile_library_binder_checkpoint	pub	default	8538:0..8586:1	327634..329407	8282421c67d420adcd0b8d51be88dbb367456f80c66079dba1cba4b285220849	checker::library_compiler::compile_library_binder_checkpoint	glob-reexport
303	continuation	fn:build_library_binder_checkpoint	private	default	8588:0..8593:1	329407..329596	9ac2868a23840a8f0c03dd91827ed3eab0619ef545828a6eeda00d08d2a5221e	checker::library_compiler::continuation::build_library_binder_checkpoint	widen-sibling
304	continuation	struct:BinderContinuationModuleSourceForTest	pub	# [cfg (any (test , feature = "test-utils"))]	8595:0..8600:1	329596..329819	bdd90f07d8015d58011c2cba227c275ecfec5352344728147f0116434ccd3930	checker::library_compiler::BinderContinuationModuleSourceForTest	glob-reexport
305	continuation	struct:LibraryBinderContinuationForTest	pub	# [cfg (any (test , feature = "test-utils"))]	8602:0..8621:1	329819..330726	11c1b39dd83fec90802cd793d588d098816fb6113ce6417c62b26e3bca6b37f2	checker::library_compiler::LibraryBinderContinuationForTest	glob-reexport
306	continuation	struct:ProjectBindingLookupForTest	pub	# [cfg (any (test , feature = "test-utils"))]	8623:0..8631:1	330726..331037	88410588ca70de2159f7a9dfe71bd197ecef8f81e0731d2af3b06108c2bdf404	checker::library_compiler::ProjectBindingLookupForTest	glob-reexport
307	continuation	impl:inherent for LibraryBinderContinuationForTest	private	# [cfg (any (test , feature = "test-utils"))]	8633:0..8717:1	331037..333763	94132f0a5b6b523a2b325072a1f2bc80ddb785ab5c15992ec56f2c39c137bbd0	checker::library_compiler::continuation::inherent for LibraryBinderContinuationForTest	keep-private
308	continuation	fn:continue_library_project_binder	pub	default	8719:0..8735:1	333763..334451	123d4067a70e2f8a8533251cc2feb4b43d4f8d380d15e2439ae70cd80d888cde	checker::library_compiler::continue_library_project_binder	glob-reexport
309	continuation	fn:continuation_receipt_for_test	pub	# [cfg (any (test , feature = "test-utils"))]	8737:0..8820:1	334451..338136	b2522712452a39ed6c33b524874703f9bd0c3cc9e3cd54b1480d472d0fa1dccd	checker::library_compiler::continuation_receipt_for_test	glob-reexport
310	continuation	fn:check_caller_certified_collision_free_project_with_owned_library	pub	# [cfg (any (test , feature = "test-utils"))]	8822:0..8917:1	338136..342983	8c2fc1cc37528114de12e5eedc0c74c56a80243f1d8c0b084daa715ce4196db2	checker::library_compiler::check_caller_certified_collision_free_project_with_owned_library	glob-reexport
311	continuation	fn:check_caller_certified_collision_free_source_with_owned_library	pub	# [cfg (any (test , feature = "test-utils"))]	8919:0..8925:1	342983..343296	d262471f0093e800e96879b193d48197b3f69cda6e32d9eab4069256d9d6b68b	checker::library_compiler::check_caller_certified_collision_free_source_with_owned_library	glob-reexport
312	continuation	fn:check_caller_certified_collision_free_source_with_base_evidence	pub	# [cfg (any (test , feature = "test-utils"))]	8927:0..8939:1	343296..343709	9513f4a6a50ac56f4e07743367602f721aa66debaa3883d753bca6e6d9108d72	checker::library_compiler::check_caller_certified_collision_free_source_with_base_evidence	glob-reexport
313	continuation	fn:check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix	private	# [cfg (test)]	8941:0..8947:1	343709..344006	2fc88cf59e7fb4971e8b8646ca902fd3ff370543363d71b8cc5f84439514eb49	checker::library_compiler::continuation::check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix	widen-sibling
314	continuation	fn:check_caller_certified_collision_free_source_with_owned_library_impl	private	# [cfg (any (test , feature = "test-utils"))]	8949:0..9119:1	344006..351348	00b928b5c66d437bc253b8ed73d4a2ec196aa8259ead8c4980c09e4abb3e57cf	checker::library_compiler::continuation::check_caller_certified_collision_free_source_with_owned_library_impl	keep-private
315	probes	fn:validate_parser_export_claims	private	default	9121:0..9149:1	351348..352492	4524c8570fec20eb1ecc82993450ff6990c0eb838f7ee5cc401f7c0e938614c2	checker::library_compiler::probes::validate_parser_export_claims	widen-sibling
316	probes	fn:match_parser_export_claims	private	default	9151:0..9180:1	352492..353590	f3ba409b487122f1e928276cc459f4d715efdb9f9621be30fa75908438b04be7	checker::library_compiler::probes::match_parser_export_claims	widen-sibling
317	probes	fn:canonical_inputs	private	default	9182:0..9223:1	353590..355103	14b3f3b80617910d65ff49e9bdb876831c7a0ffef78ab16ad016cc2667327f80	checker::library_compiler::probes::canonical_inputs	widen-sibling
318	probes	fn:collect_type_probes	private	# [cfg (any (test , feature = "test-utils"))]	9225:0..9260:1	355103..356495	e4c6be02621f3ddf05b9f2d6205c7154baf611329cce75f9a455000d2902faa2	checker::library_compiler::probes::collect_type_probes	widen-sibling
319	probes	fn:type_probe	private	# [cfg (any (test , feature = "test-utils"))]	9262:0..9317:1	356495..358352	dd798748b0bb63317c2b805381a1a09cb20388d11309c369b311ba7c219b6e8f	checker::library_compiler::probes::type_probe	widen-sibling
320	probes	fn:collect_value_probes	private	# [cfg (any (test , feature = "test-utils"))]	9319:0..9348:1	358352..359208	22a1bb2a31b5d997202dfb9e776aae65bd0db81b5649b8aca210263f66650031	checker::library_compiler::probes::collect_value_probes	widen-sibling
321	probes	fn:value_probe_for_symbol	private	# [cfg (any (test , feature = "test-utils"))]	9350:0..9427:1	359208..362325	e51edb6b656e684abe2b2ba6943e162d6d84877e5275d4a30ae5e0d671261468	checker::library_compiler::probes::value_probe_for_symbol	keep-private
322	probes	fn:signature_ids	private	# [cfg (any (test , feature = "test-utils"))]	9429:0..9439:1	362325..362681	039775cda5ea54a6fba608033d905765dfbf401584c97204b299910e39568c3f	checker::library_compiler::probes::signature_ids	keep-private
323	probes	fn:signature_probe	private	# [cfg (any (test , feature = "test-utils"))]	9441:0..9452:1	362681..363111	6e8b2cef1ddbcb342435fd40267e13dbcc9327a1e892d67545748ebc3cf24fa1	checker::library_compiler::probes::signature_probe	keep-private
324	probes	fn:origin_for_module	private	# [cfg (any (test , feature = "test-utils"))]	9454:0..9461:1	363111..363397	35e19cc37a6461817ecc4aabda37ba3e3692e095f80eca3f2efd6b38c608ac80	checker::library_compiler::probes::origin_for_module	keep-private
325	probes	fn:library_ordinal	private	# [cfg (any (test , feature = "test-utils"))]	9463:0..9469:1	363397..363661	8b8af92cad4693c9c6cad25e36c22f6be91331fa4c778305fc52d745792485d1	checker::library_compiler::probes::library_ordinal	keep-private
326	probes	fn:release_phase_line	private	# [cfg (any (test , feature = "test-utils"))]	9471:0..9488:1	363661..364331	703b69f3e6e2e5bf6250e45f8592442a8f14015421f3145cccbb769a802615be	checker::library_compiler::probes::release_phase_line	widen-sibling
327	probes	struct:ReleaseOutcomeLine	private	# [cfg (any (test , feature = "test-utils"))]	9490:0..9502:1	364331..364691	c17615f8a18cda7c522bef60b381b6d58f699f81d6e0e0d4bf305be61aa6d347	checker::library_compiler::probes::ReleaseOutcomeLine	widen-sibling
328	probes	impl:inherent for ReleaseOutcomeLine	private	# [cfg (any (test , feature = "test-utils"))]	9504:0..9521:1	364691..365411	bee1608250be46ba2e8da64a261a7bd2c83a7ab5a71ea13ac107a25634571389	checker::library_compiler::probes::inherent for ReleaseOutcomeLine	keep-private
329	probes	fn:assert_exact_profile_interner_has_no_pending_reservations	pub	# [cfg (any (test , feature = "test-utils"))]	9523:0..9533:1	365411..365822	89edd487e67d5ef4d39f482f76540b031c9c96557a0c531ad4f570f4121633e4	checker::library_compiler::assert_exact_profile_interner_has_no_pending_reservations	glob-reexport
330	probes	fn:assert_exact_profile_replay_index_is_complete_and_deterministic	pub	# [cfg (any (test , feature = "test-utils"))]	9535:0..9585:1	365822..367746	a03c789ff4848392ff8ae12a916e346f4c3f4e9084d4a9d885b1d610bcd23909	checker::library_compiler::assert_exact_profile_replay_index_is_complete_and_deterministic	glob-reexport
331	probes	fn:assert_exact_profile_selects_complete_native_bridge_identities	pub	# [cfg (any (test , feature = "test-utils"))]	9587:0..9612:1	367746..368851	7b9115cf3657c6b2624d1cffd9e90a927246032130496b9e260f07f60f55fdfb	checker::library_compiler::assert_exact_profile_selects_complete_native_bridge_identities	glob-reexport
332	probes	fn:run_library_release_probe	pub	# [cfg (any (test , feature = "test-utils"))]	9614:0..9696:1	368851..371845	5313bfa6cca13437418c59ef28738916d1cd394d32cde969d01977cdcde3f3b0	checker::library_compiler::run_library_release_probe	glob-reexport
333	probes	fn:assert_exact_full_profile_owned_base_checks_caller_certified_suffix	pub	# [cfg (any (test , feature = "test-utils"))]	9698:0..9780:1	371845..375388	79d29138f10a0a3f35375d865e24c446ef0a3fb726029e9230625829d80b9c2c	checker::library_compiler::assert_exact_full_profile_owned_base_checks_caller_certified_suffix	glob-reexport
334	tests	mod:tests	private	# [cfg (test)]	9782:0..14009:1	375388..546133	bb7b20904ec9ec5524b5c25c8ab4bca3dd46c2528ed66693b598687678ba50ef	checker::library_compiler::tests	externalize-module-body
```

### Complete test manifest

The aggregate tests-module digest above covers all 120 nested AST items. These per-test hashes
prove that all 93 complete `#[test]` functions move as whole items.

```text
ordinal	name	source-line	token-sha256
00	selected_library_terminal_evidence_is_exact_for_a_ready_interface	9877	de444a5bc9bae14f16793bbcd2909933427fd976e3119eeb66b9b9b649ebba18
01	sparse_selected_prefix_enum_is_filled_while_unselected_peer_stays_frozen	9915	69538b59df13b30c6e35bcf536eac1d15c270cbc5ca50faa369d79eb9d0046eb
02	sparse_reparsed_source_does_not_replace_an_unselected_enum_prefix	10000	61b0233de86fd2c6c69b7f9fef89dc38140fbb3f4089763d0ead8d9dc43e063d
03	sparse_class_owner_only_selection_preserves_frozen_class_terminal	10064	081331b6a278b93e5272d7c6ae2a6ce3d38bdc727861ac9b84d029219ab2b859
04	sparse_type_group_only_class_selection_rebuilds_with_a_fresh_class	10166	2b53e375c86a89b0bfff680a42912c8c86dabcde8361437070737d655dc0b223
05	sparse_type_group_selection_precedes_selected_class_preservation	10225	f44ad5ac5cc1aae742d36e5e16435ad9e8b2b03f2b7ed5d7e2d448a3c10d3fbc
06	sparse_class_only_merged_group_preserves_all_fragments_and_frozen_class	10289	de937b3c9ed53ac5ed47e5dd34bc451fd8fdd74a328dc4c2987747d21b22bee0
07	sparse_class_only_attachment_keeps_original_identity_and_fresh_header_params	10349	19783101324077f006a8b53cb663b508c94d69ebd73b3079bae536a5e39520ff
08	sparse_class_only_merged_group_attaches_once_to_the_class_decl_id	10414	912b818dd55bc90583d238d113211c0b9c651745c2ad642a5c160460bed10506
09	frozen_base_receipt_tracks_the_successful_central_freeze	10469	8b11fc10270f54b1d114ab2c7630c103da76f6c90b23ea22580821faee16efec
10	semantic_publication_receipt_tracks_the_actual_publish_entry	10490	4b88677c3958c6564e390bbc3818c69ec564db35b861081d2ac985bf6822d8b8
11	legacy_library_only_declaration_receipt_is_exact	10507	f330c7fa21c20811e61575f01b65f19024995203673275998cc50b4c99f6adc9
12	complete_combined_declaration_receipt_preserves_library_prefix_and_user_suffix	10586	c029773fa6e9367d0cb2e9dab91492e189630544b7c4234c95a7695a9723146e
13	complete_combined_probe_does_not_publish_product_census_evidence	10652	8cc16acd2e9dfe57b8b6ab8ca83036f8782de167c10612a24d75e38eb2e53b5c
14	replay_receipts_track_the_central_trace_and_plan_entries	10681	47d37a949812b018c00a1b6dc5361514052f5c694fafdf8c124da9748e24902f
15	complete_source_user_unit_validation_rejects_duplicate_module_ordinals	10701	6b9e6c6c578df4dd2e03cff7d6ca397128bcce46527e5ff6db7e837fa40c0b03
16	complete_source_result_assembly_rejects_unexpected_module_records	10719	c37ff6f0db045c8ac97c969e6e836697dbe5281fd4a1ccd44bee626e75eae476
17	canonical_property_key_bytes_pin_symbol_discriminants	10736	0f9a3c8eca9ab71a869c2e354d6275d717fef3a56cfb8f2ab9152d6be46880c1
18	authenticated_symbol_root_is_installed_before_interface_fill	10787	42e59dd57b808a5099951006929a0940afeef1288ab96fe5c4a6f324aec1a0ec
19	symbol_certificate_round_trips_and_rejects_corruption	10810	7ab998d099a397292360309066c444bda30908cb045bf016bec24c20e8606ee0
20	symbol_certificate_survives_dense_and_sparse_forks	10847	71df491bfca211ac6c9351c7c607b6ff30e509d600db944854cb1d02d30cccd3
21	exact_library_roots_install_only_the_six_trusted_marker_shapes	10917	ae7897ca9c28554d103991e342dd7a05cb4e3e40a6864ef544858b212d6985c0
22	early_omit_this_parameter_demand_does_not_freeze_its_library_root_twice	10949	7b14516bbcdc9fc6e9b0bca524952a854eceb24c5ad78bbc3f210f805a4ecaa1
23	same_named_wrong_shapes_and_unlisted_intrinsics_are_not_trusted_markers	10974	e2425c44640fd6ebe7e623d655984b8cef4998726e44097a6f97c52ab2a5eb16
24	trusted_string_marker_rejects_each_near_shape_dimension	11004	15e39287504a4c2c34fcc2f2089e5c994bd8782119581f78cdf7cd3b44550ea0
25	trusted_this_type_marker_rejects_each_near_shape_dimension	11022	d28cd0370b28ab0194aed00af72304dd044b14bc8f00924541b86c1b2fe119ad
26	trusted_omit_this_parameter_rejects_parameter_near_shapes	11040	81742757baf814ffac0f307e1a610590b0261e7d4d10b1506ee2f197e8440164
27	trusted_omit_this_parameter_rejects_conditional_near_shapes	11058	1fd9ece724c26ddd7a4b4863817b9c397a575bcdba95040f405f09190707817e
28	injected_results_are_ast_free_owned_terminals	11280	b636e34836f1a554adf8e4ae5c684466099a35853eca5993efac59a3b6294534
29	failed_conditional_alias_recovery_leaves_no_pending_reservation	11288	b7aabcf1737492c92bb5ee24c1506043224880030eabaf1eaf48fe98a34ba013
30	failed_mapped_alias_recovery_leaves_no_pending_reservation	11298	a369f048317fb1efc9b37d76339516d6b9dc0b7b158dd0a69684f6ed557aa1ac
31	conflicting_conditional_alias_reservations_close_in_both_orders	11307	4f8dbcb36115dd8723037e2496b4dcfb911a8e9b948b5869eaddb21342bda0d9
32	conflicting_mapped_alias_reservations_close_in_both_orders	11335	88c7fde213f51763d88a857a4b16cd5bfde4a28cba246e6039233aba852eaaf4
33	conflicting_object_alias_reservations_close_in_both_orders	11359	d4758936bff6778f00e9f2add49ab0b5063ef7bc619e76701ba7a03dd0de766e
34	valid_class_interface_merge_closes_reservations_in_both_orders	11383	14df59bb1df3781c3e06999e425e807f63c93b53583c48f114c3f31e5c6914f5
35	acyclic_object_aliases_publish_one_structural_identity	11398	a9047f77f686e9e06ec56aafcdced2d019ebdb7f2316cbbb88ef9db255fc9342
36	acyclic_object_alias_reuses_an_existing_anonymous_shape	11421	5db558cfcc4e1bed488b471bdb082c219c49f0a542d38057f74143610904c49a
37	recursive_object_aliases_retain_their_stable_reserved_roots	11445	b2afe34e4a5c342d24f51a2c5c8212b7b6f641cb6caa1eda57bad24ef0d49b03
38	captured_object_alias_roots_are_never_remapped_behind_existing_edges	11471	379605be16c941313d5b6056646c26d868490ba169e3713a451e6138501f7f51
39	type_parameter_defaults_follow_final_canonical_alias_identity	11501	70385bc6c24317dbf019434f799c43bbf7baa6997235dbc6c96e1e37bf778cb1
40	replay_index_covers_class_references_in_interface_parameter_defaults	11534	deba42258179ed8a94699591657c8b1bcdaf825bbe306d3e31f9f5b496dc0892
41	replay_index_covers_class_references_copied_across_overload_rows	11553	3fd928e8840f82832cd4da7db8c522bf7d8ed7389b35dd2f87a8509347adedb7
42	terminal_class_validator_follows_declared_recipe_template_dependencies	11573	de66a6b245b8362555ae66a01a50243cbbc693240933ab46bd8d35e979fc02be
43	terminal_class_validator_preserves_cycles_duplicates_and_multiple_classes	11674	8985e2afb4a1dc6779645f1d92a374a73e5126c06a6c17dea71b7990b44bd90e
44	terminal_class_validator_processes_a_shared_semantic_tail_once	11750	065e0138848375ec7ebf321ce7d5f6e38179b3899fc7730329facf246119470a
45	terminal_class_validator_deduplicates_repeated_class_across_shared_tail	11891	188e9db37b8d5070e2a64038fe98960f1d071e9d376f78e56238137b33651542
46	terminal_class_validator_summary_work_is_output_sensitive_for_unique_chain	11997	077193e9d2a86279e5a36b754aea2e66cdecf69dee6ba0ff4be8e70b72acd388
47	terminal_class_validator_shared_union_spine_stays_complete	12224	8ba487c2842ec24c23e9bbc2f3550b0f4f1b6b15483db24234a24c472f1646ba
48	terminal_class_validator_shared_union_spine_summary_is_output_sensitive	12260	069d2435bcb2905a4e36c8f28f4c52250cbda73b9ee9c29b3c2097ca5a627b7e
49	declared_recipe_domain_uses_independent_base_and_local_ends	12294	e7a74602617886f9f9ff47da564e279e096520ffc3944a9364c10604a019c23b
50	binder_source_unit_domain_cannot_alias_the_declared_recipe_domain	12336	5343ac3bd6eb3ec475798f8023fb36ad5f58456d37e9b97ece7fa9bc2e6cffb0
51	store_prefix_digest_authenticates_unreferenced_planned_recipes	12394	c5989a2e3aa5704efa91a9dbc10cee28909cd4382b80cd7ba0ba48fde181776f
52	replay_index_records_empty_and_constructor_only_parent_edges	12413	aacd77929919553a6b94e9898d2e3db362faf28ee3fc60a7419a1d0d5ffa8b88
53	replay_root_census_rejects_a_missing_binder_root	12465	8752465d40e1690f58f1dbace3b6a82aeebb3377716a2b15bcf80c8c087e7bb8
54	replay_root_census_rejects_an_inexact_slot_set	12478	d6a45741fbc0120bb2ea8af0983fe71f733d51e5b38593a76c20146a1404e7a3
55	replay_root_census_requires_explicit_global_this_to_publish	12497	7a074a004cdc9da6f7bf843b37ea11d3a181e2e363b9221f07e596bd69053e78
56	instantiated_namespace_expectation_does_not_depend_on_storage_or_root_row	12506	1cfcc746d848a72dec6d1b03580b5b4a7b2f27a473240ec2e6cb236a60424bf0
57	replay_root_normalization_covers_pure_and_mixed_namespace_semantics	12534	0eea6a9ac1593e14a7ef8961d8ff49334ad602d3cb7c720878575aa020b5a40e
58	object_alias_identity_ignores_member_source_order	12586	12a6e212ef498179b1f7301453c58091cccdbf645b58eddc992f9f720ca55c64
59	object_alias_identity_keeps_identity_bearing_metadata	12604	1ca8c99853fe93c846a4e905823d12bcea1f62caf1563b67ea628e63737e9a29
60	acyclic_object_alias_canonicalization_scales_linearly	12639	87c83daa8c02aece5faa003498b55c69ac171a903c27fccaf31c742a4b730ed0
61	focused_profile_selects_complete_native_bridge_identities	12694	0e6608cb8182b7a0f50fb06ee78d7a5e701db96abef73ee4138cfaf6ca3c6657
62	owned_runtime_product_parts_restore_a_consumable_base	12735	3b9ad3052def6e30e9b3cf378269570cd0c639d4ef3b094b0d1eb0d7ecdeb8af
63	owned_runtime_product_parts_restore_native_bridge_error_behavior	12773	bfb7446e4f6a118a718cb7803100002e100f9b2f71454b5d4c5555f64c3f27bb
64	owned_runtime_product_parts_reject_empty_and_gapped_source_prefixes	12795	88a129bf99835c13e38cf6c26695b6a461e4c5b6918240ac1d8a6fb15aa510a9
65	owned_runtime_product_parts_reject_group_and_counter_collisions	12820	d995cce0975d2b316e5ea042a42ede99ed6776207462111e250bef0ee12e3875
66	owned_runtime_product_parts_reject_missing_required_class_rows	12866	e4cd9a653530cd37c39b55169edad14260225f4ab71130d1e3f565469c2ca616
67	owned_runtime_product_parts_reject_dangling_semantic_and_type_references	12962	7210cdddcfc9f6159f04ba83e9b99154adfcaead90ae1fc732ad57c8e9e418a5
68	owned_runtime_product_parts_reject_dangling_runtime_binder_references	12995	4145fee23143eaa96512457b5bc5e52d6d50cd84d8e90d6712fd27beaf50e992
69	phase_timings_are_real_nonoverlapping_measurements	13025	0c399846001b550d948c831ec6e1cc7fe42f13a98dc68994cb7ed03e4976c6c6
70	release_probe_lines_have_the_exact_v1_contract	13041	0fce16dd1b0bb20cc974c3ad3ae60faff1c91c952c626a51929d0553bccee25f
71	recoverable_parser_diagnostic_fails_closed_before_profile_execution	13077	255fac78ae40bbc7be92e2438fd073e3fbebbff7e5197c1c8a7f3cc431e9b1cf
72	focused_shared_interface_identity_and_surface	13103	e082716c4541e48f517903f4032a179749f657e8ad69aae34faf81869c8b35e6
73	focused_implementation_diagnostic_keeps_exact_library_key	13137	1a9032d2ef201e6025ea7a6f476a10605068e5c690a018585e0097204e9c4ef3
74	the_base_route_retains_no_record_while_the_census_route_collects_them	13160	9788e6575eeb0974f0bf3084aa014c9bf27626bf150394c09ea9364333a73d79
75	focused_function_namespace_and_module_private_probes	13182	46c97137ef2604439f9a8ff4734c66977d0eda9bc062246810699314e0af5865
76	focused_typed_ts1319_claim_is_reported_once_by_binder_consumer	13216	7a14b42d80646682f10529e0f041f084b02566113c6229872af18080f82b58c3
77	mixed_ts1319_and_ts1063_diagnostics_fail_closed	13249	e652d9958f1c16fd036d32c9b588818446e631bca33cb7adb6a6744a088ee8ab
78	parser_export_claim_inventory_rejects_duplicates_and_unmatched_binder_claims	13283	02faf0717c90eb5897b3add0a05d5bab9287141e9d115062e8cbfb609e9abdb8
79	focused_identical_callable_offsets_keep_exact_owners_and_signatures	13299	f6a13d1a18dd5de442059be3d22943b54d7c52d97d40daf407bf8f02110c6e4e
80	focused_function_namespace_merges_are_input_order_independent	13344	66b97be0b589b3d809b7c898c45aaa986ddcb0cd23ef37ad45ac5a40daa276c7
81	focused_parse_clean_binder_reporting_families_have_real_receipts	13383	2ec619f55a7141e1b68f73ba2ecbff446e12a854bc6d2e9272dd30fcae8e5a49
82	object_overlap_demand_retries_do_not_rescan_named_property_prefixes	13554	a87b29df4a219a309ad0ee319ed6bb9759b3f1c9e041b6608cb1058bbc73685a
83	owned_generic_promise_identity_preserves_resolve_argument	13569	09134e0b8aa19689ca6ef8ef4bfed61f962e8a87a92782b25c9ff329a246ed8d
84	owned_conditional_awaited_preserves_non_thenable_argument	13630	28fb6450948c09c8d8860e1ade4ebf7dde73347355954b3f5bc2cd90aec4e2d4
85	owned_runtime_forks_share_checker_prefixes_and_keep_empty_private_suffixes	13713	0b6f9f95dfd044026add5f5ff44541a5f7e726882f9ba2b05f70630f79b2fe6d
86	owned_base_final_identity_witness_uses_real_aliases_dense_suffixes_and_base_shapes	13739	3ecd7ef70acf0cd2586d455e116b2c3c5d401c41c0332f4a32ca09f89992e495
87	production_check_program_does_not_enter_final_identity_inspection_route	13805	81c5fb53f50fb5b3cd9c10e17cb0bc99b1411a052339a1bcc24cdae27cf09767
88	owned_library_state_checks_caller_certified_collision_free_suffix_without_prefix_rebinding	13827	f6aa6f2cc0d921e4b80837cb631ce9b7da1e155c787076b44de12eef406f9f96
89	incomplete_collision_identity_selection_emits_no_user_diagnostics	13881	f4a488fbc691cceaa25f9270341af33081a876bcb04c8d468679c870274019c6
90	owned_frozen_interface_heritage_projects_into_local_interface	13919	9a03e68fb7e169e9c519a2b712b7e74eed819155395a37bb4e278256e172ec75
91	owned_library_preserves_class_and_namespace_runtime_metadata_for_caller_certified_collision_free_suffix	13956	a4e91c7f51fc2c97d71319dd4976ac8b8e86b2546c09f37f3b739d44d71ec4bd
92	owned_library_continuation_admits_declare_global_syntax	13989	1d08d2e789a31a78c0c9c0335c56c6ff342104d21bed8b3241596b782f1383a0
```

### Audit generator source

The exact generator inputs follow. Create this three-file temporary crate, then run the frozen
command above. The generator also accepts an individual post-split child; WU1 aggregates those rows
after excluding its new seam imports.

`Cargo.toml`:

```toml
[package]
name = "typokat-decomposition-manifest"
version = "0.1.0"
edition = "2021"

[dependencies]
proc-macro2 = { version = "=1.0.107", features = ["span-locations"] }
quote = "=1.0.47"
sha2 = "=0.10.9"
syn = { version = "=2.0.119", features = ["full", "extra-traits", "parsing"] }
```

`Cargo.lock`:

```toml
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 4

[[package]]
name = "block-buffer"
version = "0.10.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3078c7629b62d3f0439517fa394996acacc5cbc91c5a20d8c658e77abd503a71"
dependencies = [
 "generic-array",
]

[[package]]
name = "cfg-if"
version = "1.0.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9330f8b2ff13f34540b44e946ef35111825727b38d33286ef986142615121801"

[[package]]
name = "cpufeatures"
version = "0.2.17"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "59ed5838eebb26a2bb2e58f6d5b5316989ae9d08bab10e0e6d103e656d1b0280"
dependencies = [
 "libc",
]

[[package]]
name = "crypto-common"
version = "0.1.7"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "78c8292055d1c1df0cce5d180393dc8cce0abec0a7102adb6c7b1eef6016d60a"
dependencies = [
 "generic-array",
 "typenum",
]

[[package]]
name = "digest"
version = "0.10.7"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9ed9a281f7bc9b7576e61468ba615a66a5c8cfdff42420a70aa82701a3b1e292"
dependencies = [
 "block-buffer",
 "crypto-common",
]

[[package]]
name = "generic-array"
version = "0.14.7"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "85649ca51fd72272d7821adaf274ad91c288277713d9c18820d8499a7ff69e9a"
dependencies = [
 "typenum",
 "version_check",
]

[[package]]
name = "libc"
version = "0.2.189"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3eaf3ede3fee6db1a4c2ee091bf8a8b4dccdc6d17f656fb07896ee72867612f2"

[[package]]
name = "proc-macro2"
version = "1.0.107"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "985e7ec9bb745e6ce6535b544d84d6cd6f7ad8bd711c398938ae983b91a766d9"
dependencies = [
 "unicode-ident",
]

[[package]]
name = "quote"
version = "1.0.47"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "1fbf4db142a473a8d80c26bbf18454ed458bf8d26c8219c331daecfdbd079001"
dependencies = [
 "proc-macro2",
]

[[package]]
name = "sha2"
version = "0.10.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "a7507d819769d01a365ab707794a4084392c824f54a7a6a7862f8c3d0892b283"
dependencies = [
 "cfg-if",
 "cpufeatures",
 "digest",
]

[[package]]
name = "syn"
version = "2.0.119"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "872831b642d1a07999a962a351ed35b955ea2cfc8f3862091e2a240a84f17297"
dependencies = [
 "proc-macro2",
 "quote",
 "unicode-ident",
]

[[package]]
name = "typenum"
version = "1.20.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "b6f5e870be6c3b371b77fe0ee0bafb859fa4964b4404c27de1d380043c4dda20"

[[package]]
name = "typokat-decomposition-manifest"
version = "0.1.0"
dependencies = [
 "proc-macro2",
 "quote",
 "sha2",
 "syn",
]

[[package]]
name = "unicode-ident"
version = "1.0.24"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "e6e4313cd5fcd3dad5cafa179702e2b244f760991f45397d14d4ebf38247da75"

[[package]]
name = "version_check"
version = "0.9.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0b928f33d975fc6ad9f86c8f283853ad26bdd5b10b7f1542aa2fa15e2289105a"
```

`src/main.rs`:

```rust
use quote::ToTokens;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::{Attribute, Item, Visibility};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn one_line(tokens: impl ToTokens) -> String {
    tokens
        .into_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn item_identity(item: &Item) -> (String, String) {
    match item {
        Item::Const(item) => ("const".into(), item.ident.to_string()),
        Item::Enum(item) => ("enum".into(), item.ident.to_string()),
        Item::ExternCrate(item) => ("extern-crate".into(), item.ident.to_string()),
        Item::Fn(item) => ("fn".into(), item.sig.ident.to_string()),
        Item::ForeignMod(item) => ("foreign-mod".into(), one_line(&item.abi)),
        Item::Impl(item) => {
            let trait_name = item
                .trait_
                .as_ref()
                .map(|(_, path, _)| one_line(path))
                .unwrap_or_else(|| "inherent".into());
            ("impl".into(), format!("{} for {}", trait_name, one_line(&item.self_ty)))
        }
        Item::Macro(item) => (
            "macro".into(),
            item.ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| one_line(&item.mac.path)),
        ),
        Item::Mod(item) => ("mod".into(), item.ident.to_string()),
        Item::Static(item) => ("static".into(), item.ident.to_string()),
        Item::Struct(item) => ("struct".into(), item.ident.to_string()),
        Item::Trait(item) => ("trait".into(), item.ident.to_string()),
        Item::TraitAlias(item) => ("trait-alias".into(), item.ident.to_string()),
        Item::Type(item) => ("type".into(), item.ident.to_string()),
        Item::Union(item) => ("union".into(), item.ident.to_string()),
        Item::Use(item) => ("use".into(), one_line(&item.tree)),
        Item::Verbatim(tokens) => ("verbatim".into(), one_line(tokens)),
        _ => ("other".into(), one_line(item)),
    }
}

fn attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn visibility(item: &Item) -> String {
    let visibility = match item {
        Item::Const(item) => Some(&item.vis),
        Item::Enum(item) => Some(&item.vis),
        Item::ExternCrate(item) => Some(&item.vis),
        Item::Fn(item) => Some(&item.vis),
        Item::Macro(_) => None,
        Item::Mod(item) => Some(&item.vis),
        Item::Static(item) => Some(&item.vis),
        Item::Struct(item) => Some(&item.vis),
        Item::Trait(item) => Some(&item.vis),
        Item::TraitAlias(item) => Some(&item.vis),
        Item::Type(item) => Some(&item.vis),
        Item::Union(item) => Some(&item.vis),
        Item::Use(item) => Some(&item.vis),
        Item::ForeignMod(_) | Item::Impl(_) | Item::Verbatim(_) | _ => None,
    };
    match visibility {
        None | Some(Visibility::Inherited) => "private".into(),
        Some(vis) => one_line(vis),
    }
}

fn offset(line_starts: &[usize], line: usize, column: usize) -> usize {
    line_starts[line - 1] + column
}

fn main() {
    let path = env::args().nth(1).expect("source path argument");
    let source = fs::read_to_string(&path).expect("read source");
    let parsed = syn::parse_file(&source).expect("parse source");
    let mut line_starts = vec![0usize];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(index + 1);
        }
    }

    println!(
        "source\t{}",
        Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("UTF-8 source file name")
    );
    println!("source_bytes\t{}", source.len());
    println!("source_sha256\t{}", digest(source.as_bytes()));
    println!("item_count\t{}", parsed.items.len());
    let normalized = parsed
        .items
        .iter()
        .map(one_line)
        .collect::<Vec<_>>()
        .join("\n");
    println!("normalized_token_count\t{}", normalized.split_whitespace().count());
    println!("normalized_sha256\t{}", digest(normalized.as_bytes()));
    let test_items = parsed
        .items
        .iter()
        .find_map(|item| match item {
            Item::Mod(item) if item.ident == "tests" => item.content.as_ref().map(|content| &content.1),
            _ => None,
        })
        .unwrap_or(&parsed.items);
    let test_functions = test_items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(item) if item.attrs.iter().any(|attr| attr.path().is_ident("test")) => {
                Some(item)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let normalized_tests = test_items
        .iter()
        .map(one_line)
        .collect::<Vec<_>>()
        .join("\n");
    println!("test_count\t{}", test_functions.len());
    println!("test_item_count\t{}", test_items.len());
    println!("test_normalized_token_count\t{}", normalized_tests.split_whitespace().count());
    println!("test_normalized_sha256\t{}", digest(normalized_tests.as_bytes()));
    println!("ordinal\tkind\tidentity\tvisibility\tcfg\tstart\tend\traw_start\traw_end\ttoken_sha256\traw_sha256");

    let mut previous_end = 0usize;
    for (ordinal, item) in parsed.items.iter().enumerate() {
        let span = item.span();
        let mut start = offset(&line_starts, span.start().line, span.start().column);
        for attr in attrs(item) {
            let attr_start = attr.span().start();
            start = start.min(offset(&line_starts, attr_start.line, attr_start.column));
        }
        let end_pos = span.end();
        let end = offset(&line_starts, end_pos.line, end_pos.column);
        let raw_start = previous_end;
        let raw_end = if ordinal + 1 == parsed.items.len() {
            source.len()
        } else {
            end
        };
        assert!(raw_start <= start && start <= end && end <= raw_end);
        let (kind, identity) = item_identity(item);
        let cfg = attrs(item)
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .map(one_line)
            .collect::<Vec<_>>();
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}:{}\t{}:{}\t{}\t{}\t{}\t{}",
            ordinal,
            kind,
            identity.replace('\t', " "),
            visibility(item),
            if cfg.is_empty() { "default".into() } else { cfg.join(" && ") },
            span.start().line,
            span.start().column,
            span.end().line,
            span.end().column,
            raw_start,
            raw_end,
            digest(one_line(item).as_bytes()),
            digest(&source.as_bytes()[raw_start..raw_end]),
        );
        previous_end = end;
    }
    for (ordinal, test) in test_functions.into_iter().enumerate() {
        println!(
            "test\t{}\t{}\t{}\t{}",
            ordinal,
            test.sig.ident,
            test.sig.span().start().line,
            digest(one_line(test).as_bytes()),
        );
    }
}
```
