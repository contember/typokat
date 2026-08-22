# Project-local library candidate corpus

This disabled corpus freezes the acceptance boundary for ADR-0023. It does not alter the ordinary
`580/696` conformance inventory. Candidate integration tests load the projects explicitly.

The new fixtures cover gaps that the shipped frozen/sparse corpus does not isolate:

- `reflect_packaged.ts` demands the packaged `Reflect.ownKeys` surface through ordinary `symbol`;
- `array_library_scc.ts` demands the `Array`/`ConcatArray`/`ReadonlyArray` publication closure and
  `Reflect` in one source;
- `mixed_group_{forward,reverse}` merge one library group from a script and a module
  `declare global` fragment in both source orders;
- `isolation_{a,b}` require two incompatible project-local augmentations with no cross-project leak.

The candidate gate also reuses, rather than duplicates, these shipped controls:

- `b14_full_lib_loading_project` and `b103_library_merge_correctness_project` for script globals,
  `declare global`, `globalThis`, legal/illegal merges, modules, order, and isolation;
- `b43_namespaces_declaration_merging` for class/function/namespace and recursive group behavior;
- `tests/wu6_complete_source_cli.rs` for complete-source order parity and parse precedence;
- `tests/b15_acyclic_source_reexports_cli.rs` for source re-exports;
- `tests/wu7_official_batch.rs` for same-process project isolation;
- `tests/fixtures/library-owned-records.txt` for the accepted full-library diagnostic census.

`locked-slice.toml` authenticates the smallest load-bearing declaration/dependency slice. The full
product must cover all packaged sources; the slice is only an audit witness, never a loading plan.
`oracle.toml` records the exact real-`tsc` outcomes and current typokat divergences that WU1 must
close before enabling any candidate route.

## Known current divergence

Real TypeScript 6.0.3 reports `TS2322` and `TS2345` in the disabled backlog-121 symbol fixture and
reports the marked `TS2322` diagnostics here. Current typokat does not yet publish the real packaged
`Reflect` terminal through ordinary symbol lowering. WU1 must correct that semantic truth before
the candidate consumes it.

The stopped feasibility run reported 44 newly active library-owned records but retained no named
`(code, site)` multiset. WU0 could not reproduce that movement under a consistent symbol fix or a
controlled old-base/new-candidate symbol toggle. WU1 must derive and commit the actual named census
delta before changing the lowering, without assuming a count. A count-only assertion is not an
acceptable witness.
