# Sprint — namespace binder refactor: split & dedup (2026-07-16)

**Goal.** Mechanically restructure `crates/typokat-binder/src/binder/namespace.rs` (8 480 lines)
into a proposed `binder::namespace` submodule tree and remove measured duplication, with
bit-identical checker behavior.

**Theme.** The shipped namespace sprint left one 5.1k-line implementation file (plus 3.3k lines of in-file
tests) mixing seven concerns, four parallel AST walkers with mirrored `Statement`/`Declaration`
match arms, and three copies of `declaration_owner_scope` across binder and checker. This
sprint is a pure refactor: no semantics, no diagnostics, no new capability. Success is
`cargo test` + focused conformance + official-suite `run --check` all green with zero
scoreboard movement, and the duplication findings below gone.

**Status: planned, not started.** No WU has landed. The plan was written on 2026-07-16 and the
file has since absorbed the full-library/snapshot work (`+1 040` lines in
`crates/typokat-binder/src/binder/namespace.rs`, `+1 467` in
`crates/typokat-binder/src/binder/bind.rs`, `+757` in
`crates/typokat-check/src/check/checker/namespace_values.rs` between `547a433` and HEAD), so every
line ref below was re-measured on 2026-07-22.

## Refs re-verified at HEAD (2026-07-22, `36c3695`)

`✔` = confirmed live · `⚠` = drift/nuance caught. (Line numbers from the 2026-07-16 plan are
all stale; the values below supersede them.)

- ⚠ `crates/typokat-binder/src/binder/namespace.rs` is now 8 480 lines (was 7 954); `mod tests` starts at
  `crates/typokat-binder/src/binder/namespace.rs:5153` (~3 327 test lines, ~5 152 implementation lines). The growth is
  the snapshot round-trip layer (below), not new duplication.
- ✔ `walk_statement`'s `Declaration`-carrying arms (`crates/typokat-binder/src/binder/namespace.rs:3311-3424`)
  are token-identical to `walk_declaration` (`crates/typokat-binder/src/binder/namespace.rs:3532-3645`) — re-diffed
  at HEAD with the variant prefix normalized; the only difference is the trailing brace.
- ✔ `record_deferred_statement` (`crates/typokat-binder/src/binder/namespace.rs:4368-4549`) mirrors
  `record_deferred_declaration` (`crates/typokat-binder/src/binder/namespace.rs:4551-4656`) the same way.
- ✔ `reserve_statement_header` (`crates/typokat-binder/src/binder/namespace.rs:4088-4141`) mirrors
  `reserve_declaration_header` (`crates/typokat-binder/src/binder/namespace.rs:4143-4183`).
- ✔ `bind_selected_namespace_value_statements` (`crates/typokat-binder/src/binder/namespace.rs:3044-3095`) mirrors
  `bind_selected_namespace_value_declaration` (`crates/typokat-binder/src/binder/namespace.rs:3097-3131`).
- ✔ The same statement/declaration mirroring exists in the ordinary binder:
  `bind_type_declaration_statement` (`crates/typokat-binder/src/binder/bind.rs:936-978`) vs `bind_type_declaration`
  (`crates/typokat-binder/src/binder/bind.rs:980-1016`), and in the checker's namespace body walk
  (`crates/typokat-check/src/check/checker/namespace_values.rs:2133-2150`).
- ✔ `oxc_ast` 0.137.0 (pinned in `Cargo.toml:24`) generates `Statement::as_declaration()` via
  `inherit_variants!` (`oxc_ast-0.137.0/src/ast/macros.rs:790`); the codebase still does not use
  it anywhere (`grep as_declaration() src/` is empty).
- ⚠ `declaration_owner_scope` still exists three times with identical match logic:
  `crates/typokat-binder/src/binder/namespace.rs:2304` (on `Binder`), `crates/typokat-binder/src/binder/namespace.rs:3793`
  (free fn over `&BindState`), `crates/typokat-check/src/check/checker/namespace_values.rs:2812` (free fn over
  `&Pass`, **now generic over `Ticket: Copy + PartialEq`** — new since the plan).
  The 2026-07-16 ⚠ is **resolved**: `Binder::compilation_global` is `ScopeId`
  (`crates/typokat-binder/src/binder/bind.rs:51`), `NamespaceTable::compilation_global` is `Option<ScopeId>`
  (`crates/typokat-binder/src/binder/namespace.rs:858`) and is set to `Some(compilation_global)` at
  `crates/typokat-binder/src/binder/namespace.rs:2847` at the start of metadata binding — same scope; the `Option`
  only encodes "not yet bound". A unified host must therefore still return `Option`.
- ⚠ `NamespaceTable::classify` (`crates/typokat-binder/src/binder/namespace.rs:1326-1534`) has grown: **seven**
  canonical-ordering blocks (`canonical_namespaces`, `canonical_globals`,
  `canonical_deferred_modules`, `canonical_source_units`, `canonical_deferred_children`,
  `canonical_umd_exports`, `canonical_export_contexts`), and each is now **doubled** by a
  `library_order` branch (`crates/typokat-binder/src/binder/namespace.rs:1328`, from `uses_library_shared_globals()`)
  whose key flips component order — `(origin, start, source)` vs `(source, start, origin)`.
  That is 14 sort sites, plus two more `library_order`-branched in-place sorts
  (`namespace.fragments` at `:1331`/`:1343`, merge `declarations` at `:1394`/`:1402`).
- ⚠ **New concern in the file since the plan:** the namespace snapshot round-trip —
  `snapshot_primary` (`:1101`), `from_snapshot_primary` (`:1125`),
  `validate_snapshot_primary_for_classification` (`:1229`), `validate_snapshot_canonical`
  (`:1318`). This makes `classify`'s ordering load-bearing for snapshot canonicality and adds
  its own slice to the WU5 split.
- ✔ `module_export_name` is still duplicated verbatim in
  `crates/typokat-frontend/src/frontend.rs:234-240` and
  `crates/typokat-check/src/check/checker/mod.rs:2436-2442`; `metadata_name`
  (`crates/typokat-binder/src/binder/namespace.rs:5138`) covers the same match with a richer return
  type.
- ✔ The 12-variant "plain runtime statement" guard list is duplicated between
  `crates/typokat-binder/src/binder/namespace.rs:3502-3514` and
  `crates/typokat-check/src/check/checker/namespace_values.rs:2151-2163`
  (re-diffed at HEAD: same 12 variants, same order).
- ⚠ jscpd 5.0.12 (`-k 60 -l 8 --skip-comments`, `--ignore '**/*.d.ts'` — `crates/typokat-library/src/` now
  vendors TypeScript 6.0.3 `.d.ts`) over `src/`: **295** rust clones repo-wide (was 203), still
  **9** touching `crates/typokat-binder/src/binder/namespace.rs`, and still exactly **one** production cross-file clone —
  the guard list above (`crates/typokat-binder/src/binder/namespace.rs:3502-3514` ↔
  `crates/typokat-check/src/check/checker/namespace_values.rs:2151-2163`). The
  dominant duplication remains intra-file (four of the nine clones are the mirrored walkers).
- ⚠ External consumers of the namespace module have expanded well past the plan's three files:
  ~20 files now name `binder::namespace::…`, including `crates/typokat-binder/src/binder/symbol.rs`,
  `crates/typokat-binder/src/binder/declaration.rs`,
  `crates/typokat-diagnostics/src/diagnostics/mod.rs`,
  `crates/typokat-check/src/check/checker/library_reporting.rs`,
  `crates/typokat-check/src/check/checker/library_compiler.rs`,
  `crates/typokat-check/src/check/checker/function_groups.rs`,
  `crates/typokat-check/src/check/checker/decls/*`,
  `crates/typokat-check/src/check/checker/classes/*`.
  **`crates/typokat-binder/src/binder/references.rs` (test-only)
  does `use super::namespace::*`** — a glob, so WU5's `mod.rs` re-export must be exhaustive or
  that module breaks.
- ⚠ The dormant-substrate count has shrunk as the full-library work started consuming the
  metadata layer: at HEAD, **17 of 39** top-level `pub` items in
  `crates/typokat-binder/src/binder/namespace.rs` have no
  production consumer outside the file (`AliasContext`, `AliasSpaceIntent`, `DeclarationSpaces`,
  `DeclarationSyntaxFacts`, `DeferredChildKind`, `DeferredModuleKind`, `ImportBindingForm`,
  `ImportSyntaxFacts`, `Merge{Classification,Composition,CompositionKind,Record}`,
  `MergeSlot{Disposition,State,Summary}`, `MetadataName`, `has_external_module_indicator`) —
  down from the plan's "~28 of 56" (different counting method; treat both as order-of-magnitude).
  Still deliberate substrate owned by backlog `15`/`82` per the 2026-07-15 sprint (WU1b).
- ✔ The namespace sprint is archived after WU7 PASS: WU6A implementation landed (`23bad42`,
  `16cda3f`, `52cea92`) and the official ratchet closed at `30cd7cf`.

## Work units

### WU1 — collapse Statement/Declaration mirrored walkers (effort M)

- **Problem.** Five walker pairs re-list every `Declaration`-carrying variant twice
  (~335 removable production lines, measured at HEAD): `walk_statement`/`walk_declaration`
  (114), `record_deferred_statement`/`record_deferred_declaration` (106),
  `reserve_statement_header`/`reserve_declaration_header` (41),
  `bind_selected_namespace_value_statements`/`bind_selected_namespace_value_declaration` (35)
  (all `crates/typokat-binder/src/binder/namespace.rs`), and `bind_type_declaration_statement`/
  `bind_type_declaration` (`crates/typokat-binder/src/binder/bind.rs`, 37). Any new declaration form must be added in
  two places per walker — a drift trap.
- **Verify first.** Diff each pair arm-by-arm to confirm exact behavioral equality
  (re-done at HEAD for all five — the only statement-side extras are statement-only forms:
  imports, export wrappers, UMD export, runtime statements). Confirm
  `Statement::as_declaration()` covers all mirrored variants including
  `TSModuleDeclaration`/`TSGlobalDeclaration`/`TSImportEqualsDeclaration`.
- **Scope.** In each statement-form walker, delegate via
  `if let Some(declaration) = statement.as_declaration() { … }` to the declaration-form
  walker and delete the mirrored arms. Keep statement-only arms exactly as they are. One
  nuance: `record_deferred_statement` must pass `exported: false` when delegating (its
  current inline arms all use `OrdinaryDeclaration`). Also collapse the mirrored export arm
  in `crates/typokat-check/src/check/checker/namespace_values.rs:2133-2150` by delegating to the same per-kind
  helpers.
- **Acceptance / witness.** `cargo test` (unit + conformance) and
  `tooling/official-suite` `run --check` pass with zero change; `git diff --stat` shows
  namespace.rs shrinking by roughly 300 lines with no test file edits; the four mirrored-walker
  clones drop out of the jscpd report (9 → 5 clones touching `crates/typokat-binder/src/binder/namespace.rs`).
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs`, `crates/typokat-binder/src/binder/bind.rs`,
  `crates/typokat-check/src/check/checker/namespace_values.rs`.

### WU2 — single `declaration_owner_scope` (effort S)

- **Problem.** Three identical implementations (binder ×2, checker ×1) of owner→scope
  projection; a future `DeclarationOwner` variant must be handled three times.
- **Verify first.** *Done at HEAD* — `Binder::compilation_global: ScopeId`
  (`crates/typokat-binder/src/binder/bind.rs:51`) and `NamespaceTable::compilation_global: Option<ScopeId>`
  (`crates/typokat-binder/src/binder/namespace.rs:858`) denote the same scope; the table's field is filled at
  `crates/typokat-binder/src/binder/namespace.rs:2847` and the `Option` only encodes "not yet bound". So the unified
  method returns `Option<ScopeId>` and the two `Some(binder.compilation_global)` call sites
  keep working unchanged.
- **Scope.** One method on `NamespaceTable` (it owns namespaces, fragments, and the
  compilation-global field); the `Binder` method (`crates/typokat-binder/src/binder/namespace.rs:2304`), the
  `BindState` free fn (`:3793`), and the checker free fn
  (`crates/typokat-check/src/check/checker/namespace_values.rs:2812`, generic over `Ticket`) become delegating
  one-liners or are inlined away. The checker's `Ticket` generic is incidental — it only reaches
  `pass.binder`, so delegation drops the generic entirely.
- **Acceptance / witness.** Same gates as WU1; exactly one match over `DeclarationOwner`
  variants remains in the tree (grep witness).
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs`,
  `crates/typokat-check/src/check/checker/namespace_values.rs`.

### WU3 — canonical-ordering helper in `classify` (effort S)

- **Problem.** `NamespaceTable::classify` (`crates/typokat-binder/src/binder/namespace.rs:1326-1534`) repeats the
  "build 0..n id vector, then sort under a `library_order` branch" block **seven** times
  (`:1357`, `:1454`, `:1468`, `:1482`, `:1494`, `:1506`, `:1518`) — 14 sort sites in all.
- **Verify first.** Confirm all fourteen keys are strict total orders and that the two branches
  differ only by component order (`(origin, start, source)` under `library_order`,
  `(source, start, origin)` otherwise); confirm no block sorts a foreign index type. ⚠ Note the
  `canonical_namespaces` pair is *not* uniform with the rest — the library branch `expect`s a
  fragment while the else branch falls back to `SourceUnitKey(u32::MAX)`; keep both bodies
  verbatim.
- **Scope.** One generic helper, e.g.
  `fn canonical_order<I: Copy, K: Ord>(len: usize, make: impl Fn(usize) -> I, key: impl Fn(&I) -> K) -> Vec<I>`,
  called once per block with the branch-selected key closure; the seven blocks become single
  calls. Do not change any sort key. The two in-place sorts (`namespace.fragments`, merge
  `declarations`) sort owned vectors, not id vectors — leave them alone unless a second helper
  falls out for free.
- **Acceptance / witness.** Same gates; the in-file ordering tests
  (`standalone_namespace_storage_order_uses_stable_source_keys`,
  `namespace_public_type_groups_are_source_ordered_across_global_reopenings`) stay untouched
  and green. Since the snapshot layer landed, `NamespaceTable::validate_snapshot_canonical`
  (`crates/typokat-binder/src/binder/namespace.rs:1318`) is an additional, sharper witness: it re-derives ordering
  from a decoded snapshot and fails closed on any drift — run the snapshot round-trip tests
  explicitly, not just the ordering ones.
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs`.

### WU4 — shared small helpers across binder/checker/frontend (effort S)

- **Problem.** Two verbatim cross-file duplications: `module_export_name`
  (`crates/typokat-frontend/src/frontend.rs:234`,
  `crates/typokat-check/src/check/checker/mod.rs:2436`) and the 12-variant
  plain-runtime-statement guard (`crates/typokat-binder/src/binder/namespace.rs:3502`,
  `crates/typokat-check/src/check/checker/namespace_values.rs:2151`).
- **Verify first.** Confirm both guard lists are the same 12 variants and both
  `module_export_name` bodies are identical (re-done at HEAD). Make an explicit cross-crate
  ownership decision: frontend is the lower owner for `module_export_name`, binder is the lower
  owner for the statement classifier, and `pub(crate)` cannot serve either consumer across a crate
  boundary. Do not add a root utility module or widen source-identity modules into AST grab bags.
- **Scope.** One frontend-owned `module_export_name` consumed by check (the namespace-local
  `metadata_name` may stay — it returns `MetadataName`, a different contract) and one binder-owned
  `is_plain_runtime_statement(&Statement) -> bool` predicate consumed by check.
- **Acceptance / witness.** Same gates; jscpd re-run reports zero production cross-file
  clones touching `crates/typokat-binder/src/binder/namespace.rs`.
- **Touch points.** `crates/typokat-frontend/src/frontend.rs`,
  `crates/typokat-check/src/check/checker/mod.rs`,
  `crates/typokat-check/src/check/checker/namespace_values.rs`,
  `crates/typokat-binder/src/binder/namespace.rs`.

### WU5 — split `crates/typokat-binder/src/binder/namespace.rs` into a submodule directory (effort M)

- **Problem.** One file hosts seven concerns plus 3.3k lines of tests; navigation and
  review-scoping suffer (the checker side is already split into `crates/typokat-check/src/check/checker/` submodules).
- **Verify first.** Confirm all external consumers import via `crate::binder::namespace::…`
  so a `mod.rs` with `pub use` re-exports keeps every consumer path unchanged; confirm
  in-file tests only need `super::` access (they do — they exercise private items, so tests
  must stay inside the module tree). ⚠ `crates/typokat-binder/src/binder/references.rs` glob-imports
  (`use super::namespace::*`), so the re-export list must be exhaustive — a missing `pub use`
  fails only under `cfg(test)`; build with `cargo test --no-run` before trusting `cargo build`.
- **Scope.** Move-only split beneath the current
  `crates/typokat-binder/src/binder/namespace.rs` module:
  - `mod.rs` — id newtypes, `SourceFileKind`/`ModuleBindingContext`/`CompilationUnit`,
    `has_external_module_indicator`, and `pub use` re-exports (public API unchanged);
  - `metadata.rs` — the record/enum layer (`Namespace`, `NamespaceFragment`,
    `NamespaceMember`, `Merge*`, `Global*`, `Deferred*`, `ExportContext`, `Umd*`, …);
  - `table.rs` — `NamespaceTable` storage, accessors, `classify`, instance states,
    dormant-storage candidates;
  - `snapshot.rs` — the snapshot round-trip layer (`snapshot_primary`,
    `from_snapshot_primary`, `validate_snapshot_primary_for_classification`,
    `validate_snapshot_canonical`), new since the plan and cleanly separable from `table.rs`;
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
- **Acceptance / witness.** Same gates; `git diff` outside `crates/typokat-binder/src/binder/namespace*` is empty
  except `crates/typokat-binder/src/binder/mod.rs`; no file in the new directory exceeds ~1 500 lines
  (tests excluded — ~4 800 post-WU1 implementation lines over nine files leaves comfortable
  headroom).
- **Touch points.** `crates/typokat-binder/src/binder/namespace.rs` → the proposed submodule tree,
  `crates/typokat-binder/src/binder/mod.rs`.
  ⚠ `crates/typokat-binder/src/binder/references.rs` may need its glob import narrowed.

## Out of scope (explicit)

- **Dormant substrate removal.** The 17 externally-unconsumed metadata types
  (`MergeClassification`/`MergeSlot*` internals, `AliasContext`, `Declaration*Facts`,
  `Import*`, `Deferred*Kind`, …) are reserved by the 2026-07-15 sprint for backlog `15`/`82`/`14`
  and are pinned by direct tests. Deleting or trimming them is a scope decision for those
  backlogs, not a refactor.
- **The snapshot round-trip layer.** It lands in its own file under WU5 and is otherwise
  untouched — no re-encoding, no schema change, no ordering change. It belongs to the active
  full-library sprint; this refactor only moves it.
- **Perf cleanups** (linear scans in `standalone_merge_record` at
  `crates/typokat-binder/src/binder/namespace.rs:1666` and `root_merge_record` at `:2320`, and the linear `.find`
  cluster at `:1585`/`:1678`/`:1779`/`:1896`/`:1994`): O(n²) patterns that are harmless at
  current sizes; index them only with profiling evidence, per the ADR-0001 profiling-gate
  spirit. ⚠ The full-library work has raised the input sizes these scans see — if profiling
  under the full `lib.d.ts` base flags one, that is a backlog item for the perf sprint, not a
  scope expansion here.
- **Repo-wide id-newtype `index()` macro**: 13+ occurrences across `binder`/`types`/`check`
  follow the same idiom; a macro is a repo-wide convention change with churn beyond this
  sprint's theme.
- **Other cross-file clones** found by jscpd outside the namespace theme — the repo-wide count
  has grown from 203 to 295 rust clones as the full-library work landed, and the new
  `crates/typokat-check/src/check/checker/wu0b_*` / `*_spec.rs` modules carry much of it. Cleaning those up is a separate
  sprint; this one is scoped to the nine clones touching `crates/typokat-binder/src/binder/namespace.rs`.

## Decisions

- Pure refactor contract: zero behavior change; every WU is gated on unchanged
  `cargo test`, focused conformance, and official-suite `run --check` (zero scoreboard
  movement). No new public API; `mod.rs` re-exports preserve all consumer paths.
- Dedup lands **before** the split (WU1–WU4 reviewed on the familiar single-file layout;
  WU5 then moves already-clean code), each WU as its own commit.
- No ADR needed — no architectural boundary, data flow, or invariant changes; this file is
  the record.

## Sequencing

1. **Gate satisfied:** the namespace sprint's WU7 adversarial review and closure landed before
   this refactor starts.
2. ⚠ **New blocking gate:** do not start while
   [`sprint-2026-07-21-full-lib-performance-cutover.md`](sprint-2026-07-21-full-lib-performance-cutover.md)
   is active. It is rewriting the exact three files this refactor touches
   (`crates/typokat-binder/src/binder/namespace.rs`,
   `crates/typokat-binder/src/binder/bind.rs`, and
   `crates/typokat-check/src/check/checker/namespace_values.rs` took `+2 852/-412` lines between
   the plan and HEAD), and a
   whole-file split landing mid-flight turns every one of its edits into a conflict. This sprint
   waits for that one to close, then re-verifies the ref block again before WU1.
3. WU1 → WU2 → WU3 → WU4 (independent of each other after WU1; may be one subagent run,
   separate commits) → WU5 last.
4. Per dev-method: implementation via a subagent; the leader verifies gates and commits.
   A second read-only agent re-runs jscpd and spot-diffs the moved code as the review step
   (full adversarial review is not required for a behavior-preserving refactor, but the
   move-only claim of WU5 must be independently checked).

## Run log

<!-- Append as you work. -->

- **2026-07-22 — refs re-verified at `36c3695`, no WU started.** Every line reference in the
  original plan was stale after the full-library/snapshot work; the ref block above is
  re-measured. Substantive drift, beyond line numbers: `classify` grew from six ordering blocks
  to seven, each doubled by a `library_order` branch (WU3 is bigger and now has
  `validate_snapshot_canonical` as a witness); a snapshot round-trip layer appeared in
  `NamespaceTable` (WU5 gains a `snapshot.rs` slice); the checker's `declaration_owner_scope`
  became generic over `Ticket`; the `compilation_global` ⚠ from 2026-07-16 is resolved (same
  scope, `Option` only means "not yet bound"); the namespace module's consumer set expanded from
  three files to ~20, one of them (`crates/typokat-binder/src/binder/references.rs`)
  glob-importing it; and the
  `OriginalModuleOrdinal` out-of-scope note is obsolete — that copy was consolidated into
  `crates/typokat-core/src/source.rs`, so the bullet is dropped. Measured removable duplication in WU1 is ~335 lines
  (the plan's ~430 was an over-estimate). Added a blocking sequencing gate on the active
  full-lib performance sprint.
