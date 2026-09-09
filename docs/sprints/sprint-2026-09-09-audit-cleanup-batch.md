<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 → <sha>,
> WU2 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — audit cleanup batch: delete, dedupe, own once (2026-09-09)

**Goal.** Land the behaviour-neutral cleanups graduated from the 2026-09-06 maintainability audit
as small, independently reviewed commits: two dead dev-dependencies, three duplicated helpers,
one mirrored request type, and one declaration manifest built once per route.

**Theme.** The audit's Delete → Dedupe → Decouple order, restricted to items whose unchanged
behaviour has existing witnesses. Every work unit is a refactor with zero expected fixture,
scoreboard or divergence-ledger movement; any movement is a finding that stops the unit. Test-guard
simplification, typed native roles and transition coverage are deliberately later sprints. This
sprint consumes backlog [125](../backlog/125-checker-duplicate-helper-cleanup.md),
[126](../backlog/126-pass-request-and-manifest-ownership.md) and
[129](../backlog/129-reexport-notice-formatter.md).

Success means: the six units are on `main` as separate commits, each independently reviewed
against its exact diff; full `cargo test`, workspace clippy, ordinary conformance at the current
receipt `660/776/660/0`, complete repeated/standalone parity `660/776/660/660`, the official-suite
`run --check` and `differential.py repros --check` all pass unchanged on the final tree.

## Refs re-verified at HEAD (2026-09-09, `81147d86`)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `crates/typokat-driver/Cargo.toml:15` declares `sha2` as a dev-dependency; no file under
  `crates/typokat-driver/` uses it. ⚠ The root `Cargo.toml:100` `sha2` dev-dependency **is** used
  (`tests/lib_es5_readiness.rs:1`, `tests/lib_es2025_full_profile.rs:1`) and must stay.
- ✔ Root `Cargo.toml:106-107` declares a Unix-only `rustix` dev-dependency; no file under `tests/`
  or `src/` references `rustix`. The workspace declaration at `Cargo.toml:49` stays.
- ✔ `construction_alias_surface` (`checker/annotations/composites.rs:225`, checker-visible) and
  `class_interface_alias_surface` (`checker/decls/mod.rs:1528`, private) have identical bodies
  modulo the `AliasId` import path; the private one has a single caller at `decls/mod.rs:1626`.
- ✔ `tuple_rest_container_is_array_like` is defined three times:
  `annotations/composites.rs:414`, `classes/type_syntax.rs:525`, `project_local/lowerer.rs:2528`,
  with callers at `composites.rs:370/372`, `type_syntax.rs:461/464`, `lowerer.rs:1929/2566`.
- ✔ The notice `unsupported-source-reexport-namespace-provenance` is formatted at
  `crates/typokat-frontend/src/frontend.rs:5108` and reconstructed for removal at
  `frontend.rs:4037`; `tests/b15_acyclic_source_reexports_cli.rs` pins the rendered strings.
- ✔ `ProjectLocalUserPassPlan` (`checker/mod.rs:2652`, 14 fields + one cfg-gated
  `inject_provisional_unavailable_write`) and `ProjectLocalUserPassRequest`
  (`project_local/project_local_pass_port.rs:616`, the same 14 fields + `programs`) mirror each
  other; the port destructures one and rebuilds the other.
- ✔ On the ordinary non-empty project route `SourceDeclarationManifest::build` runs over the same
  unit slice three times: binding (`checker/mod.rs:2026`), dependency preparation
  (`project_local_pass_port.rs:160`, followed by inventory authentication), and checking
  (`checker/mod.rs:2716`). ⚠ The fourth site at `checker/mod.rs:2330` is inside the
  `cfg(test)`-only `check_project_programs_inner` route; it is not on the ordinary path and is out
  of scope unless WU6 has to touch it to keep the test route compiling.
- ✔ [ADR-0022](../decisions/0022-use-one-binder-authenticated-declaration-inventory.md) freezes the
  one-pass manifest and exact binder join; WU6 changes *when* the manifests are built, not their
  content or the join.
- ✔ A construction-count witness pattern already exists in
  `crates/typokat-check/src/check/checker/tests/project_local_session.rs:243-250`
  (cfg-test counters read after a real route); WU6 reuses the pattern rather than adding telemetry
  to production types.
- ✔ Current receipts: ordinary conformance `660/776/660/0`, parity `660/776/660/660`
  (`CLAUDE.md`, as-const closure). Official-suite scoreboard and `tooling/differential/scoreboard.txt`
  are the unchanged-behaviour ratchets.

## Work units

### WU1 — delete the two unused dev-dependencies (effort S, backlog 125 step 1)

- **Problem.** `crates/typokat-driver/Cargo.toml:15` (`sha2`) and root `Cargo.toml:107`
  (`rustix`, Unix-only) have no consumer in their owning package's targets.
- **Verify first.** `grep -rn "sha2" crates/typokat-driver/` and `grep -rn "rustix" src tests`
  return nothing but the manifests; `cargo tree -e dev --invert sha2 -p typokat-driver` shows no
  in-package edge. Confirm neither entry activates a feature another dependency relies on.
- **Scope.** Remove exactly those two entries and the now-empty
  `[target.'cfg(unix)'.dev-dependencies]` table. Keep the workspace declarations and the root
  `sha2` dev-dependency.
- **Acceptance / witness.** `cargo test --no-run --all-targets` on Linux; `cargo clippy --all-targets -- -D warnings`;
  `Cargo.lock` diff contains only the removed edges (no version movement).
- **Touch points.** `Cargo.toml`, `crates/typokat-driver/Cargo.toml`, `Cargo.lock`.

### WU2 — one construction alias surface (effort S, backlog 125 step 2)

- **Problem.** One construction-time shadowing rule (a locally unavailable alias must still shadow
  an inherited ready alias) is maintained in two identical bodies.
- **Verify first.** Diff the two bodies (`composites.rs:225-262` vs `decls/mod.rs:1528-1565`);
  list every caller; run `cargo test -p typokat-check alias` and the `tests/cases` fixtures that
  exercise alias heritage (`b43_namespaces_declaration_merging/wu3_cyclic_heritage_recovery.ts`,
  `b44_as_const/source_construction_alias_call.ts`, `b44_as_const/source_construction_copy.ts`) to
  record the pre-change outcome.
- **Scope.** Delete `class_interface_alias_surface`; point its caller at
  `construction_alias_surface`. Keep contextual extraction and heritage traversal separate; no new
  abstraction.
- **Acceptance / witness.** Local ready/unavailable versus inherited aliases, cycles and
  class/interface heritage keep exact outcomes: the fixtures above and the `typokat-check` unit
  tests are unchanged; ordinary conformance at `660/776/660/0`.
- **Touch points.** `crates/typokat-check/src/check/checker/decls/mod.rs`.

### WU3 — one pure tuple-rest predicate (effort S, backlog 125 step 3)

- **Problem.** Three copies of one pure predicate differ only in store access and visited-set
  representation.
- **Verify first.** Diff the three bodies; confirm each is pure over `&Store` and a `TypeId` (no
  diagnostics, no infer declaration handling inside). Record outcomes for
  `tests/cases/m32_signature_shape/{tuple_rest.ts,rest_parameters.ts}`,
  `b14_full_lib_loading/object_finite_tuple_rest_overlap.ts`,
  `b44_as_const/publication_class_opaque_rest_boundary.ts`,
  `b43_namespaces_declaration_merging/wu7_official_named_tuple.ts`.
- **Scope.** One helper taking `&Store` and `TypeId` in the lowest shared owner (the store-side
  helper module is the natural home; do not add it to `Pass`). Infer declaration handling,
  diagnostic ownership and the project-local recipe fallback stay in their callers. Do not merge
  the lowerers.
- **Acceptance / witness.** Readonly wrappers, parameter chains/cycles, absent constraints, arrays
  and tuples keep exact outcomes at the six call sites; the fixtures above and ordinary conformance
  are unchanged.
- **Touch points.** `crates/typokat-check/src/check/checker/{annotations/composites.rs,classes/type_syntax.rs,project_local/lowerer.rs}`
  plus the helper's new home.

### WU4 — one re-export notice formatter (effort S, backlog 129)

- **Problem.** Two byte-identical renderings of one notice form an implicit identity key.
- **Verify first.** Confirm the two format strings at `frontend.rs:4037` and `:5108` agree today;
  run `cargo test --test b15_acyclic_source_reexports_cli` and the frontend unit tests that mention
  the notice.
- **Scope.** One private formatter used at both sites; declaration-wide removal, the shared
  evidence collector, provenance blocking, sorting and every frozen string unchanged. The
  identity-first redesign stays rejected.
- **Acceptance / witness.** The b15 CLI contract and frontend tests pass byte-identically; add one
  control proving that changing the rendering at either site now changes both.
- **Touch points.** `crates/typokat-frontend/src/frontend.rs`.

### WU5 — collapse the mirrored pass request (effort S, backlog 126 M4)

- **Problem.** `ProjectLocalUserPassRequest` and `ProjectLocalUserPassPlan` share fourteen fields;
  every new input is defined and forwarded twice.
- **Verify first.** List every constructor and consumer of both types (`checker/mod.rs`,
  `project_local/entry.rs`, the pass port); locate the test that activates
  `inject_provisional_unavailable_write` and the post-pass inspection tests.
- **Scope.** Make the request contain `programs` plus the existing plan. Keep the ordinary/test
  wrappers; the cfg-gated fault field stays in its current owner. No new public compatibility
  layer; crate boundaries unchanged. Land before WU6.
- **Acceptance / witness.** Identical ordinary behaviour, test fault activation and post-pass
  inspection: the `typokat-check` unit tests that construct the plan/request are unchanged except
  for the field move; ordinary conformance at `660/776/660/0`.
- **Touch points.** `crates/typokat-check/src/check/checker/mod.rs`,
  `crates/typokat-check/src/check/checker/project_local/{project_local_pass_port.rs,entry.rs}`.

### WU6 — build and authenticate the declaration manifest once per route (effort M, backlog 126 M1)

- **Problem.** Three executions of one canonical builder on the ordinary route, with repeated
  inventory authentication, despite the one-manifest/inventory contract of ADR-0022.
- **Verify first.** Trace the ordinary route and confirm the three sites consume the same
  `ProjectProgram` slice and join the same finalized binder; write the construction-count witness
  first (cfg-test counter after the pattern in `tests/project_local_session.rs`) and record the
  current count of 3.
- **Scope.** Retain the binding manifests beside the AST-free `BoundProjectBinder` in a
  request-local execution payload; consume them into inventories once with the canonical
  authentication batch; borrow inventories for dependency preparation and manifests for lexical
  reservations, then move them into checking. No cache, registry, Binder AST lifetime or second
  state machine. The `cfg(test)` route at `checker/mod.rs:2330` is touched only if it must compile.
- **Acceptance / witness.** Exact source/module pairing, fail-closed authentication, canonical
  fragment order, lexical event order and outcomes unchanged; the construction-count witness reads
  1 on the real ordinary route; ordinary conformance `660/776/660/0` **and** complete
  repeated/standalone parity `660/776/660/660` (route lifecycle changes).
- **Touch points.** `crates/typokat-check/src/check/checker/mod.rs`,
  `crates/typokat-check/src/check/checker/project_local/{project_local_pass_port.rs,entry.rs}`,
  `crates/typokat-check/src/check/checker/tests/project_local_session.rs`.

## Out of scope (explicit)

- Test-guard scanner and spelling-assertion replacement — [127](../backlog/127-test-guard-parsers-and-shape-assertions.md).
- Typed native library roles — [128](../backlog/128-typed-native-library-roles.md); do it after
  WU5 so there is one carrier to change.
- Transition-oriented regression coverage — [130](../backlog/130-transition-regression-coverage.md);
  it touches the differential grammar and belongs with the next inference-adjacent sprint, not with
  behaviour-neutral refactors.
- Member-context scope helper and driver error payloads — [131](../backlog/131-conditional-ownership-cleanups.md),
  conditional.
- Class publication preparation (M8) — [119](../backlog/119-class-publication-preparation.md).
- Any semantic change, any fixture or scoreboard movement, any file split "while we are there".

## Decisions

- Behaviour-neutral units only; a unit whose witness moves is stopped and its movement filed, not
  absorbed. No ADR is needed: nothing here constrains future work or rejects a live alternative
  beyond what the audit record already documents.
- Each unit is one commit, reviewed against its exact diff by an independent adversarial reviewer
  hunting for changed outcomes (dev-method §1), before the next unit starts on the same files.

## Sequencing

| Order | Units | Notes |
|---|---|---|
| 1 (parallel) | WU1, WU2, WU3, WU4 | Disjoint files; one Cargo owner serializes the builds. |
| 2 | WU5 | Must land before WU6. |
| 3 | WU6 | Closure gates: full `cargo test`, workspace clippy, ordinary conformance, full parity, official-suite `run --check`, `differential.py repros --check`. |

Gate commands:

```sh
cpu-lease run -n 2 -- cargo test --test conformance conformance -- --exact --nocapture
cpu-lease run -n 2 -- cargo test --test conformance conformance_full_repeated_standalone_parity -- --ignored --exact --nocapture
cpu-lease run -n 2 -- cargo clippy --all-targets -- -D warnings
```

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
