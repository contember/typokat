# Sprint — object binding publication (2026-08-09)

**Goal.** Publish correctly typed leaves from flat object variable binding patterns, then re-screen
the pinned `placetext` project to expose its next general blocker.

**Theme.** The post-default-slot re-screen proves that the bounded module route is complete for
`placetext`, while one old declaration-model omission causes 22 of its 27 diagnostics. This sprint
ships that one general slice. It does not try to qualify the project or close all of backlog `48`.

## Refs re-verified at HEAD (2026-08-09)

Baseline is `943d810`. `✔` = confirmed live · `⚠` = drift or boundary caught.

- ✔ The source declaration prewalk already records every binding leaf, and the semantic binder
  already attaches the correct declaration scope to every leaf —
  `crates/typokat-binder/src/binder/declaration.rs:261`,
  `crates/typokat-binder/src/binder/bind.rs:1251`.
- ⚠ `bind_declarator` then publishes only a plain identifier. Its helper explicitly returns `None`
  for destructuring as an old out-of-M3 boundary —
  `crates/typokat-binder/src/binder/bind.rs:2650`,
  `crates/typokat-binder/src/binder/bind.rs:3175`.
- ⚠ The checker also resolves and publishes one `ValueStorageId` for the whole declarator. A sound
  implementation needs one identity and type per admitted leaf, including independent flow
  invalidation — `crates/typokat-check/src/check/checker/statements.rs:1291`,
  `crates/typokat-check/src/check/checker/statements.rs:1630`,
  `crates/typokat-check/src/check/checker/statements.rs:2854`.
- ✔ Existing F4 checks inspect object-destructuring access control but deliberately defer leaf
  types — `crates/typokat-check/src/check/checker/statements.rs:1320`.
- ✔ The immutable `placetext` baseline checks 9/9 roots and resolves all 13 edges with no route,
  parse, skipped, or excluded record. It exits 3 with 7 incomplete records and 27 diagnostics; 22
  of 23 `TK2304` records are the unpublished-leaf cascade —
  `docs/archive/real-project-rescreen-2026-08-09/README.md`.
- ✔ Backlog `48` explicitly owns array/object/default destructuring binding patterns, but its full
  implicit-any diagnostic family is broader than this sprint —
  `docs/backlog/48-no-implicit-any.md`.

## Work units

### WU0 — commit the disabled RED binding corpus (effort S)

- **Problem.** No corpus pins typed publication for object binding leaves. Existing F4
  fixtures only pin access control, and the declaration-store unit test currently expects leaf
  storages to remain absent.
- **Verify first.** Cross-check every case against pinned `tsc 6.0.3 --strict`; inventory the exact
  pre-change typokat output and prove the acceptance rows fail for the intended missing-leaf reason.
- **Scope.** Add a dedicated disabled corpus for flat variable object patterns over `const`, `let`,
  and `var`: shorthand, rename, and assignment defaults over static identifier/string/number keys.
  Pin block shadowing, `var` function ownership, declaration order, optional-without-default,
  default-removes-undefined, invalid defaults, exactly-once default-expression checking, nested
  default diagnostics, explicit pattern-annotation precedence over a narrower initializer,
  subsequent assignment errors, and missing-property accounting. Add controls for ordinary
  identifiers and the existing access/collision paths.
- **Acceptance / witness.** The spec records exact `tsc 6.0.3` diagnostics. A forced isolated run
  proves the corpus RED for currently missing leaf values, then the committed milestone entry stays
  disabled so HEAD remains green. It must not use an error type as silent recovery or weaken an
  existing marker.
- **Touch points.** `tests/cases/b48_object_binding_publication/`, `tests/cases/README.md`,
  `tests/conformance.rs`, `tests/surface/inventory.toml` and its accounting witness; focused binder
  unit spec only if needed to pin one-leaf/one-storage identity.

### WU1 — publish flat object binding leaves (effort M)

- **Problem.** One declarator currently assumes one value identity, while a binding pattern owns
  several independent declarations.
- **Verify first.** Construct the exact per-leaf identity map from the existing declaration prewalk
  before changing lookup or publication. If replay, frozen-prefix, or lexical-event ownership
  cannot represent multiple leaf storages without a new architectural boundary, stop and ask; do
  not select an arbitrary first leaf or share one storage across leaves.
- **Scope.** Bind each admitted leaf into the correct lexical or `var` scope, assign a distinct
  storage identity, derive its type from a ready static property on a modeled object source and its
  optional default, publish it through the normal declaration path, and invalidate only that
  symbol's stale flow reads. An explicit aggregate pattern annotation is the source type and wins
  over a narrower initializer before property/default projection. Preserve source order, access
  checks, collision preflight, private replay, and ordinary-variable behavior. Enable WU0's corpus
  only in this implementation commit. Array, nested, rest, computed, parameter, catch, `for-in` /
  `for-of` loop-head, dynamic-key, and non-ready source shapes remain outside this slice and must
  stay explicitly non-clean; none may become a bound error-typed leaf that silently accepts later
  uses.
- **Acceptance / witness.** WU0 becomes green. Each leaf has a distinct symbol/storage/type; rename
  reads the source key and publishes the local name; optional properties retain `undefined` unless
  a valid default removes it; invalid defaults and nested default-expression errors match the
  pinned oracle and each default expression is checked once; bad later assignments report
  `TK2322`; missing properties and excluded shapes remain explicit. Existing F4, B102/B103,
  declaration-hoisting, replay, and production-library route tests remain green.
- **Touch points.** `crates/typokat-binder/src/binder/bind.rs`, binder declaration specs,
  `crates/typokat-check/src/check/checker/statements.rs`, and the narrow declaration/type/flow helper
  surface proved necessary by WU0.

### WU2 — adversarial review and semantic gates (effort M)

- **Problem.** Destructuring crosses binding, inference, flow, and library replay; a green focused
  corpus alone cannot exclude false negatives or identity aliasing.
- **Verify first.** A different subagent reviews the complete WU0–WU1 diff without using the
  implementer's conclusions.
- **Scope.** Hunt shared-storage aliases, wrong `var` owner, default-evaluation order errors,
  optionality loss, hidden missing-property recovery, property-key/local-name confusion,
  block-shadow leakage, frozen-prefix writes, replay omissions, and input-order drift.
- **Acceptance / witness.** Independent review returns no HIGH or MEDIUM finding. Run focused tests,
  full workspace tests, formatting, clippy, the official-suite ratchet, and randomized differential
  checks against the pre-change binary. The differential gate includes committed repros, seeds
  1/2/3 at 400 cases, a destructuring-focused generated matrix, and known-broken `412f321` as a
  negative control. The focused matrix must also run against pre-change `943d810` and fire on the
  unpublished leaves before its post-change zero result counts. A zero result from a generator that
  cannot emit the changed syntax is not evidence. Every review finding is fixed or explicitly
  dispositioned before closure.
- **Touch points.** Review only; `tooling/differential/` changes only if the committed generator
  needs a general destructuring grammar extension proved by its own negative control.

### WU3 — immutable project re-screen and close (effort S)

- **Problem.** Fixing one root may reveal diagnostics hidden by unresolved/error recovery.
- **Verify first.** Build a fresh release binary from the reviewed commit and record its SHA-256 and
  production route attestation.
- **Scope.** Re-run the exact immutable `placetext` commit with verified license/lock/config/source
  identities, pinned `tsc 6.0.3`, and a fresh empty cache. Preserve the native/overlay distinction
  and do not claim target/library equivalence without proof.
- **Acceptance / witness.** The exact 22 object-binding `TK2304` identities disappear without root,
  resolution, notice, parse, skip, or exclusion drift. Record every newly visible channel and the
  next general blocker. Backlog `72` remains open unless the unchanged zero-clean and meaning-
  equivalence gates genuinely pass; no mutation or witness work starts otherwise.
- **Touch points.** Sprint run log, backlog `72`, and a compact immutable evidence record.
  Rescope backlog `48` to distinguish the shipped flat-variable slice from its remaining work;
  archive this sprint with its OUTCOME and update the sprint/archive indexes.

## Out of scope (explicit)

- Array, nested, rest, or computed binding patterns; parameter, catch, and `for-in` / `for-of`
  loop-head bindings; assignment targets; and the rest of backlog `48`'s implicit-any diagnostics.
- Enum semantics (`42`), predicate signatures and call-flow narrowing (`50`), exact computed object
  keys (`75`), and template interpolation (`71`). These remain separate general features even
  though `placetext` uses them.
- Project-specific branches, source/config edits, ambient shims, library changes, resolver breadth,
  target/library equivalence claims, witness mutations, CI qualification, and performance claims.

## Decisions

- Optimize for the smallest general semantic slice that removes the measured cascade. Do not batch
  the five independent blocker families into one sprint.
- One binding leaf means one value identity. Sharing a declarator-wide storage is forbidden.
- A failed project gate records the next blocker and stops. It is progress evidence, not permission
  to weaken backlog `72`.

## Sequencing

WU0 → WU1 → WU2 → WU3. Only read-only enum-identity and predicate/flow phase probes may run in
parallel with WU1; they cannot add RED specs or implementation to this sprint.

## Run log
