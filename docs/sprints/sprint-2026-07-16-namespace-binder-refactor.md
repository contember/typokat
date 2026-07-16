# Sprint — namespace binder refactor: split & dedup (2026-07-16)

**Goal.** Mechanically restructure `src/binder/namespace.rs` (7 954 lines) into a
`src/binder/namespace/` submodule tree and remove measured duplication, with bit-identical
checker behavior.

**Theme.** Backlog 43 left one 4.6k-line implementation file (plus 3.3k lines of in-file
tests) mixing six concerns, four parallel AST walkers with mirrored `Statement`/`Declaration`
match arms, and three copies of `declaration_owner_scope` across binder and checker. This
sprint is a pure refactor: no semantics, no diagnostics, no new capability. Success is
`cargo test` + focused conformance + official-suite `run --check` all green with zero
scoreboard movement, and the duplication findings below gone.

## Refs re-verified at HEAD (2026-07-16)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `src/binder/namespace.rs` is 7 954 lines; `mod tests` starts at
  `src/binder/namespace.rs:4638` (~3 316 test lines, ~4 640 implementation lines).
- ✔ `walk_statement`'s `Declaration`-carrying arms (`src/binder/namespace.rs:2794-2907`)
  are token-identical to `walk_declaration` (`src/binder/namespace.rs:3006-3130`).
- ✔ `record_deferred_statement` (`src/binder/namespace.rs:3846-4027`) mirrors
  `record_deferred_declaration` (`src/binder/namespace.rs:4029-4134`) the same way.
- ✔ `reserve_statement_header` (`src/binder/namespace.rs:3566-3619`) mirrors
  `reserve_declaration_header` (`src/binder/namespace.rs:3621-3661`).
- ✔ `bind_selected_namespace_value_statements` (`src/binder/namespace.rs:2527-2578`) mirrors
  `bind_selected_namespace_value_declaration` (`src/binder/namespace.rs:2580-2614`).
- ✔ The same statement/declaration mirroring exists in the ordinary binder:
  `bind_type_declaration_statement` (`src/binder/bind.rs:600-642`) vs `bind_type_declaration`
  (`src/binder/bind.rs:644-680`), and in the checker's namespace body walk
  (`src/check/checker/namespace_values.rs:1848-1866`).
- ✔ `oxc_ast` 0.137 generates `Statement::as_declaration()` via `inherit_variants!`
  (`oxc_ast-0.137.0/src/ast/macros.rs:790`); the codebase does not use it yet.
- ✔ `declaration_owner_scope` exists three times with identical match logic:
  `src/binder/namespace.rs:1845` (on `Binder`), `src/binder/namespace.rs:3271`
  (on `BindState`), `src/check/checker/namespace_values.rs:2488` (on `Pass`).
  ⚠ The `Binder`/`Pass` copies read `compilation_global` from the binder while the
  `BindState` copy reads `state.namespaces.compilation_global` — confirm these are the same
  scope before unifying.
- ✔ `NamespaceTable::classify` (`src/binder/namespace.rs:1029-1144`) contains six
  near-identical canonical-ordering blocks keyed on `(source, span/start, tiebreak)`.
- ✔ `module_export_name` is duplicated verbatim in `src/driver.rs:391-397` and
  `src/check/checker/mod.rs:1250-1256`; `metadata_name` (`src/binder/namespace.rs:4623`)
  covers the same match with a richer return type.
- ✔ The 12-variant "plain runtime statement" guard list is duplicated between
  `src/binder/namespace.rs:2986-2997` and `src/check/checker/namespace_values.rs:1867-1878`.
- ✔ jscpd 5.0.12 (`-k 60 -l 8 --skip-comments`) over `src/`: 203 clones repo-wide, 9 touching
  `binder/namespace.rs`; the only production cross-file clone involving it is the guard list
  above. The dominant duplication is intra-file (the mirrored walkers).
- ✔ External production consumers of the namespace module are only
  `src/binder/bind.rs`, `src/check/checker/mod.rs`, `src/check/checker/namespace_values.rs`
  (plus tests in `src/driver.rs`); ~28 of 56 public types are consumed only by in-file tests —
  deliberate dormant substrate owned by backlog `15`/`82` per the 2026-07-15 sprint (WU1b).
- ✔ Backlog-43 sprint (`sprint-2026-07-15-namespaces-declaration-merging.md`) is still active:
  WU6A implementation landed (`23bad42`, `16cda3f`, `52cea92`), WU7 adversarial review and
  closure are pending.

## Work units

### WU1 — collapse Statement/Declaration mirrored walkers (effort M)

- **Problem.** Five walker pairs re-list every `Declaration`-carrying variant twice
  (~430 duplicated production lines): `walk_statement`/`walk_declaration`,
  `record_deferred_statement`/`record_deferred_declaration`,
  `reserve_statement_header`/`reserve_declaration_header`,
  `bind_selected_namespace_value_statements`/`bind_selected_namespace_value_declaration`
  (all `src/binder/namespace.rs`), and `bind_type_declaration_statement`/
  `bind_type_declaration` (`src/binder/bind.rs`). Any new declaration form must be added in
  two places per walker — a drift trap.
- **Verify first.** Diff each pair arm-by-arm to confirm exact behavioral equality
  (done at HEAD for all five — the only statement-side extras are statement-only forms:
  imports, export wrappers, UMD export, runtime statements). Confirm
  `Statement::as_declaration()` covers all mirrored variants including
  `TSModuleDeclaration`/`TSGlobalDeclaration`/`TSImportEqualsDeclaration`.
- **Scope.** In each statement-form walker, delegate via
  `if let Some(declaration) = statement.as_declaration() { … }` to the declaration-form
  walker and delete the mirrored arms. Keep statement-only arms exactly as they are. One
  nuance: `record_deferred_statement` must pass `exported: false` when delegating (its
  current inline arms all use `OrdinaryDeclaration`). Also collapse the mirrored export arm
  in `src/check/checker/namespace_values.rs:1848-1866` by delegating to the same per-kind
  helpers.
- **Acceptance / witness.** `cargo test` (unit + conformance) and
  `tooling/official-suite` `run --check` pass with zero change; `git diff --stat` shows
  namespace.rs shrinking by roughly 400+ lines with no test file edits.
- **Touch points.** `src/binder/namespace.rs`, `src/binder/bind.rs`,
  `src/check/checker/namespace_values.rs`.

### WU2 — single `declaration_owner_scope` (effort S)

- **Problem.** Three identical implementations (binder ×2, checker ×1) of owner→scope
  projection; a future `DeclarationOwner` variant must be handled three times.
- **Verify first.** Prove `Binder::compilation_global` and
  `NamespaceTable::compilation_global` denote the same scope (or document why the
  `BindState` copy reads the table's field); check the `Option` shape at
  `src/binder/namespace.rs:3282`.
- **Scope.** One method on `NamespaceTable` (it owns namespaces, fragments, and the
  compilation-global field); the `Binder` method and the checker free function become
  delegating one-liners or are inlined away.
- **Acceptance / witness.** Same gates as WU1; exactly one match over `DeclarationOwner`
  variants remains in the tree (grep witness).
- **Touch points.** `src/binder/namespace.rs`, `src/check/checker/namespace_values.rs`.

### WU3 — canonical-ordering helper in `classify` (effort S)

- **Problem.** `NamespaceTable::classify` repeats the "build 0..n id vector, sort by
  `(source, start, tiebreak)`" block six times (`src/binder/namespace.rs:1045-1143`).
- **Verify first.** Confirm all six keys are strict total orders with the same shape and
  that no block sorts a foreign index type.
- **Scope.** One generic helper, e.g.
  `fn canonical_order<I: Copy, K: Ord>(len: usize, make: impl Fn(usize) -> I, key: impl Fn(&I) -> K) -> Vec<I>`;
  the six blocks become single calls. Do not change any sort key.
- **Acceptance / witness.** Same gates; the in-file ordering tests
  (`standalone_namespace_storage_order_uses_stable_source_keys`,
  `namespace_public_type_groups_are_source_ordered_across_global_reopenings`) stay untouched
  and green.
- **Touch points.** `src/binder/namespace.rs`.

### WU4 — shared small helpers across binder/checker/driver (effort S)

- **Problem.** Two verbatim cross-file duplications: `module_export_name`
  (`src/driver.rs:391`, `src/check/checker/mod.rs:1250`) and the 12-variant plain-runtime-
  statement guard (`src/binder/namespace.rs:2986`, `src/check/checker/namespace_values.rs:1867`).
- **Verify first.** Confirm both guard lists are the same 12 variants and both
  `module_export_name` bodies are identical (done at HEAD); pick the host module (no
  `src/util` exists — a small `src/ast_util.rs` or a `pub(crate)` home in `src/binder/` —
  decide in review, do not add a grab-bag module beyond these two functions).
- **Scope.** One `module_export_name` (the namespace-local `metadata_name` may stay — it
  returns `MetadataName`, a different contract) and one
  `is_plain_runtime_statement(&Statement) -> bool` predicate used by both call sites.
- **Acceptance / witness.** Same gates; jscpd re-run reports zero production cross-file
  clones touching `binder/namespace.rs`.
- **Touch points.** `src/driver.rs`, `src/check/checker/mod.rs`,
  `src/check/checker/namespace_values.rs`, `src/binder/namespace.rs`, one new small module.

### WU5 — split `binder/namespace.rs` into a submodule directory (effort M)

- **Problem.** One file hosts six concerns plus 3.3k lines of tests; navigation and
  review-scoping suffer (the checker side is already split into `check/checker/` submodules).
- **Verify first.** Confirm all external consumers import via `crate::binder::namespace::…`
  so a `mod.rs` with `pub use` re-exports keeps every consumer path unchanged; confirm
  in-file tests only need `super::` access (they do — they exercise private items, so tests
  must stay inside the module tree).
- **Scope.** Move-only split into `src/binder/namespace/`:
  - `mod.rs` — id newtypes, `SourceFileKind`/`ModuleBindingContext`/`CompilationUnit`,
    `has_external_module_indicator`, and `pub use` re-exports (public API unchanged);
  - `metadata.rs` — the record/enum layer (`Namespace`, `NamespaceFragment`,
    `NamespaceMember`, `Merge*`, `Global*`, `Deferred*`, `ExportContext`, `Umd*`, …);
  - `table.rs` — `NamespaceTable` storage, accessors, `classify`, instance states,
    dormant-storage candidates;
  - `classify.rs` — `classify_group`, `placement_issues`,
    `namespace_value_attachment_disposition`;
  - `lookup.rs` — the `impl Binder` qualified-view/lookup block
    (`qualified_symbol_view`, `root_merge_record`, …);
  - `walk.rs` — `WalkContext`, the (post-WU1) walkers, `bind_module_declaration`,
    `bind_global`, export contexts, reserve/dormant-symbol helpers, push helpers;
  - `deferred.rs` — deferred ambient-module recording;
  - `values.rs` — namespace value-attachment binding
    (`bind_namespace_value_attachment_members`, selected binding,
    `allocate_dormant_namespace_value_storages`);
  - `tests.rs` — the existing `mod tests` moved verbatim.
  Exact file boundaries may shift at implementation time; the rule is move + minimal
  visibility adjustments (`pub(super)`/`pub(crate)`) only — no logic edits in this WU.
- **Acceptance / witness.** Same gates; `git diff` outside `src/binder/namespace*` is empty
  except `src/binder/mod.rs`; no file in the new directory exceeds ~1 500 lines
  (tests excluded).
- **Touch points.** `src/binder/namespace.rs` → `src/binder/namespace/*`,
  `src/binder/mod.rs`.

## Out of scope (explicit)

- **Dormant substrate removal.** The ~28 externally-unconsumed metadata types
  (`ExportContext`, `DeferredAmbient*`, `UmdNamespaceExport`, `MergeClassification`
  internals, …) are reserved by the 2026-07-15 sprint for backlog `15`/`82`/`14` and are
  pinned by direct tests. Deleting or trimming them is a scope decision for those backlogs,
  not a refactor.
- **`OriginalModuleOrdinal`** stays a deliberate binder-layer copy
  (`src/binder/namespace.rs:89` documents it) — not duplication.
- **Perf cleanups** (linear fragment `find` inside the instance-state fixpoint at
  `src/binder/namespace.rs:1192-1196`, linear scans in `standalone_merge_record` /
  `root_merge_record`): O(n²) patterns that are harmless at current sizes; index them only
  with profiling evidence, per the ADR-0001 profiling-gate spirit.
- **Repo-wide id-newtype `index()` macro**: 13+ occurrences across `binder`/`types`/`check`
  follow the same idiom; a macro is a repo-wide convention change with churn beyond this
  sprint's theme.
- **Other cross-file clones** found by jscpd outside the namespace theme (largest:
  `check/checker/eval/instantiation.rs:50` ↔ `check/checker/eval/mapped.rs:94` and the
  `calls.rs` ↔ `relate/` cluster) — candidates for a separate cleanup sprint if wanted.

## Decisions

- Pure refactor contract: zero behavior change; every WU is gated on unchanged
  `cargo test`, focused conformance, and official-suite `run --check` (zero scoreboard
  movement). No new public API; `mod.rs` re-exports preserve all consumer paths.
- Dedup lands **before** the split (WU1–WU4 reviewed on the familiar single-file layout;
  WU5 then moves already-clean code), each WU as its own commit.
- No ADR needed — no architectural boundary, data flow, or invariant changes; this file is
  the record.

## Sequencing

1. **Gate:** start only after the backlog-43 sprint's WU7 adversarial review/closure lands
   (or with explicit user approval) — this refactor touches the same files that review
   re-reads, and churn mid-review invalidates it.
2. WU1 → WU2 → WU3 → WU4 (independent of each other after WU1; may be one subagent run,
   separate commits) → WU5 last.
3. Per dev-method: implementation via a subagent; the leader verifies gates and commits.
   A second read-only agent re-runs jscpd and spot-diffs the moved code as the review step
   (full adversarial review is not required for a behavior-preserving refactor, but the
   move-only claim of WU5 must be independently checked).

## Run log

<!-- Append as you work. -->
