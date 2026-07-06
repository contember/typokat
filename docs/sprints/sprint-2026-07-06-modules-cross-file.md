# Sprint — modules / cross-file correctness slice (2026-07-06)

**Goal.** Ship backlog [`15`](../backlog/15-modules-imports.md)'s first slice:
correctness-first whole-repo checking for local relative modules, named imports,
and named exports in one serial type universe.

**Theme.** Break the single-file ceiling without taking on the Stage 2 parallel
type-identity knot. Success is a multi-file fixture corpus whose imports/exports
resolve across files, whose cross-file values/types keep identity, and whose
implementation clearly documents that parallel `check_files` remains the old
per-file API until Stage 2.

## Refs re-verified at HEAD (2026-07-06)

- ✔ **Single-source entry point** — `check_source` parses one AST and calls
  `check_program` with one fresh `Interner` (`src/driver.rs:40`,
  `src/driver.rs:64`).
- ✔ **Parallel file API is explicitly independent** — `check_files` fans out
  `check_source`; the docs say no cross-file resolution exists today and per-file
  interners are lossless only under that condition (`src/driver.rs:100`).
- ✔ **Prelude is the precedent for multiple compilation units in one universe** —
  the checker parses/binds the prelude before user code, resolves it into the
  same store, then checks the user unit (`src/check/checker/mod.rs:261`).
- ✔ **Binder only knows ordinary top-level declarations today** — import/export
  statement wrappers are not matched; declarations are bound in
  `bind_type_declarations` / `bind_statement` (`src/binder/bind.rs:177`,
  `src/binder/bind.rs:213`).
- ✔ **Conformance is single-file today** — every enabled fixture path is sent to
  `check_source`; project fixtures need a harness extension before M29 can flip
  on (`tests/conformance.rs:90`, `tests/conformance.rs:123`).
- ✔ **Backlog requires two explicit slices** — first correctness-first
  whole-repo checking, then parallel/cross-universe identity; do not silently mix
  them (`docs/backlog/15-modules-imports.md:18`).
- ✔ **Official-suite module gates are available for ratcheting later** —
  `run --check` is green at HEAD; current OOS buckets include `syntax:module`
  and `multifile`, so lift gates only after the hand-written corpus and review
  pass.

## Work units

### WU1 — Disabled M29 project corpus + harness shape (effort M)

- **Problem.** The corpus is flat per `.ts` file, so a module feature has no
  acceptance harness.
- **Verify first.** Confirm `m29_modules/` is registered `false` and `cargo test`
  is behavior-neutral before implementation.
- **Scope.** Add project-fixture support: subdirectories under
  `tests/cases/m29_modules/`, markers inside every file, deterministic display
  paths, and an explicit project check path instead of `check_source`.
- **Acceptance / witness.** The committed M29 corpus flips `true` only when the
  implementation lands; before that, all existing tests remain unchanged.
- **Touch points.** `tests/conformance.rs`, `tests/cases/README.md`,
  `tests/cases/m29_modules/**`.

### WU2 — Module graph and local relative resolver (effort M)

- **Problem.** `typokat check a.ts b.ts` checks both files independently, so
  `import { x } from "./a"` never binds `x`.
- **Verify first.** Probe at HEAD: importer reports `TK2304` on use of the
  imported name.
- **Scope.** Resolve only local relative `./` and `../` specifiers to `.ts`
  files; build an acyclic dependency order; report `TK2307` for unresolved
  modules; keep packages, `node_modules`, `tsconfig`, and `.d.ts` out.
- **Acceptance / witness.** `missing_resolution/` reports exactly the module
  resolution diagnostic without cascading unresolved-name noise.
- **Touch points.** `src/driver.rs`, CLI path handling in `src/main.rs`,
  diagnostics definitions/rendering if a new code is needed.

### WU3 — Named export surface extraction (effort L)

- **Problem.** Other files need lifetime-free value/type surfaces; `TypeId`s are
  only meaningful in the same interner.
- **Verify first.** Inspect OXC AST wrappers for `export const`, `export type`,
  `export interface`, `export class`, and `export { x as y }`.
- **Scope.** In one serial `Interner`, check dependency modules first, then
  extract exported value/type slots for top-level declarations and simple export
  specifier lists. Report `TK2305` for a named import missing from the resolved
  module.
- **Acceptance / witness.** `basic_named/` and `export_list/` pass with only the
  expected assignment / missing-export errors.
- **Touch points.** `src/binder/`, `src/check/checker/`, `src/driver.rs`.

### WU4 — Synthetic import binding in importers (effort L)

- **Problem.** The importer must resolve identifiers and type references through
  imported names while preserving shadowing and separate value/type slots.
- **Verify first.** Cross-check `import type` and class imports against
  `tsc --strict`.
- **Scope.** Bind named imports into the importer module scope before user checks:
  value imports fill value slots, `import type` fills type slots only, classes can
  fill both where imported as values, and local declarations shadow imported
  names by ordinary scope order. Keep default imports, namespace imports, star
  imports/re-exports, dynamic import, CommonJS, and cycles out.
- **Acceptance / witness.** `types_and_classes/` preserves generic type aliases,
  interface members, and nominal class identity across files.
- **Touch points.** `src/binder/`, `src/check/checker/context.rs`,
  `src/check/checker/annotations.rs`, `src/check/checker/expr.rs`.

### WU5 — Independent adversarial review + ratchet (effort M)

- **Problem.** Module plumbing can easily create silent FNs through a missing slot,
  wrong shadowing, or cross-file identity split.
- **Verify first.** Re-run every M29 fixture with `tsc --strict` and classify any
  divergence before review.
- **Scope.** Review import/export AST coverage, type/value slot separation,
  `import type`, class nominality, local shadowing, missing module/export
  diagnostics, deterministic order, and unchanged `check_files` behavior for the
  old per-file API.
- **Acceptance / witness.** `cargo test`, `cargo clippy --all-targets -- -D
  warnings`, and official-suite `run --check` are green; any intentional gate lift
  is saved only after zero regressions.
- **Touch points.** Review probes may live in scratch only; ratchet touches
  `tooling/official-suite/scoreboard.txt` only if intended progress appears.

## Out of scope (explicit)

- Node/TypeScript resolver modes, `package.json`, `node_modules`, `baseUrl`,
  `paths`, project references, `.tsx`, `.d.ts`, and full `lib.d.ts`.
- Default imports, namespace imports, `export *`, re-export-from, dynamic import,
  CommonJS `require`, `export =`, `import =`, ambient modules, module
  declarations, and cyclic module graphs.
- Parallel Stage 2 cross-universe identity, stable structural hash, shared
  growing interner, and incrementality.
- Broad official-suite module gate lifting before the hand-written M29 corpus and
  adversarial review are green.

## Decisions

- **Correctness-first uses one serial type universe.** This deliberately chooses
  backlog `15` slice 1, not parallel Stage 2; it keeps cross-file `TypeId`
  identity simple and reversible while proving semantics.
- **Resolver scope is local relative `.ts` only.** Real-world package resolution is
  a separate feature; this sprint needs enough module graph to test checker
  semantics, not a full TS project loader.
- **Project fixtures are subdirectories.** A flat file cannot express imports, so
  M29 introduces project fixture directories while preserving old single-file
  fixture behavior.

## Sequencing

1. Commit disabled corpus + sprint plan.
2. Implement WU1/WU2 first: harness and graph shape.
3. Implement WU3/WU4 as one worker-owned checker slice if the AST surface is
   clear; otherwise split after the export extractor lands.
4. Run independent adversarial review; fix; then commit implementation.
5. Ratchet docs/scoreboard only for intentional, verified movement.

## Run log

<!-- Append as you work. -->

- 2026-07-06: Implemented slice-1 project checking as an additive serial API:
  `check_project` resolves only provided local relative `.ts` files, keeps
  `check_source`/parallel `check_files` unchanged, and checks dependency-ordered
  modules in one interner. Exported declarations are unwrapped consistently in
  binder, type-decl reserve, flow build, and statement checking; simple export
  lists resolve the local symbol slots rather than trusting syntax kind. Type-decl
  filling now has a range-aware path so each project module lowers under its own
  module scope while `next_type_param`/`next_class_id` remain project-global.
- 2026-07-06: Enabled `m29_modules` project fixtures in conformance. Adjusted one
  M29 `TK2345` marker substring to the checker's existing argument diagnostic
  wording after cross-checking the project fixtures with `tsc --strict --noEmit
  --module esnext --moduleResolution bundler`.
- 2026-07-06 review fix: value-position identifier resolution now requires a
  value slot, so type-only imports/exports used as values report `TK2304` while
  missing-module/export value placeholders still suppress cascades. The CLI
  `check` command now routes supplied paths through `check_project`; `check_files`
  remains the old independent parallel API.
