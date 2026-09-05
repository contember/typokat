# Project-local library contract corpus

This disabled corpus preserves focused ADR-0023 fixtures loaded explicitly by integration tests. It
does not alter the ordinary `583/699` conformance inventory.

The fixtures isolate:

- `reflect_packaged.ts` demands the packaged `Reflect.ownKeys` surface through ordinary `symbol`;
- `array_library_scc.ts` demands the `Array`/`ConcatArray`/`ReadonlyArray` publication closure and
  `Reflect` in one source;
- `mixed_group_{forward,reverse}` merge one library group from a script and a module
  `declare global` fragment in both source orders;
- `isolation_{a,b}` require two incompatible project-local augmentations with no cross-project leak.
- `official_assignment_compat_between_tuple_and_array.ts` pins one exact `IN` frame from the
  authenticated TypeScript commit in the official-suite manifest. Its tuple/Array relations give
  the demand trace an upstream semantic consumer instead of a local-only calibration.

Acceptance uses production `CheckerSession` execution and the named production-owned record census.
No alternate complete-source route or candidate trace is authoritative.

The focused gates also reuse, rather than duplicate, these shipped controls:

- `b14_full_lib_loading_project` and `b103_library_merge_correctness_project` for script globals,
  `declare global`, `globalThis`, legal/illegal merges, modules, order, and isolation;
- `b43_namespaces_declaration_merging` for class/function/namespace and recursive group behavior;
- `tests/wu7_production_cutover.rs` for the sole production lifecycle and filename semantics;
- `tests/b15_acyclic_source_reexports_cli.rs` for source re-exports;
- `tests/wu7_official_batch.rs` for same-process project isolation;
- `tests/fixtures/library-owned-records.txt` for the accepted full-library diagnostic census.

`locked-slice.toml` authenticates the smallest load-bearing declaration/dependency slice. The full
product must cover all packaged sources; the slice is only an audit witness, never a loading plan.
`oracle.toml` records the exact real-`tsc` outcomes and typokat divergences that established the WU1
semantic baseline.

## WU1 semantic baseline

Real TypeScript 6.0.3 and typokat now agree on `TS`/`TK2322` and `TS`/`TK2345` in the enabled
backlog-121 symbol fixture and on the marked `TS`/`TK2322` diagnostics here. The packaged
`Reflect.ownKeys` terminal publishes through the canonical project-local `symbol` identity.

The stopped feasibility run retained no named `(code, site)` multiset for its reported movement.
ADR-0028 replaced that count-only producer. The sole current authority is the named 703-row pin:
318 diagnostics and 385 incompletes.
