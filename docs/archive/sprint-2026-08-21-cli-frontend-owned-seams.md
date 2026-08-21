> **OUTCOME — shipped 2026-08-21.** The CLI now has one private writer-level report transaction
> that renders project notices, parse errors, diagnostics, and incomplete records in order and
> owns the single `incomplete > diagnostics > clean` status decision. Official-batch, project,
> and path wrappers retain their distinct output cap, buffering, conversion, write, flush, and
> error policies. The frontend now uses one private, consuming `ModuleAccounting` owner: one
> reachable statement dispatch records owned syntax evidence and resolution tasks, resolution
> replay preserves the previous default-first/ordinary ordering, and finalization consumes complete
> owned state into the existing plan without retaining AST or checker borrows. Backlogs 112 and 118
> are closed.
>
> Commit map: sprint plan → `b0c45ca`; initial contracts → `0b04386`; adversarially strengthened
> ownership contracts → `0dc0121`; CLI transaction → `6bba6bb`; frontend accounting owner →
> `f2671aa`.
>
> Final verification: both independent WU reviews PASS with no unresolved HIGH/MEDIUM finding;
> CLI ownership 8/8, module-accounting structure 4/4, scan receipts 7/7, frontend 32/32,
> incomplete 7/7, official batch 4/4, parse routing 7/7, B72 project routes 6/6 and 12/12,
> source re-exports 7/7, and default slots 8/8 with one pinned-oracle test ignored. Ordinary
> conformance printed `580/696/580/0`; full workspace tests, all-target Clippy with `-D warnings`,
> and formatting passed. A fresh release official-suite run covered 874 tests / 379 in-scope with
> zero regressions, zero progress, and complete scoreboard coverage. Full shared/cold parity and
> randomized differential were not run because neither WU changed checker semantics, module
> breadth, or contextual inference.

# Sprint — CLI and frontend owned seams (2026-08-21)

**Goal.** Close backlogs 112 and 118 as one behavior-preserving cleanup sprint: centralize the CLI
report transaction and turn project module accounting into one owned scan/finalization object.

**Theme.** Both targets are accepted safe seams from the development-friction audit. Each replaces
implicit local coordination with one private owner while preserving the current public routes,
semantic authorities, output bytes, and ordering. Their write territories are disjoint, so the two
implementations can run in parallel after independently authored acceptance contracts land.

## Refs re-verified at HEAD (2026-08-21, `26061c1`)

`✔` = confirmed live · `⚠` = constraint or nuance caught before implementation.

- ✔ `src/main.rs` still has three report loops at `render_official_batch_reports`,
  `render_project_reports`, and `check_paths_with_io` (`:304-346`, `:649-700`, `:811-889`). Each
  renders project notices, parse errors, diagnostics, and incompletes, then applies
  `incomplete > diagnostics > clean`.
- ✔ The static baseline remains three calls each to
  `diagnostics::render_to_writer_with_format` and
  `diagnostics::render_incomplete_to_writer_with_format`, with nine occurrences each of
  `had_errors` and `had_incomplete` in `src/main.rs`.
- ⚠ The loops share rendering semantics but not outer I/O policy. Official batch writes rich output
  through `BoundedOutput` and converts it to a string; project and path routes buffer bytes before
  one stderr write and flush. The shared seam must not own those wrapper policies.
- ✔ `account_project_modules` spans 576 lines at
  `crates/typokat-frontend/src/frontend.rs:1734-2309`. Its mutable state is owned collections plus
  an owned resolver; the function does not retain a checker `Pass` or a self-borrowing phase.
- ✔ `AccountedModulePlan` already provides one owned final product at
  `crates/typokat-frontend/src/frontend.rs:1519-1526`. Production and test consumers call the same
  accounting function for frozen, admitted source-reexport, and default-candidate modes.
- ⚠ Finalization still needs borrowed inputs/programs for stable paths and namespace provenance.
  The accumulator may accept these borrows per scan/finalization call, but must not retain AST,
  `FileInput`, checker, or resolver-context borrows in its stored state.
- ✔ Existing route witnesses cover exact CLI status/output, official-batch isolation, parse
  precedence, Bundler summaries/edge refusals, acyclic source re-exports, and default-module slots.
  Frontend unit tests already carry mutation controls for default-module authority and source
  re-export accounting.
- ✔ The active library-compiler decomposition sprint explicitly excludes both `src/main.rs` and
  `frontend.rs`; the active `as const` sprint owns expression/class-initializer semantics instead.
  This sprint therefore has no planned production-file overlap with either active sprint.

## Work units

### WU0 — freeze the two cleanup contracts (effort S)

- **Problem.** Existing behavior tests protect many output examples, but they do not directly prove
  that one CLI transaction owns all four channels or that module accounting scans each program and
  statement exactly once. A move-only refactor could therefore pass examples while retaining the
  duplicate authority.
- **Verify first.** Assign the CLI and frontend contracts to spec writers who will not implement
  the corresponding WU. Freeze the current green behavior matrix first. Then add deterministic RED
  contracts for one report-channel loop/status decision and for one program/statement scan receipt
  across frozen, source-reexport, and default-candidate accounting modes. Include negative controls
  that fail when one channel is omitted, status precedence changes, a statement is revisited, or a
  proof product is finalized from partial coverage.
- **Scope.** Tests and test-only instrumentation only. Do not introduce the production helper or
  accumulator, change route output, widen APIs, or move production code in this WU. Keep the two
  acceptance territories independent so each implementation receives a frozen contract.
- **Acceptance / witness.** All pre-existing behavior gates remain green. Each new acceptance
  contract is RED only because its production owner is absent, with a clear failure signature and
  no checker-semantic fixture changes. Commit the contract separately before WU1/WU2 start.
- **Touch points.** `src/main.rs` test module or a focused CLI contract test;
  `crates/typokat-frontend/src/frontend.rs` test modules and narrowly scoped test instrumentation.

### WU1 — centralize the CLI report transaction (effort S)

- **Problem.** Three production branches repeat about 145 protocol lines and independently decide
  final status. A future channel or status change can drift by route even though all three consume
  the same `FileReport` model.
- **Verify first.** Run WU0's CLI contract plus `incomplete_outcome`, `wu7_official_batch`, both B72
  project tracer suites, and `wu7_parse_routing` before editing. Freeze their stdout, stderr, exit,
  bounded-output, and flush behavior.
- **Scope.** Introduce one private writer-level transaction that renders, in order, project notices,
  parse errors, diagnostics, and incomplete records, then returns one `CheckStatus`. Keep
  `BoundedOutput`, rich-format selection, string conversion, buffer allocation, final stderr
  write/flush, path reading, project dispatch, and route-specific error wrapping in their current
  wrappers. Reuse the existing diagnostics renderers; do not add a report abstraction or public API.
- **Acceptance / witness.** The WU0 contract passes. Static counts become one call each to the two
  diagnostics renderers, three occurrences each of `had_errors`/`had_incomplete`, one channel loop,
  and one status-precedence decision. All frozen output/error witnesses are byte- and exit-identical,
  including mixed diagnostics plus incomplete surfaces and an overflowing official-batch buffer.
- **Touch points.** `src/main.rs`; focused CLI tests only where WU0 proves missing coverage.

### WU2 — extract one owned module-accounting accumulator (effort M)

- **Problem.** One 576-line function interleaves path/resolver setup, per-program/per-statement
  scanning, source/default candidate evidence, graph construction, cycle handling, notices, and
  final certification. The one-pass invariant and product completeness live only in local control
  flow.
- **Verify first.** Run WU0's scan receipt, the full `typokat-frontend` package suite, acyclic
  source-reexport/default-module CLI tests, and both B72 project tracer suites. Capture exact
  project-summary JSON and resolution/notice ordering for admitted and rejected controls.
- **Scope.** Add one private owned `ModuleAccounting`-style accumulator. It owns the mutable
  collections and resolver/path context derived by the current facade, accepts borrowed inputs and
  programs only for a scan/finalization call, scans each program and statement once, and consumes
  itself into the existing `AccountedModulePlan`. Keep source re-exports, default candidates,
  physical resolution, cycles, namespace provenance, and notice certification within that one
  authority. Leave small pure helper algorithms in place unless moving them is required for private
  ownership; this is not a general `frontend.rs` file split.
- **Acceptance / witness.** The facade becomes a short linear setup → scan → finalize sequence; the
  accumulator stores no AST/checker borrow; WU0 proves one scan per program/statement and complete
  finalization. All modes retain identical dependency order, resolution identities, notices,
  missing-module locations, source/default products, route receipts, and project-summary bytes.
  No supported or rejected module form changes.
- **Touch points.** `crates/typokat-frontend/src/frontend.rs`; its internal accounting tests and
  existing project-route witnesses. No crate-boundary or public API change.

### WU3 — independent review and integrated closure (effort M)

- **Problem.** Mechanical deduplication can accidentally merge route-specific I/O semantics, while
  an accumulator extraction can hide a second AST walk, retain invalid borrows, or reorder proof
  products despite green happy-path examples.
- **Verify first.** Freeze WU1 and WU2 diffs separately. Give each to a reviewer who neither wrote
  its acceptance contract nor implemented it.
- **Scope.** Review WU1 for exact channel/error ordering, bounded-output failure, buffering/flush
  atomicity, rich/selected formats, and status/exit precedence. Review WU2 for one-pass evidence,
  complete mode coverage, physical-resolution call count, deterministic ordering, partial-product
  rejection, retained borrows, and accidental second authorities. Route prescribed fixes once to
  the original implementer, then repeat the affected review.
- **Acceptance / witness.** Both independent verdicts are PASS with no unresolved HIGH/MEDIUM
  finding. Run:
  1. `cargo fmt --check`;
  2. `cargo test --test incomplete_outcome`;
  3. `cargo test --test wu7_official_batch`;
  4. `cargo test --test wu7_parse_routing`;
  5. `cargo test --test b72_bundler_project_tracer_cli`;
  6. `cargo test --test b72_bundler_project_tracer_edges`;
  7. `cargo test --test b15_acyclic_source_reexports_cli`;
  8. `cargo test --test b15_default_module_slots_cli`;
  9. `cargo test -p typokat-frontend`;
  10. `cargo test --test conformance conformance -- --exact --nocapture`, expecting
      `580/696/580/0`;
  11. full `cargo test`;
  12. `cargo clippy --all-targets -- -D warnings`;
  13. a freshly built release binary plus the full pinned official-suite `run --check` ratchet;
  14. manifest and diff checks after backlog/sprint closure.
  Pass the explicit local gate list to `tooling/verify/verify.py` under one two-vCPU lease. Its
  resolved receipt may prove gate 9 subsumed by gate 11, but must retain the targeted commands and
  visible conformance receipt. Full shared/cold parity and randomized differential are not required
  unless implementation crosses the declared non-semantic boundary.
- **Touch points.** Read-only review of WU1/WU2, then normal sprint/backlog/archive index closure.

## Out of scope (explicit)

- No diagnostic wording, format, channel, buffering, output cap, status, or exit-code change.
- No new module form, resolver breadth, dependency-order rule, source/default re-export behavior,
  namespace provenance, or project-summary schema.
- No `PreparedProject` or provider single-parse work from backlog 115. `ModuleAccounting` owns one
  accounting pass over already parsed programs; it does not own parser/worker lifetimes.
- No class-publication extraction from backlog 119 and no checker expression/inference work.
- No sparse-collision lifecycle measurement/deletion (113), lazy-library spike (114), session
  boundary (116), or default-library compiler decomposition.
- No generic phase framework, retained self-borrowing context, public accumulator, crate move,
  broad `frontend.rs` split, test-framework construction, or source-text-oracle cleanup.

## Decisions

- Backlogs 112 and 118 form one sprint because both replace duplicated/implicit coordination with a
  private owner, have exact non-semantic acceptance, and write disjoint production files.
- The CLI transaction owns report rendering and status only. Route wrappers retain their different
  allocation, output-cap, format, conversion, write, and flush policy.
- Module accounting remains one frontend authority and one AST scan. The new object owns mutable
  scan/finalization state but never owns or retains parsed AST or checker borrows.
- Structure is accepted only when it reduces local control state. Reject callback-heavy helpers,
  effect replay enums, parallel accounting products, or visibility widening even if tests pass.

## Sequencing

1. Commit this sprint contract and freeze the two production territories.
2. Independent spec writers author WU0's CLI and frontend contracts; commit them together before
   implementation.
3. Implement WU1 and WU2 in parallel because `src/main.rs` and `frontend.rs` are disjoint.
4. Cross-review the frozen diffs, apply at most one prescribed fix round, and rerun affected gates.
5. Commit each reviewed WU separately, serialize closure gates, delete backlogs 112/118, stamp the
   outcome, archive the sprint, and refresh roadmap/index counts.

## Run log

- 2026-08-21: sprint prepared from current-HEAD backlogs 112 and 118. No implementation or
  acceptance-test mutation started.
- 2026-08-21: baseline route gates passed before implementation. WU0 initially proved the missing
  CLI owner and scan receipts, but adversarial review found that both structural contracts could be
  satisfied without the intended authority. The contracts were strengthened with lexical
  sanitization, reachable-call analysis, omission/reordering/revisit/partial-coverage controls, and
  exact/overflow output-cap controls before either production WU landed.
- 2026-08-21: WU1 centralized all four report channels and status precedence. Its final independent
  review passed with no HIGH/MEDIUM finding; route-specific buffering, output cap, write, flush,
  and error wrapping stayed in the wrappers.
- 2026-08-21: WU2's first review found hidden helper AST walks and a reusable/partial scan surface.
  The implementation moved namespace/default evidence into the owned dispatch and made scan
  consuming. Final review then caught a physical-resolution ordering drift; owned tasks were
  replayed default-first and then ordinary in their original source order. Re-review passed with no
  unresolved HIGH/MEDIUM finding.
- 2026-08-21: the same-tree verifier passed the full closure matrix at `f2671aa`. Ordinary
  conformance printed `580/696/580/0`; full workspace tests and all-target Clippy passed; the fresh
  release official-suite ratchet reported 874 corpus / 379 in-scope, zero regressions, zero
  progress, and complete corpus/scoreboard coverage.
