<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU0 → <sha>,
> WU1 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — executable completeness accounting (2026-07-10)

**Goal.** Make it impossible for typokat to report a trustworthy clean result after silently
skipping an in-scope AST surface, and make every deferred divergence machine-owned.

**Theme.** Backlog `73` (unsupported-surface accounting) and the inventory/validator portion of
`75` (deferred-divergence disposition) are the trust boundary that must precede the ambient
prelude and real-project preview. They belong together because the CLI can only distinguish
"clean" from "incomplete" when the checker and the roadmap share stable identities, owners, and
witnesses. This sprint builds that accounting system; it does not implement the semantic feature
tail that the census discovers.

## Refs re-verified at HEAD (2026-07-10)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ⚠ Planning baseline: HEAD is `062f1ad`, while the post-audit roadmap patch (`72`–`75`, scope
  families, and the strengthened completion manifest) is still uncommitted in the working tree.
  Land that patch atomically before the WU0 spec commit so spec-first history stays honest.
- ✔ The checker pipeline has distinct bind, type-fill, flow-build, statement-check, and relation
  phases; coverage must therefore be classified per **role and child slot**, not only per OXC node
  variant — `src/check/checker/mod.rs:60`, `src/check/checker/mod.rs:159`.
- ✔ `CheckOutput` contains only `diagnostics` and `parse_errors`; an empty pair is currently clean
  — `src/driver.rs:22`.
- ✔ The public CLI currently has only exit `0` clean, `1` diagnostics, and `2` usage, and returns a
  bare `bool` from `run` — `src/main.rs:12`, `src/main.rs:18`, `src/main.rs:35`.
- ✔ Expression inference explicitly says unsupported shapes are unchecked and ends in `_ =>
  None`; object/array literals also skip child slots such as spreads/computed keys —
  `src/check/checker/expr.rs:22`, `src/check/checker/expr.rs:162`,
  `src/check/checker/expr.rs:259`, `src/check/checker/expr.rs:282`.
- ✔ Statement checking silently drops other statement/declaration forms —
  `src/check/checker/statements.rs:62`, `src/check/checker/statements.rs:139`,
  `src/check/checker/statements.rs:149`.
- ✔ Binding has independent wildcards for type predeclaration, statements, declarations, class
  elements, and expressions; `bind_expression` is narrower than inference/flow —
  `src/binder/bind.rs:228`, `src/binder/bind.rs:283`, `src/binder/bind.rs:368`,
  `src/binder/bind.rs:511`, `src/binder/bind.rs:597`.
- ⚠ The flow pre-pass still claims `for`/`for-of`/`do-while` are not checked, but WU1 of the prior
  sprint now checks them structurally; the wildcard and comment drift independently from the
  checker — `src/check/checker/flowgraph/mod.rs:104`.
- ✔ Annotation lowering covers a subset of `TSType` and returns `None` for every remaining variant;
  unsupported operators also return `None` — `src/check/checker/annotations/mod.rs:63`,
  `src/check/checker/annotations/mod.rs:128`, `src/check/checker/annotations/mod.rs:174`.
- ✔ OXC is pinned to `0.137.0`; an upgrade must visibly invalidate the surface inventory —
  `Cargo.toml:23`.
- ✔ The existing disabled-corpus registry is the spec-first extension point, while the hand-rolled
  completion-manifest validator already demonstrates aggregated schema/path errors and exact
  scope-family ownership — `tests/conformance.rs:27`, `tests/manifest.rs:345`,
  `tests/manifest.rs:559`.
- ✔ The official-suite harness accepts only exit `0`/`1` and treats every other code as a hard
  failure; it must learn the incomplete outcome without weakening crash detection —
  `tooling/official-suite/tsofficial.py:177`, `tooling/official-suite/tsofficial.py:233`.
- ✔ Five minimal probes currently parse and exit `0` silently while `tsc 6.0.3 --strict` rejects:
  a bad call inside a template interpolation, computed object key, and array spread; `typeof
  Missing` in a type annotation; and bad assignments inside `try`/`catch`/`finally`.

## Work units

### WU0 — spec-only surface corpus, schemas, and split gate (effort L)

- **Problem.** The sprint changes a trust contract, but there is no acceptance corpus for an
  incomplete check, no stable surface identity, and no bounded census proving the proposed WUs fit
  without a traversal rewrite.
- **Verify first.** Re-run the five pinned probes with both typokat and `tsc 6.0.3 --strict`; count
  the relevant OXC `0.137.0` enum variants and every dispatcher/child-slot role at HEAD; trace how
  `diagnostics`, binder outputs, and flow metadata reach `CheckOutput`. Confirm the current
  official-suite behavior for unexpected exits and same-count identity swaps.
- **Scope.** Add a disabled `b73_surface_accounting` corpus and code-adjacent schema fixtures for:
  expression child slots (template interpolation, computed key, spread), statement containers
  (`try`/`catch`/`finally`), annotation lowering (`typeof Missing`), controls that are already
  supported, duplicate/missing surface records, malformed divergence metadata, and dependency
  drift. Define stable product identities by `role/surface/slot-or-variant`, independent of OXC's
  display names. Record the exact CLI outcome snapshots but do not change behavior. Update
  `tests/cases/README.md` with the incomplete-marker convention. Commit this WU alone with all new
  behavior directories disabled.
- **Split gate.** If the census finds more than eight newly ownerless semantic families, keep them
  classified/owned but do not implement them. If honest accounting requires a second traversal
  architecture or a new cross-layer ownership boundary, stop after the executable inventory and
  outcome foundation (WU1/WU2/WU6), file the behavioral WU3–WU5 follow-up, and resolve the design
  explicitly. Difficulty or volume alone does not trigger the split.
- **Acceptance / witness.** The spec commit is behavior-neutral; enabling each fixture at old HEAD
  proves the intended silent path or schema failure. Every probe has a tsc verdict and a nearby
  control. The recorded census names each dispatcher role and relevant child position rather than
  claiming node-variant coverage alone.
- **Touch points.** `tests/cases/b73_surface_accounting/`, `tests/cases/README.md`,
  `tests/conformance.rs`, new `tests/surface/` and divergence-schema fixture data, scratch tsc
  probes, and the cited source paths read-only.

### WU1 — exhaustive OXC surface inventory and validator (effort L)

- **Problem.** A prose list of unsupported syntax cannot detect a newly added OXC variant, a
  supported wrapper whose child slot is skipped, or different coverage across binder/flow/checker.
- **Verify first.** From WU0's census, confirm the smallest set of OXC enums and roles that covers
  binder, expression/statement checking, flow, annotation lowering, declarations/signatures, and
  class elements. Prove an exhaustive Rust match fails to compile when a synthetic enum variant is
  unhandled; do not depend on Cargo-registry source paths in CI.
- **Scope.** Add a code-adjacent `tests/surface/` manifest with pinned OXC version and records for
  stable id, role, variant/child slot, disposition (`supported`, `unsupported-in`, `design-oos`),
  owner, and witness. Add exhaustive compile-time classifiers plus a hand-rolled validator in the
  style of `tests/manifest.rs`. It must reject version drift, missing/duplicate/unknown identities,
  missing owners/witnesses, `unsupported-in` without a live backlog owner, and a supported wrapper
  whose required child slot has no coverage record. This WU inventories behavior only; it does not
  silently promote a feature to supported.
- **Acceptance / witness.** Mutation tests fail for an omitted enum arm, missing child slot,
  duplicate identity, bad disposition, dead owner link, or OXC-version mismatch. The committed
  inventory validates with every current role classified.
- **Touch points.** New `src/surface.rs` or an equivalently narrow shared pipeline module selected
  after verify-first, `tests/surface/`, `tests/surface.rs`, `Cargo.toml`/`Cargo.lock` as read-only
  version witnesses.

### WU2 — first-class incomplete outcome and CLI/harness contract (effort L)

- **Problem.** `CheckOutput` cannot represent incomplete work, and a warning with exit `0` would
  preserve the false-clean failure mode. The official-suite harness currently treats any third
  exit as a crash.
- **Verify first.** Confirm no external code in the repository pattern-matches `CheckOutput` or the
  CLI's `bool` result; replay harness process-failure tests so exit `3` cannot become a generic
  success path. Confirm rich and compact renderers can share the stable identity/span data without
  inventing a `TK` code.
- **Scope.** Add a third structured channel (`IncompleteSurface`) alongside diagnostics and parse
  errors, carried through single-file and project reports. Render it separately as
  `incomplete[<stable-id>]` with file/span and owner-facing context. CLI contract: `0` = complete
  clean, `1` = complete with type/parse diagnostics, `2` = usage/IO, `3` = incomplete (takes
  precedence even when ordinary diagnostics also exist; all output is still rendered). No new TK
  diagnostic and no `--allow-incomplete` escape hatch. Update official-suite parsing so exit `3`
  becomes an identity-bearing `OOS:unsupported` discovery result; an IN→unsupported move remains a
  regression, and crash/signal/unparseable-output handling remains strict. The exit-`3` scoreboard
  record keeps carrying the full diagnostic diff (matched/fn/fp identities) alongside the
  incomplete identities — demotion must not blind the harness to diagnostic regressions inside
  now-unsupported tests, and `diag-recall`/`matched %` must stay comparable across the boundary.
- **Acceptance / witness.** Unit/black-box tests cover all four exits, incomplete+diagnostic
  coexistence, deterministic rich/compact rendering, project aggregation, duplicate suppression,
  and official-suite identity round trips (an exit-`3` record round-trips both its incomplete
  identities and its diagnostic diff). Exit `0` with any diagnostic or incomplete record is a
  hard inconsistency.
- **Touch points.** `src/driver.rs`, the shared checker result in `src/check/checker/mod.rs`,
  `src/main.rs`, `src/diagnostics/` or a sibling incomplete renderer,
  `tooling/official-suite/tsofficial.py`, its unit tests and scoreboard schema/docs.

### WU3 — expression, binder, and flow child-slot accounting (effort L)

- **Problem.** Expression coverage differs across binder, flow, and inference; templates,
  computed keys, spreads, call/new arguments, and wrappers can lose nested type errors before the
  value-level checker sees them.
- **Verify first.** Run WU0 expression probes and compare `bind_expression`, flow-expression walk,
  `infer_expr`, object/array inference, and call argument collection role by role. Separate a
  wrapper's own OOS semantics from the obligation to account for its in-scope children.
- **Scope.** Wire WU1 dispositions through existing binder/flow/checker paths. A `supported` record
  must visit every declared child slot; `unsupported-in` must record incomplete before any
  error-type/`None` degradation; `design-oos` may ignore its own semantics but cannot hide an
  in-scope child. Reuse existing walkers and the single CFG—do not add a generic second checker.
  Known binary/template/spread/iteration semantics remain backlog `71`: this WU may traverse or
  mark incomplete, not claim those features complete. Newly found semantic gaps graduate to
  backlog owners before the implementation commit.
- **Acceptance / witness.** The template/computed-key/spread probes can no longer exit clean; each
  produces the nested TK diagnostic or the exact incomplete identity prescribed by WU0. Reordered
  and nested controls are stable, and supported expression families retain existing verdicts.
- **Touch points.** `src/binder/bind.rs`, `src/check/checker/expr.rs`,
  `src/check/checker/flowgraph/exprs.rs`, `src/check/checker/calls.rs`, expression-related surface
  records and `b73_surface_accounting` fixtures.

### WU4 — statement and declaration accounting (effort L)

- **Problem.** Binder, flow, and checker statement/declaration wildcards disagree, and stale flow
  coverage already proves that one layer's "OOS" comment can survive after another layer starts
  checking the construct.
- **Verify first.** Trace every Statement/Declaration/module-declaration role in binder, flow,
  checker, export handling, and function/class body entry. Run the `try`/`catch`/`finally` witness
  and controls for the recently added loop/throw traversal.
- **Scope.** Apply the same supported/unsupported-in/design-OOS discipline to statements,
  declarations, module wrappers, and their child blocks/expressions. Remove stale flow claims and
  make unsupported wrapper semantics distinct from child traversal. Do not implement declaration
  hoisting (`74`), return analysis (`46`), module breadth (`15`), or new control-flow semantics.
- **Acceptance / witness.** No WU0 statement/declaration probe exits clean; all supported loop,
  throw, switch, export, overload, and block-scope fixtures remain green. Inventory validation
  proves binder/flow/checker roles agree or carry an explicit, owned difference.
- **Touch points.** `src/binder/bind.rs`, `src/check/checker/statements.rs`,
  `src/check/checker/flowgraph/mod.rs`, declaration/export helpers, surface records and fixtures.

### WU5 — annotations, signatures, and class-member accounting (effort L)

- **Problem.** Annotation lowering returns `None` for unmodeled `TSType` variants, while object
  signatures and class collection can skip unsupported/computed members. Those fallbacks suppress
  cascades without proving that an error or incomplete record exists.
- **Verify first.** Inventory OXC `TSType`, `TSSignature`, class-element, property-key, and type-
  query child roles. Run `let x: typeof Missing = 1` plus controls for supported `keyof`, mapped,
  conditional, template, overload, readonly, and recursive annotations.
- **Scope.** Record incomplete before every unsupported annotation/signature/member degradation,
  and enforce child-slot accounting for wrappers classified supported. Preserve the error type for
  cascade suppression only after a diagnostic or incomplete record. Do not implement the model
  gaps in `41`–`44`, `49`, `52`, `70`, `71`, or `75`; link them from the inventory.
- **Acceptance / witness.** The type-query witness and adversarial computed/member shapes cannot
  exit clean; supported M24–M33 and recursion corpora remain verdict/order stable. An independent
  reorder/repeat probe finds no query-order-dependent loss.
- **Touch points.** `src/check/checker/annotations/`, `src/check/checker/decls/`,
  `src/check/checker/classes/`, type/signature/member surface records and fixtures.

### WU6 — structured divergence census and dependency parity (effort L)

- **Problem.** Scope-family ownership is machine-validated in the pending roadmap patch, but
  `divergences.md` remains prose-only and manifest `deps` are checked only for path existence. Track
  C's "no known silent FN" claim is therefore not executable.
- **Verify first.** Census every top-level and nested deferred/skipped/error-type/OOS statement in
  `divergences.md`; reconcile each with current manifest criteria and backlog owners. Compare each
  incomplete criterion's `deps` with its owner's `blocked-by` frontmatter and identify legitimate
  slice exceptions before defining one.
- **Scope.** Keep `divergences.md` as the single human and machine source: add compact inline
  metadata for stable id, direction (`under`, `over`, `cosmetic`), scope disposition, owner, and
  witness. Add a dedicated validator patterned after `tests/manifest.rs`; it rejects unmarked
  divergence rows, duplicates, bad enums, dead/missing owners or witnesses, and every unclassified
  under-report. Extend manifest validation to compare `deps` with `blocked-by`; a slice exception
  needs an explicit schema field and rationale, never an implicit mismatch. Migrate the complete
  current ledger, including raw-arity/generic-base overrides, `TS2675`, intrinsic/error degradation,
  type-parameter defaults, optional tuples, generic `T[K]`, and dropped call arguments. Do not fix
  the semantic divergences.
- **Acceptance / witness.** Table-driven mutation tests reject each malformed/missing field,
  ownerless under-report, unmarked row, and the historical `14`/`70` dependency drift. Every live
  divergence validates and links to a shipped witness, live backlog owner, or explicit design-OOS
  scope family. Mark manifest criterion `C-deferred-divergence-census` complete on ship; backlog
  `75` remains open for the actual semantic surface tail.
- **Touch points.** `docs/reference/divergences.md`, `docs/reference/scope.md`,
  `docs/backlog/completion-1.0.toml`, backlog frontmatter, `tests/manifest.rs` and/or a focused
  `tests/divergences.rs`.

### WU7 — independent adversarial reviews (effort L total)

- **Problem.** A green authored inventory can still label a skipped child "supported", let an
  incomplete result fall back to exit `0`, or hide an under-report behind broad family metadata.
- **Verify first.** After each implementation WU, a reviewer independent from its implementer reads
  the WU0 spec commit and the focused uncommitted diff, then creates fresh nested probes before the
  leader commits.
- **Scope.** WU7-A reviews WU1's exhaustiveness/schema; WU7-B reviews WU2's result/CLI/harness
  contract; WU7-C/D/E review WU3/WU4/WU5 role and child-slot coverage; WU7-F reviews WU6's full
  divergence census and dependency parity. Every review hunts false-clean paths first, tests OXC
  upgrade/identity drift, and cross-checks disputed TS verdicts against `tsc 6.0.3 --strict`.
- **Acceptance / witness.** Each checkpoint returns PASS with exact probes/commands. A FAIL adds a
  spec-only regression witness before the fix and receives a subsequent PASS. Every discovery is
  resolved in-scope or graduated to a live backlog owner; no prose-only "known" gap remains.
- **Touch points.** WU0–WU6 diffs, scratch probes, focused fixtures/validators, backlog and
  divergence metadata.

### WU8 — full verification and sprint closure (effort M)

- **Problem.** The sprint is incomplete until inventories, behavior, CLI, official-suite, docs,
  and CI agree on what "complete" means.
- **Verify first.** Audit the commit map and every WU7 PASS; confirm all WU0 fixtures are enabled or
  deliberately remain a linked future-owner witness, and diff official-suite identities before
  changing its scoreboard.
- **Expected fallout.** The first honest run demotes a large share of the 497 in-scope tests —
  especially the 214 expected-clean ones (the exact false-trust candidates) — to `OOS:unsupported`
  via exit `3`. This is the sprint working, not a regression wave. Audit the movement **aggregated
  by incomplete identity** (one identity explains many tests), never test by test; each aggregate
  group needs a disposition (correct demotion vs. accounting bug) before the re-baseline `--save`.
- **Scope.** Run the complete quality gate, audit any official-suite status/identity movement, and
  update public limitations plus tooling docs. Close/delete backlog `73` only if every covered role
  is executable end to end. Keep/rescope `75` to its remaining semantic families after marking its
  census infrastructure complete. Stamp OUTCOME, archive this sprint, and clear active indexes.
- **Acceptance / witness.** `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D
  warnings`, `cargo build --release`, official-suite unit tests, and the freshly fetched full
  `run --check` all pass. No unexplained IN→unsupported move, dropped matched identity, new false
  positive, missing surface record, ownerless under-report, or dependency drift remains.
  The closure records `74` → `38` → `72` as the next executable chain; it does not describe this
  accounting sprint itself as real-project-preview or MVP ready.
- **Touch points.** All WU outputs, `README.md`, relevant reference/tooling docs,
  `docs/sprints/README.md`, `docs/INDEX.md`, and `docs/archive/`.

## Out of scope (explicit)

- Implementing semantic gaps owned by `71`, `74`, `18`/`19`/`45`–`52`, Track A (`41`–`44`,
  `70`), or the Tier S/A/B tail in `75`.
- The minimal ambient prelude (`38`) and real-project preview (`72`); this sprint makes their clean
  verdict trustworthy but does not start them.
- Full `lib.d.ts`, module resolver breadth, package/tsconfig discovery, parallel cross-file
  identity, incrementality, or bytecode VM work (`14`–`17`, `13`).
- A generic second AST traversal/checker architecture. Existing binder, flow graph, checker, and
  annotation paths remain the semantic owners.
- New TK codes, an `--allow-incomplete` override, or treating incomplete work as an exit-0 warning.
- Relation-engine, type-store, inference-policy, or narrowing-semantic refactors. Traversal may
  enter the existing single CFG, but cannot create another flow model.
- Deploying, publishing, or changing the TypeScript/OXC pins except through an explicit follow-up.

## Decisions

**Sprint choice (Tier 1, two-way door).** Axes: soundness/risk containment (highest), downstream
unblock, time to visible demo, and scope reversibility.

- **Accounting first — chosen.** Best when a future preview must have a trustworthy clean verdict;
  it wins soundness and makes every later feature measurable.
- **Prelude `38` / preview `72` first.** Best when a near-term demo matters more than verdict
  completeness; rejected here because current unsupported paths still exit clean.
- **Track A `41` first.** Best when shortest calendar time to full `lib.d.ts` dominates; rejected
  because it advances breadth while the checker cannot prove what it skipped.

The choice is wrong if WU0 proves exhaustive accounting needs a new traversal architecture rather
than a narrow registry/collector layer; the split gate then ships inventories/outcome/owners and
defers WU3–WU5 instead of forcing the architecture.

Resolved implementation-contract decisions:

- Incomplete checking is a third structured outcome, not a TK diagnostic or warning.
- Exit `3` means incomplete and takes precedence over exit `1`; ordinary diagnostics still render.
- Surface identities include dispatcher role and relevant child slot, not only an OXC variant.
- OXC drift is caught by exhaustive Rust matches plus the pinned checked-in manifest; CI never
  reads Cargo-registry source files.
- A design-OOS wrapper does not make in-scope children OOS. It must traverse them through existing
  paths or produce an incomplete identity.
- `divergences.md` remains canonical; inline structured metadata is validated rather than copied to
  a second status file.
- Error-type cascade suppression remains, but every unsupported degradation records incomplete
  first. Relation/type-store/flow invariants remain unchanged.

## Sequencing

1. Leader lands the pending post-audit backlog/reference/manifest patch separately.
2. WU0 is one behavior-neutral spec commit.
3. WU1 → WU7-A → leader gate/commit.
4. WU2 → WU7-B → leader gate/commit.
5. WU3, WU4, and WU5 execute sequentially because they share surface records and checker state;
   each is independently reviewed and committed before the next.
6. WU6 may be prepared after WU0 in parallel with WU1–WU5 because it is docs/validator-heavy, but
   it receives WU7-F and its own atomic commit before closure.
7. WU8 is leader-owned and strictly last.

Every implementation work unit is delegated to a bounded subagent, remains uncommitted until its
independent review passes, and follows the explicit-path atomic commit convention. The WU0 split
gate is recorded in the run log before WU1 begins.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint when archived). -->

- **2026-07-10 — Plan registered.** Recommended accounting-first over `38`/`72` and Track A;
  implementation has not started. Prerequisite: commit the pending post-audit roadmap patch before
  WU0 so the spec commit remains isolated.
- **2026-07-10 — Prerequisite satisfied.** The post-audit roadmap patch landed as `4d91921`,
  before the plan commit `c38ed51`; the working tree is clean. WU0 is unblocked.
- **2026-07-10 — Plan amended before WU0 (review).** Two gaps closed: (a) WU2 — the exit-`3`
  official-suite record must keep the full diagnostic diff alongside incomplete identities, so the
  demotion does not hide diagnostic regressions inside now-unsupported tests; (b) WU8 — an
  explicit **expected fallout** note: the first honest run will mass-demote in-scope tests
  (baseline: 497 in-scope, 214 expected-clean), and the audit runs aggregated by incomplete
  identity, not per test, to keep WU8 at effort M.
