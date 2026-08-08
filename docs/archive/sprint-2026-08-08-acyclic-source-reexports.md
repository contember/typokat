> **OUTCOME — shipped 2026-08-08.** The production Bundler route now admits acyclic local named
> source re-exports, including aliases, outer and inline type-only forms, chains, and existing
> value/type class slots. The frontend alone constructs the opaque admission product after
> resolution, namespace-provenance classification, and combined dependency ordering; the checker
> validates that evidence and projects target slots without creating barrel-local bindings.
> Explicit-file and legacy routes retain their frozen results. Missing modules and members remain
> `TK2307`/`TK2305`; empty lists are erased; namespace-bearing targets and cycles remain explicit
> non-clean outcomes. Commit map: plan/oracle → `c46ce6a`, `f29d687`; WU1 → `9125dd9`; WU2/WU3 →
> `b2c1923`, `2de3905`; WU4/WU5 → `b0a6fa2`, `273a1cd`; WU6/WU7 → `daaad0c`.
> Verification: exact `tsc 6.0.3` replay passed 60/60, source projection 13/13, replay access audit
> 6/6, B15 6/6, B72 6/6, and full workspace tests including conformance and checker 944/944 passed.
> Formatting and all-target clippy with warnings denied passed. Independent review ended PASS with
> no HIGH/MEDIUM/LOW findings and 7/7 negative controls firing.
> Backlog `15` remains open for default, namespace/star, package/config, declaration-file, and cycle
> breadth. Backlog `72` still owns the public-project witness, mutation pack, ratchet, and CI gate.

# Sprint — acyclic named source re-exports (2026-08-08)

**Goal.** Admit acyclic local named source re-exports on the production Bundler project route,
including aliases, outer and inline type-only forms, and chains, without creating fake local
bindings or broadening the module profile.

**Theme.** The shipped project route already resolves local named imports through
`oxc_resolver`, orders acyclic dependencies, and publishes value/type export slots. A named source
re-export needs the same physical lookup plus a direct projection from the target module's
`ExportedSlots` into the barrel's export surface. This sprint adds only that missing semantic edge.
It does not add a second resolver, a default slot, namespace/star publication, cycle handling, or a
public-project witness. Success means this exact form moves from an explicit exit-3 notice to
checked production behavior while every deferred form remains explicit and non-clean.

## Refs re-verified at HEAD (`5c15154`, 2026-08-08)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `ProjectProgram` carries only resolved named imports; no typed source-re-export product crosses
  the frontend/checker boundary — `crates/typokat-frontend/src/frontend.rs:135-163`.
- ✔ Project assembly maps `RawImport` rows into dependency-ordered `ProjectImport` rows —
  `crates/typokat-frontend/src/frontend.rs:591-650`.
- ✔ The production project inventory uses `Resolver::default()` and `resolve_dts`; resolved targets
  must already be configured project roots. This is the existing authority for extensionless and
  `.js` to `.ts` substitution — `crates/typokat-frontend/src/frontend.rs:697-723` and
  `crates/typokat-frontend/src/frontend.rs:1143-1197`.
- ✔ Named source re-exports are currently recorded as `source-reexport -> unsupported`, while
  star and namespace re-exports and default exports have separate explicit dispositions —
  `crates/typokat-frontend/src/frontend.rs:919-974`.
- ✔ The inventory already owns deterministic cycle reporting, but its edge set is currently built
  from admitted imports — `crates/typokat-frontend/src/frontend.rs:1021-1038` and
  `crates/typokat-frontend/src/frontend.rs:1290-1342`.
- ✔ Dependency order is derived only from `RawImport` rows, so re-export edges must be included
  deliberately rather than inferred later by the checker —
  `crates/typokat-frontend/src/frontend.rs:1435-1475`.
- ✔ `ExportedSlots` carries value and type identities plus value-erasure and unavailable-type
  barriers. It has no namespace slot — `crates/typokat-check/src/check/checker/mod.rs:1479-1490`.
- ✔ The serial project binder consumes already-published dependency surfaces before binding the
  next module — `crates/typokat-check/src/check/checker/mod.rs:3064-3084`.
- ✔ Named imports already consume `ExportedSlots` and own `TK2307`/`TK2305` reporting —
  `crates/typokat-check/src/check/checker/mod.rs:3476-3533`.
- ✔ Export collection deliberately skips every export with a source; local list exports already
  implement outer/inline type-only value erasure and lookup barriers —
  `crates/typokat-check/src/check/checker/mod.rs:3543-3579` and
  `crates/typokat-check/src/check/checker/mod.rs:3670-3718`.
- ✔ The public contract pins named source re-exports as exit `3`, and separately pins cycles as
  exit `3` — `tests/cases/b72_bundler_project_tracer/contract.json:487-489` and
  `tests/cases/b72_bundler_project_tracer/contract.json:528-530`.
- ✔ The public driver routes are distinct: legacy `check_project` is
  `crates/typokat-driver/src/driver.rs:209-232`; accounted explicit `check_project_once` is
  `crates/typokat-driver/src/driver.rs:234-264`; accounted Bundler
  `check_bundler_project_once` is `crates/typokat-driver/src/driver.rs:266-287`. The last two join at
  `check_project_once_inner`, while the legacy path still uses `run_project_frontend`/`scan_imports`.
- ⚠ Backlog `15` previously described the next slice as value/type/**namespace** publication, but
  the live export surface has no namespace slot. This sprint admits only the already-representable
  value/type pair, including a class's value and type identities. Namespace-bearing forms remain
  explicit exit `3`.
- ✔ The semantic oracle available at planning time reports `Version 6.0.3`. Every fixture below
  must be run with `--strict --noEmit --module esnext --moduleResolution bundler`; exact output is
  recorded before implementation.
- ⚠ The exact oracle falsified the initial empty-list design. For both an existing and absent source,
  `/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc --strict --noEmit --module esnext
  --moduleResolution bundler -p tsconfig.normal.json` reports exit `0` and no diagnostics for
  `export {} from "<source>"`; the same command with `tsconfig.reverse.json` is byte-identical.
  This syntax is erased before module resolution; it is not an admitted empty re-export.

## Admission and namespace-safety contract

There is one cross-crate evidence boundary: opaque frontend-owned
`AdmittedSourceReexports`. Its fields are private and it contains two opaque declaration variants:

- `Resolved` retains the resolved target and declaration/member rows. Every requested exported name
  carries `NamespaceProvenance::ProvenAbsent` evidence. A requested name may still be absent from
  the target surface; the evidence proves only that no hidden namespace meaning can be dropped, so
  checker lookup may then emit `TK2305`.
- `Missing` bypasses namespace census because no target exists. For a non-empty named list, it
  retains declaration identity, module/source span, `owner_start`, and the complete member list so
  the checker can emit exactly one declaration-owned `TK2307`.

The checker receives and consumes this product but cannot construct or mutate it. Production
construction stays a private frontend operation after Bundler
inventory, resolution, dependency ordering, cycle detection, and provenance classification have
all completed. No public/manual constructor, caller-provided boolean, parallel checker token, or
independently forgeable row exists. A constructor for focused internal tests may exist only behind
the workspace's existing `test-utils` feature. Other callers may forward an opaque product created
by the frontend, but cannot forge its fields or upgrade an unadmitted census.

WU2 may land the opaque type and its collector while the production Bundler path never invokes its
constructor. WU4 consumes test-created opaque products internally; it does not add a second
capability. WU6 is the only work unit that makes the Bundler frontend call the private production
constructor and routes that exact product through the driver to the checker. No new production
feature flag is introduced.

WU1 freezes the exact current result of three distinct route families; it does not assume they
share one outcome or even one frontend:

- **Bundler:** directory/`tsconfig.json` CLI and `check_bundler_project_once` use accounted Bundler
  inventory and currently block semantics on the source-re-export notice.
- **Explicit accounted:** explicit-file CLI and `check_project_once` use
  `ProjectResolutionMode::ExplicitFileList` and retain their current accounted notices/results.
- **Legacy:** `check_project` uses the older `run_project_frontend`/`scan_imports` path and has no
  project inventory. WU1 records its exact returned reports byte-for-byte without treating them as
  evidence that source re-exports are sound or explicitly refused.

WU2 and WU4 must preserve all three frozen baselines byte-identically. WU6 changes only the Bundler
family. Explicit `check_project_once` and legacy `check_project` remain exact frozen baselines; this
sprint makes no public semantic claim about the legacy path.

The frontend owns namespace safety before `blocks_semantics` is decided. Its bounded
syntax/provenance census follows local exported declarations, local aliases, source-re-export
aliases, and acyclic re-export chains and assigns
`NamespaceProvenance::{ProvenAbsent, PresentOrUnknown}`. The default is `PresentOrUnknown`; aliases
and chains propagate it without narrowing. For a resolved target, `PresentOrUnknown` adds an exact
project notice, makes `blocks_semantics` true, and emits no `Resolved` admitted declaration. Only
structurally proved `ProvenAbsent` requested names enter that variant. A non-empty list with a
missing target enters `Missing` without namespace evidence. An empty list is erased before either
path. A namespace-only export, class+namespace merge,
function+namespace merge, or unresolved provenance therefore remains frontend-owned explicit
unsupported input. It must not become `TK2305`, project only the visible value/type pair, or
disappear through an error type.

The census is metadata over the already-accounted acyclic module graph, not a second semantic export
graph. If proving absence requires recreating checker publication, resolving semantic slots in the
frontend, or adding another export graph, stop and re-plan. If the checker ever observes unknown
provenance inside a `Resolved` declaration, that is a typed internal invariant failure. It is never a user
`TK2305` and never converted back into an exit-3 project notice.

## Work units

### WU1 — commit the disabled oracle corpus (effort S)

- **Problem.** The only committed source-re-export row is a clean unsupported-form guard. It cannot
  specify slot projection, aliases, type-only barriers, dependency chains, missing targets, or the
  rule that a re-export does not create a local binding.
- **Verify first.** Run every proposed project against exact `tsc 6.0.3` with the sprint oracle
  flags. Record the full ordered diagnostic identity, not only the exit code. Run the current
  Bundler route at `5c15154` and prove the positive cases remain exit `3`; this is the RED negative
  control. Record the explicit and legacy routes separately rather than projecting that result onto
  them.
- **Scope.** Add one permanently disabled raw-conformance corpus,
  `tests/cases/b15_acyclic_source_reexports/`, its `tests/conformance.rs` registration, and its
  `tests/cases/README.md` entry. Add a separate black-box CLI/driver acceptance contract marked
  ignored/RED until WU6, plus an always-enabled route-baseline guard that expects each route's exact
  current bytes/result through WU2 and WU4. Commit this spec alone; it must not change production behavior.
  The raw corpus remains disabled permanently because project semantics are owned by black-box
  integration tests; only the separate acceptance contract is enabled in WU6. The exact matrix is:

  1. `export { value as renamed } from "./source.js"` with a clean consumer and a `TS2322`/`TK2322`
     value-type mismatch; the `.js` specifier must resolve to the configured `.ts` root.
  2. `export type { Shape } from` and `export { type Shape as RenamedShape } from`, each with clean
     type consumption and a value-space misuse whose exact `tsc 6.0.3` result is recorded.
  3. A mixed list containing an ordinary value and an inline type-only item. Neither item may erase
     or manufacture the other item's slot.
  4. A two-barrel chain (`source -> barrel-a -> barrel-b -> consumer`) for a value and a type.
  5. A re-exported class used both as a constructor value and as an instance type, through the
     chain. This pins the existing value/type pair; it is not namespace support.
  6. A barrel that re-exports `value` and then refers to bare `value` locally. The local use must
     report `TK2304`. This is the no-fake-local-binding falsifier.
  7. A missing source module, which must report `TK2307`, and a resolved source without the requested
     member, which must report `TK2305`. Add `export { a, b } from "./missing.js"` as a separate
     oracle: exact `tsc 6.0.3` emits one `TS2307` for the declaration, so typokat must emit one
     grouped `TK2307`, with one deterministic resolution-summary owner rather than one record per
     member. The missing declaration uses the opaque `Missing` variant and bypasses namespace
     census. Neither path may fall through an error type and appear clean.
  8. An explicit `export {} from "./source.js"` disposition against both an existing and missing
     source. Both are clean erased syntax: no resolver invocation, resolution-summary row,
     dependency/cycle edge, opaque admitted declaration, `TK2307`, local binding, or exported slot.
  9. Duplicate output names in a complete matrix: source/source and source/local, each in both
     declaration orders. For source/source, record the exact `tsc 6.0.3` `TS2300` on both source
     declarations. Pin the exact source/local sites as well. The implementation may not rely on
     `BTreeMap::insert` last-write behavior.
  10. Fail-closed namespace-provenance controls: namespace-only, class+namespace,
      function+namespace, and an aliased two-barrel namespace-bearing chain. Record clean/diagnostic
      `tsc 6.0.3` semantics, but require the Bundler route to emit the exact deterministic notice
      `unsupported-source-reexport-namespace-provenance <site> <specifier> <exported-name>`, block
      semantics, and produce no opaque admitted product. No case may become `TK2305` or a partial
      value/type export.
  11. Every admitted Bundler project through both `tsconfig.files` orders. Diagnostics and the JSON
     project summary must be byte-identical.
  12. A re-export-only cycle and a mixed import/re-export cycle. Both remain deterministic exit `3`
      with `unsupported-module-cycle`; no partial surface may be checked.
  13. Negative controls for default import/export, star re-export, namespace re-export,
      string-literal export names, export attributes, bare package specifiers, and namespace-bearing
      targets. Each remains the same explicit exit-3 family as at `5c15154`.
  14. A route matrix for the same source-re-export input, frozen independently through WU4:
      Bundler CLI plus `check_bundler_project_once`; explicit-file CLI plus accounted
      `check_project_once`; and legacy `check_project` through `run_project_frontend`/`scan_imports`
      with no inventory. Record exact stdout/stderr/exit for CLI and exact returned reports/inventory
      where each driver API actually exposes them. The WU6 movement is Bundler-only; no baseline is
      interpreted as proof of legacy source-re-export soundness.
- **Acceptance / witness.** The spec commit is behavior-neutral and green because the new raw
  corpus is permanently disabled and the black-box RED acceptance is ignored until WU6. The
  always-enabled route guard passes. The oracle record contains exact `tsc 6.0.3` commands and
  outputs. Running the ignored acceptance explicitly against the pre-change binary fails every new
  admitted-form expectation, while each unsupported control exercises its intended Bundler notice.
  The explicit and legacy guards pin their distinct current results without relabelling them. The
  namespace provenance matrix proves that unsupported namespace payload cannot be confused with a
  missing member. No expectation is copied from current typokat output.
- **Touch points.** `tests/cases/b15_acyclic_source_reexports/`, `tests/cases/README.md`,
  `tests/conformance.rs`, and a focused black-box integration test covering RED acceptance plus the
  always-enabled public-route guard.

### WU2 — retain typed re-export edges in the frontend (effort M)

- **Problem.** The frontend currently discards all named source-re-export semantics after recording
  an unsupported notice. The checker therefore receives neither the requested/exported names nor a
  dependency edge.
- **Verify first.** Trace one alias, one outer type-only item, one inline type-only item, one missing
  target, and one re-export-only cycle from the OXC AST through project accounting. Confirm the
  existing `module_export_name` rejection of string-literal names and the exact resolver outcome
  used by named imports.
- **Scope.** An implementation subagent adds a typed project re-export declaration with a stable
  declaration identity, module/source span, declaration `owner_start`, resolved/missing target,
  and member rows that retain imported name, exported name, member span, and type-only bit. The
  declaration identity groups missing-module reporting and summary ownership, including
  `export { a, b } from "./missing.js"`. An empty member list is erased before resolution and creates
  no typed declaration or graph edge.
  Reuse the existing `oxc_resolver` call and configured-root policy. Add resolved re-export targets
  to dependency ordering and cycle accounting. Before deciding `blocks_semantics`, run the bounded
  syntax/provenance census and propagate `PresentOrUnknown` through local aliases and source chains.
  For resolved targets it emits the exact WU1 notice and no `Resolved` variant for any requested
  name that is not `ProvenAbsent`. A non-empty list with a missing target instead creates one
  `Missing` declaration with all declaration/member metadata and bypasses namespace census.
  String-literal names,
  attributes, default/star/namespace forms, namespace-bearing or unknown targets, bare packages,
  unconfigured targets, and cycles stay unsupported. Land the opaque
  `AdmittedSourceReexports` type, private fields/accessors, and collector, but leave its private
  production constructor unreachable. Only the existing `test-utils` feature may expose a test
  constructor. Bundler, explicit accounted, and legacy route baselines remain byte-identical.
- **Acceptance / witness.** Focused frontend tests prove exact row extraction, aliasing, outer/inline
  type-only bits, both spans, declaration identity/owner, grouped missing resolution, empty-list
  erasure with zero resolution/edge/product rows, `.js` substitution, stable order, namespace
  provenance propagation, and non-empty re-export cycle edges. The always-enabled WU1 guard proves
  each distinct route still reports its exact pre-change
  result. No filesystem fallback, second semantic export graph, or second resolver is introduced.
- **Touch points / ownership.** The implementation agent owns only
  `crates/typokat-frontend/src/frontend.rs`, focused frontend tests, and
  `crates/typokat-frontend/Cargo.toml` only if the existing `test-utils` exposure needs explicit
  wiring. It must not edit checker, CLI, driver, corpus expectations, or docs, and must not add a
  production feature.

### WU3 — independently review and land the frontend product (effort S)

- **Problem.** A frontend row can look complete while omitting a graph edge, losing type-only
  syntax, or changing public accounting before semantics exist.
- **Verify first.** A different, context-free reviewer receives the WU1 spec, the exact WU2 diff,
  the live resolver path, and the pre-change contract. It does not edit files.
- **Scope.** Hunt missing syntax branches, duplicate rows, unstable ordering, lost source/member
  spans, forged or unstable declaration identity, per-member `TK2307`, resolver drift, empty-list
  resolution/admission, namespace-provenance narrowing, a forgeable opaque-product field/constructor, a census
  that duplicates semantic export publication, re-export-only cycles, and any movement on the three
  frozen route families. Require a concrete falsifier for each claimed invariant.
- **Acceptance / witness.** Reviewer returns PASS with no unresolved HIGH/MEDIUM findings. On FAIL,
  the WU2 agent fixes the same frozen diff and the reviewer rechecks it. The leader runs the focused
  frontend gate, full `cargo test`, formatting, and clippy, then commits only the reviewed frontend
  paths.
- **Touch points / ownership.** Review is read-only. The leader alone stages explicit WU2 paths and
  verifies the cached path list before commit.

### WU4 — project target slots directly into the barrel surface (effort M)

- **Problem.** `collect_exports` skips source re-exports, although every acyclic dependency surface
  is already available. Routing a re-export through `ImportedSymbol` would create a fake local
  binding and violate TypeScript semantics.
- **Verify first.** Follow `ExportedSlots` from a dependency through an ordinary named import and
  compare local list-export type-only handling. Confirm the target surface exists before the barrel
  under both `tsconfig.files` orders. Confirm the opaque product exposes only read access to
  frontend-proved rows and cannot be manually constructed in production.
- **Scope.** A checker implementation subagent consumes opaque `AdmittedSourceReexports`. For a
  `Resolved` declaration, it projects the target `ExportedSlots` directly into the current module's
  `ExportSurface` under the exported name.
  Outer or inline type-only syntax removes the value slot and preserves `value_erased` and
  `type_unavailable` barriers. A non-empty `Missing` declaration emits exactly one `TK2307` at its
  source span before member iteration; its declaration identity supplies one summary owner
  regardless of member count. A `Resolved` declaration may emit `TK2305` for an absent requested
  member only after validating that member's `ProvenAbsent` evidence. `export {} from` never reaches
  the checker. Chained
  surfaces reuse the same projection. Duplicate source/source
  and source/local output names follow the exact WU1 oracle in both orders and never silently
  overwrite. The checker does not reclassify namespace provenance: the opaque product certifies
  `ProvenAbsent` on every resolved requested name. If a deliberately invalid test product exposes
  unknown provenance in a `Resolved` declaration, return a typed
  internal invariant failure before member lookup; never emit `TK2305`, an exit-3 notice, or a
  partial class/function value/type pair. No `ImportedSymbol`, binder declaration, local symbol,
  synthetic error type, second capability, or manual production constructor is created. Internal
  tests receive products only from the frontend's existing-`test-utils` constructor. All three
  public route families remain at their frozen WU1 baselines.
- **Acceptance / witness.** Internal checker tests pass the WU1 slot, chain, class, non-empty
  missing, duplicate, and no-local-name cases while every production route remains fail-closed
  pending WU6. The enabled route guard proves empty lists never reach the checker.
  The namespace-only, class+namespace, function+namespace, and aliased-chain controls are stopped by
  the frontend and never reach the checker. A deliberate fake-local-binding mutant makes the
  `TK2304` falsifier fail. A
  deliberate value/type-slot collapse makes the class or type-only fixture fail; a deliberate
  invalid opaque product makes the typed internal-invariant test fail.
- **Touch points / ownership.** The implementation agent owns
  `crates/typokat-check/src/check/checker/mod.rs`, focused internal checker tests, and
  `crates/typokat-check/Cargo.toml` only if consuming the frontend `test-utils` constructor needs
  existing-feature wiring. It must not edit frontend, binder, relation/inference, CLI, public
  contract, or docs, and must not add a production feature.

### WU5 — independently review and land slot projection (effort S)

- **Problem.** The highest-risk failures are silent: a missing target can become an error type, a
  type-only barrier can disappear, or `BTreeMap` insertion can hide a duplicate.
- **Verify first.** A different reviewer receives only the WU1 oracle, exact WU4 diff, live
  `ExportedSlots` consumers, and the no-local-name and duplicate falsifiers. It does not edit files.
- **Scope.** Hunt false negatives, order dependence, span/owner mistakes, local-binding leakage,
  grouped-diagnostic splits, accidental empty-list resolution/admission, barrier loss, namespace-provenance narrowing,
  confusion between `Resolved` and `Missing`, opaque-product forgery, namespace leakage, and
  last-write behavior. Re-run the three distinct WU1
  route baselines: Bundler, explicit accounted, and legacy. Cross-check disputed cases with exact
  `tsc 6.0.3`.
- **Acceptance / witness.** Reviewer returns PASS with no unresolved HIGH/MEDIUM findings. On FAIL,
  route fixes to the WU4 agent and re-review the frozen diff. The leader runs focused checker tests,
  full `cargo test`, formatting, and clippy, then commits only reviewed checker paths.
- **Touch points / ownership.** Review is read-only. The leader alone verifies and commits the exact
  WU4 path set.

### WU6 — atomically admit the Bundler route and regress its summary (effort M)

- **Problem.** WU2 and WU4 deliberately leave the production Bundler route non-clean. Removing the notice
  before both products exist would create another mixed internal/public state.
- **Verify first.** On current HEAD, prove the internal frontend row and checker projection both pass
  while the old B72 source-re-export contract and all three WU1 route families retain their exact
  frozen results. Confirm no production code can construct `AdmittedSourceReexports` yet.
- **Scope.** In one uncommitted integration diff, stop classifying only admitted named source
  re-exports as unsupported in the Bundler family, make only the accounted Bundler frontend invoke
  the private production constructor after `blocks_semantics`/namespace classification, route that
  opaque product through `check_bundler_project_once` to the checker, unignore the separate WU1
  black-box acceptance, and update the deterministic JSON summary. The permanently disabled
  raw-conformance corpus remains disabled. Explicit-file CLI/`check_project_once` and legacy
  `check_project` do not construct or receive the product and retain their exact WU1 baselines.
  Only the intended Bundler files move from skipped to checked; resolved/missing specifiers remain
  accounted; diagnostics retain stable declaration ownership and order. Resolved namespace-bearing
  or unknown targets emit their exact frontend notice, block semantics, and produce no `Resolved`
  declaration; non-empty missing targets still produce the diagnostic-only `Missing` variant.
  Empty named source exports are erased before resolution and produce neither variant. Every other
  deferred-form contract row remains byte-identical. Update the old B72 Bundler source-re-export
  row so it no longer contradicts the admitted form, without changing the explicit or legacy
  baselines or rebaselining unrelated rows.
- **Acceptance / witness.** Directory and `tsconfig.json` invocations agree. Both caller root orders
  in `tsconfig.files` are byte-identical. `.js` substitution, chains, class slots, type-only barriers, `TK2307`,
  grouped multi-member `TK2307`, `TK2305`, clean zero-accounting empty lists, both duplicate
  matrices, and the no-local-name `TK2304` pass through the real CLI route. Namespace provenance controls, re-export-only
  cycles, and mixed cycles still exit `3`. A summary diff contains only the planned Bundler
  source-re-export movements. Always-enabled guards prove explicit-file CLI/`check_project_once`
  and legacy `check_project` remain byte-for-byte/result-for-result at their distinct frozen
  baselines. This acceptance makes no source-re-export soundness claim for legacy `check_project`.
  The full existing B72 integration contract stays green.
- **Touch points / ownership.** One integration agent owns the minimal frontend/checker exposure
  seam, `crates/typokat-driver/src/driver.rs`, the focused CLI/driver integration test, the B72
  contract row, and the WU1 acceptance files. It may edit `crates/typokat-frontend/Cargo.toml`,
  `crates/typokat-check/Cargo.toml`, or `crates/typokat-driver/Cargo.toml` only for existing
  `test-utils` test wiring; production routing must not enable that feature or expose a constructor.
  It must not edit resolver breadth, binder, type model, default/star/namespace support, or docs.
  This diff is not committed before WU7.

### WU7 — adversarial public review and leader commit (effort M)

- **Problem.** Internal tests cannot prove that public accounting, exit codes, and diagnostic
  publication all use the same admitted route.
- **Verify first.** Freeze the exact WU6 diff. Build a pre-change `5c15154` binary in an isolated
  scratch worktree and confirm it fires on the positive source-re-export corpus while current code
  passes. Also run deliberate broken variants: omit a non-empty re-export graph edge, resolve or
  admit an empty list, create a
  local barrel binding, collapse class slots, erase the type-only barrier, split one missing module
  into per-member diagnostics, incorrectly block a non-empty missing target in namespace census, admit a
  resolved missing member without `ProvenAbsent`, drop namespace provenance across an alias, restore
  silent last-write, and forge an opaque product or route one through explicit/legacy entry points.
- **Scope.** A new read-only reviewer hunts false negatives and cross-checks every semantic case
  against exact `tsc 6.0.3`. Review public CLI output, both `tsconfig.files` orders, grouped
  declaration diagnostics/summary ownership, empty lists, duplicate output names, namespace
  provenance, empty-list erasure, and all unsupported controls. Prove only the Bundler frontend constructs the opaque
  product and that explicit accounted and legacy results remain at their distinct WU1 baselines.
  Inspect the full diff for
  accidental default, namespace, package, cycle, resolver, or type-model breadth.
- **Acceptance / witness.** Reviewer returns independent PASS with no unresolved HIGH/MEDIUM
  findings, and every negative control demonstrably fires. The leader then runs the complete gate:
  full tests, all integration targets, formatting, clippy with warnings denied, WU1 production spot
  runs, and exact `tsc 6.0.3` replay. Because this slice must not touch inference, contextual typing,
  argument walking, or overload resolution, the randomized differential gate is not required; if
  the diff touches any of those surfaces, stop and add the dev-method differential plus its broken
  control before commit. The leader commits only the reviewed WU6 paths.
- **Touch points / ownership.** Review is read-only. The leader owns verification, explicit staging,
  and commit.

### WU8 — record the result and archive the sprint (effort S)

- **Problem.** Shipping one backlog-15 slice does not close backlog `15` or the public-witness
  promise in backlog `72`.
- **Verify first.** Compare docs to the shipped public contract and final commit map. Confirm every
  deferred form on the Bundler route still has an explicit non-clean guard and the explicit/legacy
  baselines did not move.
- **Scope.** Stamp the closure header and run log evidence, archive this sprint, update active indexes,
  and rescope backlog `15` to the next unshipped module slice. Do not delete backlog `15` or `72`.
  Update reference docs only where production behavior actually changed.
- **Acceptance / witness.** No active index points to the archived path. Backlog `15` names exact
  shipped and deferred breadth. Backlog `72` still requires its independent zero-clean witness,
  mutation pack, runner, and CI ratchet.
- **Touch points / ownership.** Leader-owned docs only, plus any reference page whose live behavior
  changed.

## Out of scope (explicit)

- Default imports and default declaration/expression exports.
- Star exports, namespace imports/re-exports, namespace-bearing target slots, string-literal export
  names, export attributes, side-effect imports, import-equals, `export =`, and
  `export as namespace`.
- Cyclic module semantics, SCC publication, partial cycle surfaces, or any replacement for the
  current deterministic exit-3 cycle notice.
- Bare packages, `node_modules`, `@types`, package `exports`/`imports` or `types`, symlinks, path
  aliases, tsconfig inheritance/references, `.d.ts` loading breadth, additional root selection, and
  alternate host profiles.
- Checker/type-model, binder, relation, inference, contextual-typing, argument-walking, overload,
  or diagnostic-completeness fixes that are not necessary to preserve the already-represented
  value/type slots.
- Backlog `72` witness selection, mutations, runner, scoreboard, or CI. A clean candidate result is
  evidence for the next plan, not closure of `72`.
- Performance claims, parallel checking, and incrementality. This is serial correctness breadth.

## Decisions

- **Project target slots; do not import then re-export.** A source re-export changes the module's
  export surface but does not declare the imported name in the barrel. Direct projection preserves
  that language rule and gives the no-local-name fixture a precise falsifier.
- **Reuse physical resolution.** The source specifier follows the existing named-import
  `oxc_resolver` path and configured-root policy. Typokat adds module semantics, not another lookup
  authority.
- **Admit only representable, namespace-free slots.** The current surface represents value and type
  identities and their barriers. An ordinary class is admitted because it is a proven value/type
  pair; a class+namespace or function+namespace merge is not. Conservative provenance survives
  aliases and chains. For resolved targets, `PresentOrUnknown` remains unsupported until a
  separately specified namespace surface exists. Non-empty missing targets use the opaque
  diagnostic-only `Missing` variant and do not pretend to have namespace provenance.
- **Erase empty named source exports before resolution.** Exact `tsc 6.0.3` accepts
  `export {} from "<source>"` even when the source is absent. The frontend therefore creates no
  resolution, dependency/cycle edge, summary row, admitted declaration, or diagnostic. This is
  erased syntax, not an empty `Resolved`/`Missing` admission.
- **Cycles stay fail-closed.** Re-export edges participate in cycle detection and dependency order,
  but this sprint does not publish cyclic surfaces.
- **Public admission is opaque-product-gated and atomic.** The frontend owns one
  `AdmittedSourceReexports` evidence type with private fields; the checker only consumes it. WU2 and
  WU4 may land the collector/consumer while no production frontend constructs the product. WU6
  makes only the accounted Bundler frontend construct and route it after all guards pass. Explicit
  and legacy entry points cannot forge it and do not move.
- **The fixture corpus and executable RED are separate.** The raw project-shaped corpus remains
  permanently disabled. Its black-box acceptance is ignored only until WU6, while an always-enabled
  guard pins all three distinct route baselines through the internal commits.
- **Candidate screening is non-authoritative.** The six pinned candidates may be re-run read-only
  after WU6 to learn the next first blocker. The screen may not change this sprint, patch a
  candidate, relax a threshold, or claim backlog `72`.

## Sequencing and ownership

| Order | Owner | Mutable paths | Commit boundary |
|---|---|---|---|
| WU1 | leader/spec author | new B15 corpus, registration/docs, ignored RED acceptance, enabled route guard | separate spec commit |
| WU2 → WU3 | frontend implementer → independent reviewer → leader | frontend product/census, focused tests, frontend manifest only for existing `test-utils` wiring | reviewed frontend commit |
| WU4 → WU5 | checker implementer → different independent reviewer → leader | opaque-product consumer, focused tests, checker manifest only for existing `test-utils` wiring | reviewed checker commit |
| WU6 → WU7 | integration implementer → new adversarial reviewer → leader | Bundler-only opaque-product routing, unignored acceptance, exact route baselines | reviewed integration commit |
| WU8 | leader | closure/index/backlog/reference docs | closure/archive commit |

Only one RED/root-cause cluster may be active. WU2, WU4, and WU6 are serial semantic gates; do not
open the next one until the current frozen diff has independent PASS and lands. Candidate screening
may run in parallel only as read-only evidence. The namespace-binder refactor stays paused while
this sprint touches project binding/checker publication. Checker-scaling work may proceed only in
explicitly disjoint files; if it reaches any owned file above, serialize it. Every agent must treat
a build error in a file it does not own as another owner's work in progress: report it, do not fix
it.

Serialize every Cargo invocation through the shared lock and one queue, and lease CPU for every
CPU-bound gate, for example:

```sh
flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 2 -- cargo test --test b15_acyclic_source_reexports_cli'
flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 2 -- cargo test'
flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 2 -- cargo clippy --all-targets -- -D warnings'
```

Do not overlap Cargo jobs. If `cpu-lease` is unavailable or not enforcing, stop instead of running
or reporting an unleased heavy gate. This sprint has no benchmark work; any later benchmark must
also use `--no-smt` and the shared lock.

Before every commit, print `git diff --cached --name-only` and confirm it matches the relevant table
row exactly. Stage explicit paths only. Implementation agents and reviewers do not commit.

## Stop / falsifier gates

Stop and re-plan instead of expanding scope if any of these occurs:

- Direct slot projection requires a fake local binding or an `ImportedSymbol` in the barrel.
- Correct ordering requires SCC/cyclic publication rather than adding acyclic re-export edges to the
  existing graph.
- A target needs a namespace slot or other type-model representation absent from `ExportedSlots`.
- Namespace absence for a resolved requested name cannot be proved structurally, or
  `PresentOrUnknown` would be narrowed across a local/source alias or chain. Stop rather than emit
  `TK2305` or publish a partial value/type pair.
- The bounded provenance census would require a second semantic export graph or checker-style slot
  resolution in the frontend.
- Exact duplicate-output behavior requires solving general duplicate identifiers from backlog `18`
  rather than a bounded export-surface rule.
- The frontend commit changes the public source-re-export disposition before checker semantics land.
- `AdmittedSourceReexports` fields or a production/manual constructor become public, the checker can
  forge the product, any production path constructs it before WU6, or WU6 routes it through
  ExplicitFileList/`check_project_once`/legacy `check_project`.
- The checker converts impossible unknown provenance in a `Resolved` declaration into `TK2305` or an
  exit-3 notice instead of a typed internal invariant failure.
- A missing module/member becomes clean through an error type or loses `TK2307`/`TK2305`.
- A non-empty missing target is blocked by namespace census, fails to produce a `Missing` variant,
  or an absent member on a resolved target reaches `TK2305` without per-name `ProvenAbsent` evidence.
- A non-empty missing source declaration emits one `TK2307` per member, loses its declaration/source span,
  or produces multiple summary owners.
- An empty named source export invokes resolution, contributes a dependency/cycle edge or summary
  row, constructs an admitted variant, or emits `TK2307`/`TK2305`.
- The implementation needs default/star/namespace/package/config/resolver breadth, changes the
  checker type model, or touches inference/contextual typing/argument walking/overload resolution.
- A candidate screen becomes an argument to add unrelated syntax or model fixes to this sprint.

## Run log

<!-- Append discoveries/blockers here. Graduate changed rationale to an ADR and future work to the
     backlog; leave transient execution notes here until archive. -->

- The exact `tsc 6.0.3` oracle proved that `export {} from` is erased even for a missing target;
  `f29d687` corrected the plan before implementation.
- The first WU6 candidate changed seven B38 diagnostics from the existing `TK2304` stand-in to
  `TK2693`. Exact TS1361/TS1362 replay rejected that rebaseline, so `daaad0c` carries explicit
  provenance for “never had a value” versus “a real value was erased.”
- Independent review passed the frozen candidate, but the leader's complete workspace gate then
  caught one raw type lookup rejected by the replay-index audit. The final candidate uses the
  replay-aware lookup, passed a second independent review, and then passed the complete gate.
