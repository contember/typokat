# Sprint — real-project preview (2026-07-12)

**Goal.** Close backlog `72`: `typokat` can be pointed at one pinned, genuinely small strict
TypeScript project and produce a deterministic, honest differential result through its public CLI.

**Theme.** The serial cross-file checker, bounded source-backed prelude, and exhaustive incomplete
channel are already shipped. This sprint connects those pieces to a narrow project workflow:
select and freeze a witness that fits the implemented model, discover its configured roots, resolve
its imports through `oxc_resolver` under the 1.0 Bundler profile, account for every selected file and
result channel, and ratchet a clean baseline plus seeded errors in CI. It is an early preview slice,
not the full `lib.d.ts` or module-semantics milestone. Physical resolver ownership follows
[`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md).

## Refs re-verified at HEAD (2026-07-12)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The public CLI currently accepts only explicit file paths and reads every path before invoking
  the serial project checker; there is no directory or `tsconfig.json` discovery path —
  `src/main.rs:3-4`, `src/main.rs:19`, `src/main.rs:57-64`, `src/main.rs:133-148`.
- ✔ `driver::check_project` already coordinates frontend parsing and dependency ordering with
  checking through one run-local `Interner`, preserving the correctness-first Stage 0.5 type universe —
  `crates/typokat-driver/src/driver.rs:145-159`,
  `crates/typokat-driver/src/driver.rs:166-225`, `docs/reference/architecture.md:386-391`.
- ⚠ The current resolver scans only local relative named imports. An extensionless specifier gains
  `.ts`, but `./foo.js` remains `foo.js`; default/namespace imports and non-relative specifiers are
  skipped by the scanner — `crates/typokat-frontend/src/frontend.rs:190-239`,
  `crates/typokat-frontend/src/frontend.rs:313-327`.
- ✔ Project-shaped conformance fixtures already exercise the serial driver, so synthetic config and
  Bundler resolver witnesses can extend an established test seam —
  `tests/conformance.rs:143-154`, `tests/cases/m29_modules/`.
- ✔ The prelude is one checked source unit in the same type universe and currently exposes only the
  admitted utility aliases plus bounded `console` and numeric `Math` values; no project-only ambient
  shim is needed or allowed — `crates/typokat-check/src/check/checker/mod.rs:42-43`,
  `crates/typokat-check/src/check/checker/mod.rs:80-126`, `crates/typokat-check/src/prelude.ts:1-41`.
- ✔ Incomplete surfaces are a separate deterministic, deduplicated channel and aggregate per project
  file, while exit `3` outranks ordinary diagnostics — `crates/typokat-diagnostics/src/diagnostics/incomplete.rs:59-103`,
  `tests/incomplete_outcome.rs:127-178`, `src/main.rs:170-195`.
- ✔ The executable completion manifest still marks `D-real-project-preview` incomplete with backlog
  `72` as its owner; the surface-accounting prerequisite and minimal prelude are complete —
  `docs/backlog/completion-1.0.toml:559-576`.
- ✔ CI already runs the exact local Rust gates and the official-suite identity ratchet; the preview
  runner can add a separate black-box job without weakening that checker-wide gate —
  `.github/workflows/ci.yml`.
- ⚠ The README still correctly warns that an unknown npm/Bun/Node project is not a completeness
  claim. Closure must replace that limitation only with the precisely pinned preview promise —
  `README.md:158-174`.

## Work units

### WU0 — pin the witness and commit the preview contract (effort L)

- **Problem.** Backlog `72` defines the gate, but no public project, CLI/output contract, clean
  oracle result, mutation set, allowlist, or numeric threshold is frozen. Implementation before
  those facts exist could select a witness to fit the code or grow a project-specific compatibility
  layer.
- **Verify first.** Evaluate genuinely small public strict-TypeScript candidates at immutable commits.
  For each candidate, run its documented install and pinned `tsc --noEmit`, enumerate the selected
  `tsconfig` roots/import forms/ambient names/AST surfaces, and probe the same files with the current
  explicit-file CLI. Reject candidates needing broad Node/Bun declarations, generic standard-library
  methods, a package graph, a non-Bundler resolution profile, unsupported type semantics, or a local
  declaration shim. Confirm the repository license permits the chosen fetch/use pattern.
- **Scope.** Select one candidate and record its repository URL, full commit SHA, license, lockfile
  digest, install command, exact `tsconfig.json`, TypeScript oracle version, configured roots, local
  import graph, prelude names, and exercised language/resolver surfaces in a checked-in preview
  descriptor. Pin the public directory/tsconfig CLI syntax and the deterministic summary schema with
  black-box acceptance snapshots. Pin `moduleResolution: "bundler"`, all resolver condition/options,
  and the `oxc_resolver` version/options as part of the oracle contract. Define three
  source-preserving mutations: assignability, bad call argument, and missing member. Record the clean
  baseline, mutation identities, and an allowlist mapping every accepted mismatch or unsupported
  identity to a live backlog/divergence owner.
- **Threshold gate.** The initial contract is zero actionable false positives, zero unresolved
  modules, zero skipped files/forms, zero unsupported forms in the chosen witness, and zero missed
  seeded diagnostics. If no honest candidate meets those numbers, stop after publishing the evidence
  and ask before relaxing scope; do not alter the prelude or checker to make a candidate fit.
- **Acceptance / witness.** One behavior-neutral spec commit precedes all implementation. The
  descriptor is reproducible from an empty cache, the pinned `tsc` baseline is clean, every mutation
  produces its expected `TS` identity, expected typokat/project identities are fully specified, and
  the disabled black-box contract fails at old HEAD only because project discovery/Bundler behavior
  is absent.
- **Touch points.** New `tooling/project-preview/` descriptor, baseline, mutation manifest, runner
  contract, and README; disabled `tests/cases/b72_real_project_preview/` synthetic controls;
  `tests/cases/README.md`; `tests/conformance.rs`; black-box CLI test contract.

### WU1 — deterministic project discovery and public CLI (effort L)

- **Problem.** `typokat check` treats every positional input as a readable `.ts` file. It cannot
  discover a config from a directory, expand `files`/`include`/`exclude`, or report which roots it
  selected.
- **Verify first.** From WU0's pinned syntax and fixture config, enumerate path normalization, config
  lookup, glob ordering, duplicate roots, excluded roots, empty matches, malformed config, and
  `extends`/reference cases. Compare the selected root set and resolved config with the pinned
  TypeScript Bundler oracle and confirm the existing explicit-file invocation remains byte-for-byte
  compatible.
- **Scope.** Add a narrow project/config orchestration component outside the checker core. Use
  `oxc_resolver` to find and resolve the directory or explicit `tsconfig.json`, including config
  inheritance/reference behavior it supports; do not implement a second config resolver. Typokat
  enumerates the WU0-pinned `files`/`include`/`exclude` root set where the crate does not expose a
  complete set, normalizes and sorts it deterministically, classifies every configured source as
  selected or explicitly unsupported, and hands owned `FileInput`s to the existing serial
  `check_project` path. Render the selected config/root set and project-level unsupported notices
  separately from type diagnostics and AST incomplete records. Preserve the current explicit `.ts`
  file-list mode and four-way exit contract.
- **Stop gate.** If faithful parsing of the witness config requires a general compiler-option model,
  changes to checker scopes/type identity, or silent fallback for an unsupported field, stop and ask.
- **Acceptance / witness.** Focused unit and black-box tests prove directory/config discovery,
  deterministic include/exclude expansion, duplicate suppression, pinned inheritance/reference
  behavior, missing/malformed config reporting, unchanged file-list behavior, and exact root/config
  summary snapshots. All roots in the synthetic project are accounted for once; unsupported config
  fields/profiles cannot fall back silently.
- **Touch points.** `src/main.rs`, a narrow project/config discovery module, `src/lib.rs`, focused
  unit tests, `tests/incomplete_outcome.rs`, and WU0's synthetic controls.

### WU2 — witness-bounded Bundler resolution and project accounting (effort L)

- **Problem.** The serial resolver cannot map a runtime-style `./foo.js` specifier to a supplied
  `foo.ts`/`foo.d.ts`, silently ignores non-relative imports, and exposes no project summary of
  checked/skipped files or unresolved modules.
- **Verify first.** Inventory every import/export form in the selected witness and build focused
  controls for `.js` to `.ts`, `.js` to `.d.ts`, extensionless imports, missing targets, ambiguous
  candidates, non-relative/package specifiers, `paths`, package conditions, and stable dependency
  order. Cross-check each admitted resolution against the WU0-pinned TypeScript Bundler oracle.
  Include dependency-gap sentinels for manually supplied tsconfig path mapping and simplified
  `typesVersions` selection.
- **Scope.** Configure the pinned `oxc_resolver::resolve_dts` API as the only physical resolver for
  the witness. Retain the existing local `.ts` semantic behavior, classify every encountered
  specifier as resolved, unresolved, or explicitly unsupported, and do not add fallback path probes.
  Typokat loads resolved files, constructs stable dependency order, and supports only the
  import/export semantics admitted by WU0. Aggregate deterministic project results by normalized
  relative path: roots selected, files checked/skipped, resolved/unresolved modules, project
  unsupported notices, AST incomplete identities, and diagnostics by code/file/line. Do not collapse
  any channel to aggregate counts.
- **Acceptance / witness.** Enabled synthetic fixtures prove every admitted Bundler specifier
  resolves identically to pinned `tsc`, missing/unsupported imports and known crate gaps cannot
  disappear cleanly, all files share the existing serial type universe, repeated project runs are
  byte-identical, and the summary matches the pinned schema. The selected real witness has zero
  unresolved modules, skipped/unsupported files or forms, and unclassified diagnostics.
- **Touch points.** Dependency/configuration, `crates/typokat-driver/src/driver.rs`,
  project result/summary structures,
  `src/main.rs`, `tests/cases/b72_real_project_preview/`, `tests/conformance.rs`, and black-box CLI
  snapshots.

### WU3 — reproducible clean/mutation ratchet and CI gate (effort L)

- **Problem.** A one-off successful checkout is not a durable preview promise. The repository needs
  a reproducible runner that detects identity swaps, lost seeded diagnostics, resolver drift, and
  unsupported-surface drift.
- **Verify first.** From an empty cache, fetch the pinned witness, verify its commit and lockfile
  digest, install exactly as declared, run the clean oracle, apply each mutation independently, and
  compare two normalized typokat runs byte-for-byte. Exercise corrupt cache, digest mismatch,
  unexpected exit, timeout, and stale/missing baseline paths.
- **Scope.** Implement the stdlib-minimal black-box runner around the public CLI. It verifies the
  pinned checkout, materializes mutations without changing the source fixture permanently, records
  normalized identities rather than counts, and checks the committed clean/mutation baseline plus
  allowlist and thresholds. Add a CI job that uses the same command and cache contract as local use.
  Keep the checker-wide official-suite ratchet separate and unchanged.
- **Acceptance / witness.** The clean witness passes with all WU0 thresholds, each mutation reports
  the expected `TK` identity at the mutated site, identity swaps/losses and newly unresolved or
  unsupported surfaces fail the ratchet, two fresh runs are byte-identical, runner unit tests pass,
  and CI invokes the documented public command path.
- **Touch points.** `tooling/project-preview/`, its committed normalized baseline and tests,
  `.gitignore`, `.github/workflows/ci.yml`, and public/tooling documentation.

### WU4 — independent adversarial preview review (effort L)

- **Problem.** A curated project can look green while discovery omits a root, resolver paths feed
  error types, mutations miss the intended checker path, or normalization hides nondeterminism.
- **Verify first.** A reviewer independent of WU1-WU3 starts from the committed WU0 contract and
  reviews the uncommitted implementation without relying on its rationale. Reproduce the witness
  from an empty cache and rerun its exact pinned `tsc` oracle.
- **Scope.** Adversarially probe omitted/duplicate/glob-ordered roots, config path traversal,
  inheritance/references/options, `.js`/`.ts`/`.d.ts` ambiguity, missing and package imports,
  `paths`, package conditions, the known `typesVersions` limitation, type-only/value import
  boundaries, dependency cycles, prelude shadowing, parse diagnostics, diagnostic-plus-incomplete
  precedence, path/timestamp/cache nondeterminism, mutation isolation, allowlist wildcarding, and
  same-count identity swaps. Cross-check fresh resolver probes against pinned `tsc` in Bundler mode
  and semantic probes against `tsc --strict`. Any FAIL receives a focused witness and returns to the
  implementation agent; the same independent reviewer rechecks the remediation.
- **Acceptance / witness.** Explicit PASS with commands, probes, and identity totals; zero omitted
  configured file, false-clean import, missed seeded error, unowned mismatch, mutable input, or
  unexplained false positive. No relation/type-store/CFG invariant changes and no broad resolver or
  ambient expansion.
- **Touch points.** Read-only WU0-WU3 diff, fresh-cache runner output, synthetic fixtures, and
  scratch `tsc 6.0.3 --strict` probes; focused regression fixtures only for confirmed failures.

### WU5 — public claim, manifest transition, and closure (effort M)

- **Problem.** Shipping the runner without updating the completion contract and limitations would
  leave the preview either undiscoverable or overstated.
- **Verify first.** Run the full local gate, the preview runner from an empty cache twice, and the
  fresh official-suite `run --check`. Audit every reference to backlog `72`, the active sprint, and
  the old unknown-project limitation.
- **Scope.** Document the exact public project CLI, summary channels, exit behavior, pinned witness,
  reproducibility command, Bundler-only profile, `oxc_resolver` version/options, thresholds, and
  narrow limitations. Mark `D-real-project-preview` complete, delete backlog `72`, preserve
  `14`/`15`/`16` ownership, stamp the sprint outcome, archive it, and update the docs indexes. Record
  clean/mutation identity totals and any owned safe-direction or explicitly unsupported resolver
  mismatch in the outcome.
- **Acceptance / witness.** `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D
  warnings`; `cargo build --release`; project-preview runner tests and fresh-cache ratchet; official-
  suite unit tests and freshly fetched `run --check` all pass. No live reference claims general npm,
  Node/Bun ambient, package, or full-lib support.
- **Touch points.** `README.md`, project-preview docs, `docs/reference/{architecture,scope,
  divergences}.md` where behavior changed, `docs/backlog/completion-1.0.toml`,
  `docs/backlog/README.md`, `docs/INDEX.md`, `docs/sprints/README.md`, `docs/archive/`, and backlog
  `72` at closure.

## Out of scope (explicit)

- Full `lib.d.ts`, lib discovery/loading, or a broader ambient prelude — backlog
  [`14`](../backlog/14-libdts-loading.md).
- General Bundler package/`node_modules`/`@types` coverage, package conditions/layouts,
  declaration packages, project enumeration, and broad import/export semantics — backlog
  [`15`](../backlog/15-modules-imports.md).
- NodeNext, Node16, classic Node, CommonJS-specific, and other alternate resolution profiles —
  explicitly deferred by [`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md),
  not approximated through Bundler.
- Cross-file parallel type identity or incrementality — backlogs
  [`16`](../backlog/16-parallelism-type-universe.md) and
  [`17`](../backlog/17-incrementality.md).
- The later full-stack witness. Deptective remains only a candidate: it is gated on `14` + `15`,
  must use a meaning-preserving Bundler witness config, and is not evidence for this bounded preview.
- Fixing unrelated checker semantics discovered while screening candidates. Reject/rescope the
  candidate or link the mismatch to an existing backlog; do not add a witness-only shim, permissive
  fallback, `any`, cast, or diagnostic suppression.
- General compiler-option validation and `5xxx`/`6xxx` tsc-compatible option/CLI diagnostics; the
  narrow project notices are structured preview accounting, not a claim of compiler parity.

## Decisions

- Backlog `72` remains one sprint because the public promise is vertical: discovery, resolution,
  accounting, differential mutations, and CI are not independently shippable evidence.
- WU0 selects the witness before behavior changes and freezes every user-visible CLI/summary detail
  in the spec commit. The candidate must fit the shipped model; the implementation does not grow the
  model to fit the candidate.
- Bundler is the sole 1.0 resolver profile. `oxc_resolver` is the physical-resolution/config
  authority; typokat owns source-root enumeration, module graph and semantics, `.d.ts` checking,
  accounting, diagnostics, and determinism. Resolver gaps become upstream work or explicit
  unsupported identities, never local fallback probes.
- Start at zero for every trust threshold. A proposal to accept an unsupported form, unresolved
  module, skipped file, missed mutation, or actionable false positive changes the preview promise
  and requires approval before implementation continues.
- Use the crate's resolved tsconfig model for inheritance/references. Any field or root-enumeration
  behavior not admitted by WU0 is explicitly unsupported; silently ignoring it is forbidden.
- Reuse `driver::check_project` and its one serial `Interner`. Project discovery and summaries are
  orchestration around that boundary, not a second checker path or a new type universe.
- Keep project notices, AST incomplete records, parse errors, and `TK` diagnostics as distinct
  identities. The normalized ratchet may aggregate them for display but never erase their channel,
  file, or identity.
- Stop rather than improvise if WU0 cannot find a zero-threshold candidate or implementation needs a
  broad package/lib/config model, a project-specific ambient declaration, or a checker architecture
  change.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Leader commits the immutable witness/spec contract before behavior changes. |
| 2 | WU1 | Implementation subagent lands discovery/CLI against synthetic specs; leader verifies. |
| 3 | WU2 | Same implementation context extends only the admitted resolver/accounting seam. |
| 4 | WU3 | Ratchet/CI lands only after public CLI output is stable and repeated runs match. |
| 5 | WU4 | Different independent reviewer; every FAIL is remediated and re-reviewed. |
| 6 | WU5 | Full gates, fresh-cache witness, identity audits, documentation, then archive/closure. |

WU0 candidate screening can parallelize repository/license inspection, oracle execution, and
current-checker surface inventory. WU1-WU3 are ordered because each freezes the input/output contract
consumed by the next unit. WU4 begins only after all implementation is available for adversarial
review.

Exact full gate: `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D warnings`;
`cargo build --release`; project-preview runner unit tests; two fresh-cache preview ratchet runs;
official-suite unit tests; fresh official-suite fetch and `run --check`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-12 — Plan grounded at `141ab10`; working tree clean. Backlog `72` is the roadmap's next
  executable item. HEAD confirms the existing serial type universe, bounded prelude, and exhaustive
  incomplete channel; the missing vertical pieces are project discovery, local NodeNext `.js`
  mapping, project-level identity accounting, and the reproducible witness ratchet.
- 2026-07-12 — WU0 BLOCKED at its zero-threshold witness gate after public-candidate screening.
  `0xaldric/is-strictly-seven@95011ae6321f8bf29d40e06006762960c0ec2086` is clean under both
  `tsc 6.0.3` and current typokat and admits `TK2322`/`TK2345`/`TK2339` mutations, but has one root,
  no import graph, and a 151-entry tooling lockfile, so it cannot witness the promised project path.
  `morkg/jabr@9415fdad8b98dc0f1aba09c8badc5fc209bc30ba` has eight roots and a minimal TypeScript
  lockfile, but requires unsupported array methods/ambient names and mapped/conditional/tuple
  semantics, producing diagnostics and incomplete records at HEAD. Other screened candidates
  (`Lulzx/tinypdf`, `vercel/async-sema`) require broad Web/Node surfaces. No implementation or
  prelude expansion started; continuing requires explicit approval to change the witness contract.
- 2026-07-13 — Resolver policy graduated to
  [`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md): the 1.0 witness now uses
  Bundler resolution through `oxc_resolver`; NodeNext/alternate profiles are deferred, and typokat
  retains module semantics and complete project accounting. The prior candidate-screening result
  remains valid; no witness or implementation has been selected by this documentation change.
