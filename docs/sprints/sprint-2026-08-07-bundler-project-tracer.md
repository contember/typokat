<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 → <sha>,
> WU2 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — Bundler project tracer (2026-08-07)

**Goal.** Through the public CLI, check one pinned small public strict-TypeScript project with the
production default library and `oxc_resolver`, with complete deterministic project accounting and a
clean-plus-mutation differential ratchet.

**Theme.** The production TypeScript 6.0.3 full library and the serial cross-file checker are
shipped, but the CLI still accepts only already-known files and the frontend silently filters module
forms outside its local named-import slice. This sprint delivers the smallest honest Bundler project
workflow: specify and implement project discovery plus local physical resolution first, then select a
real public project that fits that proven surface. It closes backlog [`72`](../backlog/72-real-project-preview-readiness.md)
only. Backlog [`15`](../backlog/15-modules-imports.md) remains open for general package,
declaration, import/export, and cycle breadth.

This supersedes the terminated
[`2026-07-12 preview sprint`](../archive/sprint-2026-07-12-real-project-preview.md). That sprint put
the public witness before the resolver and stopped at WU0. The replacement does not relax its trust
thresholds; it removes the circular sequencing.

## Refs re-verified at HEAD (2026-08-07, `7e3c221`)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `oxc_resolver` is not integrated or present in the workspace dependency set —
  `Cargo.toml:30-55`, `crates/typokat-frontend/Cargo.toml:9-16`.
- ✔ The public `check` command accepts positional paths only, and reads each path directly as a file
  before invoking the project checker — `src/main.rs:29`, `src/main.rs:436-486`,
  `src/main.rs:499-523`.
- ✔ Production CLI checks use `check_project_once`, which parses the fixed full library and the user
  project into one complete-source run — `src/main.rs:58-69`,
  `crates/typokat-driver/src/driver.rs:224-319`.
- ✔ The existing serial project API accepts owned `Vec<FileInput>` and resolves only `./` / `../`
  specifiers among supplied `.ts` files — `crates/typokat-driver/src/driver.rs:199-222`.
- ✔ The frontend builds one project interner, an in-memory normalized-path map, and a deterministic
  dependency order from the already supplied files — `crates/typokat-frontend/src/frontend.rs:373-451`,
  `crates/typokat-frontend/src/frontend.rs:454-517`.
- ⚠ Module discovery is currently a silent-loss boundary: non-relative specifiers are skipped,
  only named `ImportSpecifier` rows are retained, and a relative specifier without an extension is
  rewritten locally to `.ts` — `crates/typokat-frontend/src/frontend.rs:537-579`,
  `crates/typokat-frontend/src/frontend.rs:675-689`.
- ⚠ Re-exports with a source are skipped, and the current DFS treats a visiting dependency as done
  without a supported cycle-publication protocol — `crates/typokat-check/src/check/checker/mod.rs:3551-3558`,
  `crates/typokat-frontend/src/frontend.rs:604-643`. Neither is part of this tracer.
- ✔ A missing import that reaches the checker emits `TK2307`, but forms filtered by the frontend
  never reach that guard. Complete pre-filter module accounting is therefore a binding acceptance
  condition — `crates/typokat-check/src/check/checker/mod.rs:3482-3494`.
- ✔ Driver output currently has per-file parse, diagnostic, and incomplete channels, but no project
  discovery/resolution summary. Its exact input/result coverage assembly is reusable —
  `crates/typokat-driver/src/driver.rs:106-165`,
  `crates/typokat-driver/src/driver.rs:841-908`.
- ✔ The project conformance seam sorts files, exercises both the shared-base and production
  complete-source routes, and compares exact result channels — `tests/conformance.rs:702-792`.
- ✔ The exact TypeScript 6.0.3 ES2025 full-host library is now the production default; the old
  candidate-screening result predates this closure — `README.md:183-208`,
  `docs/archive/sprint-2026-07-12-real-project-preview.md:292-309`.
- ✔ ADR-0007 assigns physical Bundler lookup to `oxc_resolver` and keeps root enumeration, module
  semantics, accounting, and determinism in typokat. It forbids local fallback probes and silent
  reinterpretation of another resolver profile —
  `docs/decisions/0007-bundler-resolution-via-oxc-resolver.md:37-68`.

## Work units

### WU0 — retire the deadlocked plan (effort S)

- **Problem.** The paused 2026-07-12 sprint has higher documentation precedence than the roadmap,
  but its witness-first order stopped before any resolver or project implementation landed.
- **Verify first.** Confirm the old run log contains only screening and policy work, inventory every
  live reference to its active path, and re-check the code facts above at current HEAD.
- **Scope.** Archive the old sprint with an explicit incomplete outcome, create this replacement,
  and refresh the sprint/archive/index/roadmap/public status without changing behavior or closing
  backlog `72`.
- **Acceptance / witness.** No live document calls the old sprint active; relative links resolve;
  docs lint adds no finding; `git diff --check` passes; the commit contains documentation only.
- **Touch points.** `docs/{sprints,archive}/`, `docs/INDEX.md`, `docs/backlog/README.md`, and
  `README.md`.

### WU1 — synthetic project and resolver contract (effort M)

- **Problem.** The current fixture corpus proves already-loaded local named imports, but not the
  public project input, physical Bundler resolution, pre-filter module accounting, or a stable
  project result. Selecting implementation details or a public witness first would repeat the old
  circular gate.
- **Verify first.** Against pinned `tsc 6.0.3 --strict --noEmit --module esnext
  --moduleResolution bundler`, record exact outcomes for directory and explicit-config input,
  deterministic roots, extensionless local imports, `./foo.js` → `foo.ts`, a missing local target,
  a bare specifier, default/namespace imports, source re-export, a two-file cycle, and a non-Bundler
  profile. Run the same project cases against pre-change typokat as the negative control.
- **Scope.** Commit a disabled synthetic corpus and black-box contract before implementation. Pin
  the public CLI syntax, normalized project-summary schema, admitted root-config shape, and identity
  for every root, checked file, resolved/unresolved specifier, unsupported form, parse error,
  incomplete surface, and `TK` diagnostic. Admit only local named imports plus declaration/local
  export forms already supported by M29. Every other module/config form is an explicit non-clean
  project notice.
- **Acceptance / witness.** One behavior-neutral spec commit precedes production changes. Old HEAD
  fails directory/config and `.js` substitution controls, while the oracle artifacts are
  reproducible and the negative controls prove the contract can see silent filtering and wrong
  target selection.
- **Touch points.** New `tests/cases/b72_bundler_project_tracer/`, black-box CLI specs,
  `tests/cases/README.md`, `tests/conformance.rs`, and a small checked-in resolver oracle descriptor.

### WU2 — deterministic project discovery substrate (effort M)

- **Problem.** `typokat check` treats each positional input as a source file. It cannot discover a
  config from a directory, enumerate configured roots, or say which config fields it did not
  consume.
- **Verify first.** Freeze WU1 cases for path normalization, directory/config disambiguation,
  duplicate and reordered roots, missing/malformed config, unsupported resolver profile, and the
  exact unchanged explicit-file invocation.
- **Scope.** Pin `oxc_resolver` and use its config discovery/parsing facilities behind a narrow
  internal project-orchestration boundary outside the checker core; do not add a local tsconfig
  parser. Resolve a directory or explicit `tsconfig.json`, consume only the root-selection shape
  admitted by WU1, and normalize/sort selected roots. Keep this component unreachable from the
  public CLI and production driver until WU3 atomically adds complete module accounting. Preserve
  explicit file-list mode. Unknown or unconsumed config forms are typed project notices in the
  internal result; they must not be silently ignored.
- **Stop / falsifier.** Stop if the admitted slice requires a general compiler-option model, a
  second checker/type universe, changes to scope/type identity, or silent fallback for a config
  field.
- **Acceptance / witness.** Focused unit tests prove exact root coverage, deterministic ordering,
  and malformed/unsupported reporting. Black-box guards prove directory/config input is still
  rejected by the public CLI and explicit file-list behavior is byte-identical. Removing or
  duplicating a configured root breaks the internal witness; a source-level reachability guard
  fails if production dispatch exposes the substrate before WU3.
- **Touch points.** `Cargo.toml`, `Cargo.lock`, `crates/typokat-frontend/Cargo.toml`, `src/main.rs`,
  a focused frontend-owned project/config module, `crates/typokat-driver/src/driver.rs`, CLI tests,
  and `tests/workspace_layout.rs` if source ownership changes.

### WU3 — atomic public Bundler route and module accounting (effort L)

- **Problem.** The frontend performs local path joining over already-loaded files and silently
  drops non-relative/default/namespace forms. That is neither Bundler resolution nor trustworthy
  project accounting.
- **Verify first.** Re-run every WU1 resolution case against the WU2-pinned `oxc_resolver` version
  and `tsc 6.0.3` Bundler mode. Confirm resolver options and known limitations before wiring lookup.
  Reverse input and filesystem enumeration order and record exact normalized identities.
- **Scope.** Use the pinned dependency's `resolve_dts` API as the only physical resolver for every
  admitted specifier. For this dependency-free tracer, resolved targets must be configured local
  roots; do not dynamically admit packages or declaration layouts. Inventory every module
  declaration before filtering, classify every specifier/form as resolved, unresolved, or
  unsupported, construct a deterministic local graph/order, and expose a separate project summary
  while reusing the existing serial semantic route. In the same reviewed commit, wire directory and
  explicit-config inputs into the public CLI. The same inventory must guard the existing explicit
  file-list route: admitted file-list output stays byte-identical, but a currently filtered module
  form becomes explicit non-clean output rather than preserving a false negative. No public project
  mode may exist without this accounting boundary.
- **Stop / falsifier.** Stop if correctness requires a local fallback resolver, package/`@types`
  loading, default/namespace/star/re-export semantics, supported module cycles, a second parse or
  publication path, or Stage-2 cross-file identity.
- **Acceptance / witness.** Every admitted resolution matches pinned `tsc`; missing and unsupported
  cases are non-clean and preserve normalized file/specifier identity; both production/shared
  semantic routes retain exact result coverage; directory/config CLI contracts and the four-way exit
  behavior pass; repeated runs and reversed enumeration are byte-identical. The raw conformance row
  stays disabled because it cannot observe config or summary behavior; the black-box contract is
  unignored in this atomic commit. A deliberately broken pre-change/local-join resolver fails the
  `.js` substitution and accounting controls.
- **Touch points.** `Cargo.toml`, `Cargo.lock`, `crates/typokat-frontend/{Cargo.toml,src/}`,
  `crates/typokat-driver/src/driver.rs`, `src/main.rs`, WU1 fixtures, and focused tests.

### WU4 — select and pin the public witness (effort M, hard stop gate)

- **Problem.** The old sprint could not find a qualifying witness under the former library/model
  surface. The production full library and M0-M33 now justify one fresh, bounded screening pass,
  but the project must fit WU1-WU3 rather than drive witness-specific implementation.
- **Verify first.** Re-screen `morkg/jabr@9415fdad8b98dc0f1aba09c8badc5fc209bc30ba` first, then other
  genuinely small public strict-TypeScript candidates. For each, run its pinned `tsc 6.0.3
  --noEmit`, inventory configured roots, module forms, ambient names and AST surfaces, then run the
  production CLI from WU3.
- **Scope.** Select one immutable public project with at least two configured source files and one
  local named-import edge, a clean Bundler `tsc` baseline, no type-checking package dependency,
  no Node/Bun ambient dependency, and no unsupported/module-cycle/model surface. Record repository,
  commit, license, lockfile digest, config, roots, graph, tool versions, clean oracle, and three
  source-preserving mutations: assignment, call argument, and missing member.
- **Threshold / stop gate.** Zero actionable false positives, unresolved modules, skipped files or
  forms, unsupported witness forms, and missed seeded diagnostics. Stop after one working day or six
  fully evidenced candidates if none qualifies. Do not add a shim, change the fixed library, expand
  checker semantics, or relax a threshold. Preserve WU1-WU3 and archive the sprint incomplete with
  backlog `72` still open.
- **Acceptance / witness.** The descriptor reproduces from an empty cache; the pinned oracle is
  clean; every mutation has the expected `TS` identity; and no project-specific production branch
  is needed.
- **Touch points.** New `tooling/project-preview/` descriptor, README, mutation manifest, immutable
  source identity, and normalized expected contract.

### WU5 — real-project ratchet and CI gate (effort L)

- **Problem.** A successful local checkout is not a durable preview promise. It can hide root loss,
  resolver-target drift, skipped forms, diagnostic identity swaps, or mutation failures.
- **Verify first.** From an empty cache, fetch the exact witness, verify commit and lockfile digest,
  run its clean oracle, apply mutations independently, and compare two normalized typokat runs.
  Prove the runner rejects a missing root, changed target, dropped mutation diagnostic, corrupt
  cache, digest mismatch, unexpected exit, and stale baseline.
- **Scope.** Add a stdlib-minimal black-box runner around the public CLI. Ratchet exact identities for
  config, roots, checked/skipped files, resolved/unresolved/unsupported modules, project notices,
  AST incompletes, parse errors, and diagnostics. Add the same command to CI. Keep the official-suite
  ratchet separate and unchanged; allowlists must name an exact live owner and cannot use wildcards.
- **Acceptance / witness.** The clean witness meets every WU4 zero threshold; all three mutations
  report the expected `TK` identity at the mutated site; two fresh runs are byte-identical; each
  injected runner fault fails; and CI invokes the documented public command.
- **Touch points.** `tooling/project-preview/`, committed baselines/tests, `.github/workflows/ci.yml`,
  `.gitignore`, and public/tooling documentation.

### WU6 — independent adversarial review (effort L)

- **Problem.** A curated project may look clean while discovery omits a root, module filtering
  creates an error-type channel, resolution selects the wrong file, or normalization hides drift.
- **Verify first.** A reviewer independent of WU2-WU5 receives the frozen WU1 contract and exact
  uncommitted diff, reproduces the witness from an empty cache, and reruns the pinned oracle without
  relying on implementation rationale.
- **Scope.** Attack omitted/duplicate/reordered roots; malformed/unsupported config; path traversal;
  `.js`/`.ts` ambiguity; missing/bare/default/namespace/re-export/cycle forms; wrong resolution
  target; parse-plus-incomplete precedence; timestamp/path/cache nondeterminism; mutation isolation;
  wildcard/count-only allowlisting; and same-count identity swaps. Cross-check fresh resolution
  probes against `tsc 6.0.3` Bundler mode and semantic probes against `tsc --strict`.
- **Acceptance / witness.** Explicit PASS with commands, probe identities, and zero unresolved HIGH
  findings. Every FAIL returns to the owning implementation agent as one focused root-cause cluster
  and is re-reviewed by the same independent reviewer.
- **Touch points.** Read-only WU1-WU5 diff, synthetic corpus, project-preview runner, and scratch
  oracle probes; durable fixtures only for confirmed failures.

### WU7 — public claim and closure (effort M)

- **Problem.** Shipping the path without updating the completion contract would either hide the new
  capability or overstate it as general package/project support.
- **Verify first.** Run the complete local gate, the preview twice from an empty cache, its fault
  controls, and a fresh official-suite `run --check`. Audit every live claim about backlog `72`,
  module support, resolver profiles, packages, `.d.ts`, cycles, and parallelism.
- **Scope.** Document the exact CLI, summary channels, exit behavior, pinned witness, resolver
  version/options, reproducibility command, and limitations. Mark `D-real-project-preview`
  complete, delete backlog `72`, retain and rescope backlog `15` around the shipped tracer, archive
  this sprint with commit map and exact identities, and refresh README/reference/index files.
- **Acceptance / witness.** `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D
  warnings`; release build; project-preview fresh-cache and fault gates; fresh official-suite ratchet;
  docs lint; and WU6 PASS all succeed on the closure candidate. No claim extends beyond one
  dependency-free Bundler witness and the admitted synthetic surface.
- **Touch points.** `README.md`, project-preview docs, `docs/reference/{architecture,scope,
  divergences}.md` where behavior changed, `docs/backlog/{completion-1.0.toml,README.md,15-modules-imports.md}`,
  `docs/INDEX.md`, `docs/{sprints,archive}/`, and backlog `72`.

## Out of scope (explicit)

- General `node_modules`, `@types`, package `exports`/`imports`, package conditions, declaration
  layouts, and package `.d.ts` consumption. Those remain backlog `15` even though the selected
  resolver API can locate some of them.
- Default, namespace, star, and source re-export semantics; `export =`; ambient external modules;
  UMD `export as namespace`; and supported cyclic module publication. They remain backlog `15`.
- `paths`/`baseUrl`, config inheritance/references, and full `files`/`include`/`exclude` parity unless
  the exact WU1 contract admits one bounded shape. Unconsumed forms are explicit unsupported
  project notices, never silently ignored.
- NodeNext, Node16, classic Node, CommonJS-specific, and every non-Bundler profile. ADR-0007 defers
  them; this sprint must not approximate them.
- Checker/type-model fixes selected to make a candidate pass. Reject the candidate or file a
  separate spec-first backlog/sprint; do not create a witness-specific semantic branch.
- Parallel cross-file identity and incrementality — backlogs
  [`16`](../backlog/16-parallelism-type-universe.md) and
  [`17`](../backlog/17-incrementality.md).
- The consumer resolution map and Pavouk oracle — backlogs
  [`79`](../backlog/79-resolution-query-surface.md) and
  [`80`](../backlog/80-pavouk-resolution-oracle.md). They can follow the tracer without expanding
  its public checker contract.
- The planned namespace-binder refactor. It overlaps module/namespace substrate and must not run
  concurrently with this sprint. The checker-scaling sprint may continue only in disjoint files;
  one production RED/root-cause cluster remains the global limit.

## Decisions

- The old witness-first order is superseded. Synthetic differential project/resolver behavior ships
  before public witness selection; WU4 is still a hard zero-threshold gate, not a relaxed promise.
- The tracer is dependency-free and local-only. Exercising `oxc_resolver` does not authorize a
  package or declaration-consumption claim.
- `oxc_resolver` is the sole physical lookup authority. Typokat owns root enumeration, complete
  pre-filter module inventory, graph/order, semantics, accounting, and deterministic rendering.
- Discovery/accounting wrap `check_project_once`; they do not create another checker path or type
  universe. The complete-source production library route remains unchanged.
- Project discovery may land internally in WU2, but the public directory/config route remains
  unavailable until WU3 atomically ships pre-filter module accounting and resolver classification.
- Every configured root and encountered module form has one explicit identity and outcome. A form
  filtered before accounting is a soundness failure, not an unsupported convenience.
- WU1-WU3 are useful backlog-15 progress even if WU4 finds no qualifying public project. In that
  case the sprint terminates incomplete and does not close backlog `72`.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Documentation-only transition; no production behavior changes. |
| 2 | WU1 | Leader commits the complete disabled RED/spec contract separately. |
| 3 | WU2 | Implementation subagent lands unreachable discovery/config substrate; a different agent verifies it cannot escape before leader commit. |
| 4 | WU3 | Same implementation owner atomically exposes the public route with resolver/accounting; a different agent reviews before leader verification and commit. |
| 5 | WU4 | Public candidate is selected against the shipped surface; zero-threshold hard stop. |
| 6 | WU5 | Ratchet/CI lands only after the witness descriptor is immutable. |
| 7 | WU6 | Different agent performs adversarial review; every FAIL is remediated and re-reviewed. |
| 8 | WU7 | Leader runs full gates, updates the exact public claim, and archives the sprint. |

Read-only WU4 candidate screening may run beside WU1-WU3, but it must not change production scope or
open another RED cluster. WU2-WU3 production edits are serial. Each semantic finding is classified
before action; no checker-model fix joins this sprint implicitly. Every agent receives explicit file
ownership, and a 20-minute interval without an edit, test result, or concrete root-cause finding
triggers ownership transfer per `dev-method.md`.

Every implementation WU follows the full spec → implementation subagent → different adversarial
reviewer → leader verification → commit loop. WU6 is an additional cross-cutting closure review; it
does not replace the per-WU reviews.

All Cargo gates run through an appropriate `cpu-lease`. Benchmarks, if any, use `--no-smt`; an
unleased number is not evidence. The final gate is: `cargo fmt --check`; `cargo test`;
`cargo clippy --all-targets -- -D warnings`; release build; project-preview clean/mutation/fault
ratchets twice from a fresh cache; fresh official-suite `run --check`; docs lint; WU6 PASS.

## Run log

- 2026-08-07 — WU1 PASS. The behavior-neutral spec commit pins 25 synthetic projects, 22 exact
  config-boundary cases, every OXC module-declaration form outside the admitted named-import slice,
  four-way exit precedence, normalized line/column identities, and directory/config equivalence.
  Pinned `tsc 6.0.3` config and directory invocations were byte-identical for every project and
  config case. The two active preservation guards passed; forcing the five disabled WU3 contract
  groups failed on pre-change project/accounting behavior. Independent adversarial review passed
  after three remediation rounds; no production source changed.
- 2026-08-07 — Plan grounded at clean `7e3c221` after the full default-library closure. Two
  independent read-only audits agreed that the old witness-first sprint should be archived, the
  first slice should remain local/named/dependency-free, and complete pre-filter module accounting
  is the load-bearing false-clean guard. No production or test implementation exists yet.
