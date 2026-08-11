> **OUTCOME — shipped 2026-08-11.** Flat object variable binding patterns now publish one distinct
> value symbol, storage identity, projected type, and flow state per admitted leaf. Static
> identifier/string/number keys, renames, defaults, optionality, shadowing, `var` ownership,
> collision replay, union projection, `any`, and recovery blocking are covered; excluded binding
> shapes remain explicitly non-clean. Production commits: publication `79348f0`, replay lookup
> `5636a85`, demand guard `c27c1ae`, and source projection `8638c4b`. The full WU2 gate passed:
> workspace tests including 15/15 conformance, clippy, formatting, and the 874-file official suite
> with zero ratchet regressions. Differential evidence passed 55/55 harness tests, the exact 8/8
> tsc matrix, 1,200 randomized pre-change comparisons, committed repros, and both known-broken
> controls. The immutable WU3 re-screen preserved 9 roots, 9/0/0 file accounting, 13 resolutions,
> and zero notices/parse errors while removing all exact 22 old object-binding `TK2304` identities.
> It remains non-clean at 7 incomplete records and 6 diagnostics; backlog `72` stays open and new
> backlog `109` owns the revealed optional-parameter `TK2345`. Independent WU2 and WU3 reviews
> returned **PASS**. No performance result is claimed.

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

- **2026-08-10 — WU2 project-order RED parked under backlog 76.** Declaration-first object `var`
  matches tsc's single wrong-type diagnostic; consumer-first reports two `TK2454` records and drops
  `TK2322`. The ordinary unannotated `var` control exits falsely clean in the same order, proving the
  root is backlog `76`'s general declaration-type demand. The two B48 project fixtures stay parked;
  no object-only pre-inference enters this sprint, and WU1 remains the same-file flat slice.
- **2026-08-10 — WU0/WU1 shipped.** The enabled corpus and binder inspectors prove independent
  leaf identities, static-key projection, optional/default behavior, lexical and `var` ownership,
  replay, excluded-shape accounting, and exact storage/private-public namespace identities.
- **2026-08-10 — WU2 review closed two false-green routes.** A stale array-pattern white-box
  assertion was corrected to require the intentional incomplete record. Official-suite triage then
  exposed direct-only source projection: `any`, internal error recovery, and common-key unions were
  misclassified as missing properties. Spec-first projection fixtures and inspector units led to
  the reviewed order-independent `Ready` / `Missing` / `Blocked` implementation; mixed missing and
  blocked unions emit one property diagnostic plus one pattern incomplete and never publish a
  recovery leaf. Dependent destructuring correlation remains a documented safe over-report under
  backlog `51`.
- **2026-08-10 — WU2 gates passed.** `cargo test`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo fmt --check` passed from the reviewed source. The full official suite classified 354 files
  in-scope and 520 out-of-scope across its 874-file corpus with zero regressions. The release binary
  SHA-256 is `628daee6cf1d55b6a96dd51e8aeb8cb8261f3661b545a72af552826fd72ace25`.
- **2026-08-10 — WU3 immutable re-screen passed its scoped acceptance.** Two typokat runs were
  byte-identical (`ed646172491a6aef5ddc9b3ff7e6037dc93a06b557762b417cea956464ffd9cb`).
  Every exact old object-binding `TK2304` disappeared with no route/accounting drift. The newly
  visible first blocker is `src/core/generator.ts:36:48 TK2345`: optional parameter calls reject an
  explicit `undefined`-bearing argument. The independent evidence review returned PASS; backlog
  `109` owns the bug and backlog `72` retains the unchanged zero-clean/meaning-equivalence gate.
