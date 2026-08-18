# Sprint — Canonical declaration inventory (2026-08-18)

**Goal.** Make one binder-authenticated inventory drive type-declaration reservation, lexical
owners, and publication so every bound type group is either published or explicitly unavailable
before semantic checking.

**Theme.** Valid local type declarations currently cross three independent source traversals: the
binder declaration census, the checker's `walk_type_decls`, and a shallow interface-occurrence
census. Their coverage and scope roots can diverge. The resulting binder/publication split is a
reachable panic, not merely missing TypeScript breadth. This sprint removes the parallel semantic
authorities while preserving `DeclId`, `TypeGroupId`, immutable publication, replay order, and the
single published type environment.

This sprint blocks the active [`as const` sprint](sprint-2026-08-17-as-const.md). Its current WIP is
preserved on local branch `wip/b44-as-const` at commit `996eb5a`; resume it only after this sprint
passes its closure gates.

## Refs re-verified at HEAD (2026-08-18, `709cf26`)

- ✔ The exhaustive binder visitor already supplies the shared source declaration census —
  `crates/typokat-binder/src/binder/declaration.rs:122-128`; interfaces enter that same visitor at
  `crates/typokat-binder/src/binder/declaration.rs:602-619`.
- ✔ Each admitted declaration already owns a stable `DeclId`, attached lexical `ScopeId`, and
  optional `TypeGroupId` — `crates/typokat-binder/src/binder/declaration.rs:766-805`.
- ✔ Canonical group order already lives in `TypeGroup::fragments`, whose rows carry `DeclId`, source,
  scope, site, and fragment kind — `crates/typokat-binder/src/binder/declaration.rs:889-919`.
- ⚠ Type reservation still starts a separate checker AST walk at
  `crates/typokat-check/src/check/checker/decls/mod.rs:4659-4666`.
- ⚠ That walk starts at `statement_lexical_root` and drops a block when its binder parent differs
  from the passed parent — `crates/typokat-check/src/check/checker/decls/mod.rs:5623-5633` and
  `crates/typokat-check/src/check/checker/decls/mod.rs:6000-6014`. In a complete-source script
  continuation, `statement_lexical_root` may be the compilation-global scope rather than the syntax
  module — `crates/typokat-binder/src/binder/namespace/lookup.rs:86-103`.
- ⚠ Lexical reservation consumes the shared declaration census, then separately runs a shallow
  interface-only traversal — `crates/typokat-check/src/check/checker/lexical_events.rs:395-452` and
  `crates/typokat-check/src/check/checker/lexical_events.rs:475-535`. It reaches namespaces and
  global blocks but not ordinary blocks or function bodies.
- ✔ The published-registry assertion is a soundness guard and must remain —
  `crates/typokat-check/src/check/checker/decls/resolve.rs:834-845`.
- ⚠ Backlog [`84`](../backlog/84-function-local-interface-panic.md) records the missing
  function-local interface owners, but the newly minimized script-continuation block repro affects
  interface, alias, class, and enum declarations before publication.

## Work units

### WU0a — commit the binding-pattern owner RED specification (effort S)

- **Problem.** The first official-suite failure is a missing lexical owner for a default expression
  inside a binding pattern.
- **Verify first.** Reproduce the panic at `709cf26` for object, parameter, and catch defaults and
  cross-check all three with pinned TypeScript 6.0.3.
- **Scope.** Add one disabled behavior-neutral corpus containing only the binding-pattern owner
  cluster.
- **Acceptance / witness.** The spec commit contains no production change. Forced execution is RED
  for the missing-owner panic, while tsc is clean.
- **Touch points.** `tests/cases/b84_binding_pattern_owners/`, `tests/conformance.rs`, fixture index,
  and this run log.

### WU1 — land binding-pattern lexical reservation completeness (effort S)

- **Problem.** Semantic checking evaluates `AssignmentPattern.right`, but lexical reservation does
  not recursively reserve expressions inside declarator, parameter, and catch binding patterns.
- **Verify first.** Recover only the reviewed lexical-events hunk and focused test from `996eb5a`;
  confirm no `as const` semantic or harness hunk enters the commit.
- **Scope.** Recurse object/array/rest/assignment binding patterns in source order. Reserve the
  assignment right-hand side through the existing initializer and expression reservation paths.
  Do not add computed-key semantics or change unsupported binding-pattern accounting.
- **Acceptance / witness.** Enable `b84_binding_pattern_owners`. Object, parameter, and catch
  defaults no longer panic. Existing unsupported array-pattern output is byte-identical. The
  ordinary conformance gate has no delta beyond the new witnesses. Independently review and commit
  this cluster before starting the declaration-inventory RED cluster.
- **Touch points.** `crates/typokat-check/src/check/checker/lexical_events.rs` and colocated tests.

### WU0b — commit the inventory RED corpus and GREEN parity oracle (effort M)

- **Problem.** After WU1, a block-local type used by an annotation or assertion reaches a binder
  group absent from the published registry. A declaration inventory also risks admitting syntax
  the semantic binder rejects or changing route-specific group order.
- **Verify first.** Reproduce the publication panic after WU1. Record exact binder declaration,
  scope, group, lexical owner, and published group counts on every route before implementation.
- **Scope.** Add a disabled RED corpus for used and unused block/function/control-flow local
  interface, alias, class, and enum declarations. Add GREEN parity controls for sibling shadowing
  versus real merges, named and anonymous default classes, and a named class expression that must
  not become a type fragment. Pin test-support prelude, legacy library-only compile, shared-provider
  user, sparse selected replay, complete-source library+user, and multi-user project routes.
- **Acceptance / witness.** RED rows fail for the measured publication/owner causes. GREEN controls
  retain exact declaration/group/order/owner receipts. Each witness names its future owner: WU3a
  activates interface-owner rows, WU3b activates reservation/publication rows, and WU3c removes the
  legacy parity oracle only after every remaining consumer moves.
- **Touch points.** `tests/cases/b84_declaration_inventory/`, `tests/conformance.rs`, fixture index,
  binder/checker route contract tests, and this run log.

### WU2 — build and prove a canonical bound type-declaration inventory (effort L)

- **Problem.** The binder knows declaration identity and scope, while the checker independently
  rediscovers AST payloads and scope ancestry.
- **Verify first.** Prototype a read-only inventory and compare it against the WU0b oracle without
  switching any production consumer. Stop if it cannot reproduce existing fragment order, owner
  identity, and selected-replay membership.
- **Scope.** Extend the existing source declaration visitor with a type-fragment projection and
  interface header/heritage/member occurrence sites. After binding, join each source fragment to
  its exact `LexicalDeclaration` and build a checker-private dense mapping keyed by `DeclId`. Build
  the manifest and bound inventory exactly once per `(module, program)` and pass them read-only
  through each route. Use `TypeGroup::fragments` as the only fragment-order authority. Add test-only
  comparison with the legacy walker. Do not change ticket reservation or any semantic consumer,
  and do not add a second exact-site index or publication registry.
- **Acceptance / witness.** For every selected source, each admitted binder type fragment has
  exactly one AST payload and owner set; each source type fragment either joins exactly one bound
  declaration or is explicitly rejected. Current supported rows reproduce identical `DeclId`,
  `ScopeId`, `TypeGroupId`, fragment order, and replay ownership. The known local-block misses are
  the only intentional delta.
- **Touch points.** `crates/typokat-binder/src/binder/declaration.rs`, a checker-private inventory
  module under `checker/decls/`, route plumbing, and focused contract tests. Production
  `checker/lexical_events.rs` remains unchanged in this WU.

### ADR — freeze the proven inventory boundary (effort S)

- **Problem.** Consumer cutover must not drift from the behavior-neutral seam WU2 proved.
- **Verify first.** Require WU2's exact route/order/replay receipts.
- **Scope.** Record the accepted identity, ordering, terminality, and single-publication contract as
  an ADR before any semantic consumer moves.
- **Acceptance / witness.** The ADR names `DeclId` and `TypeGroup::fragments` as authorities,
  preserves one `Pass`/published environment, and states the selected-owner terminality rules below.
- **Touch points.** `docs/decisions/`, its index, and this sprint.

### WU3a — cut interface occurrence owners over to the manifest (effort M)

- **Problem.** The shallow interface census misses ordinary blocks and function bodies.
- **Verify first.** Freeze interface owner order for all existing top-level, namespace, and global
  rows, then compare manifest coverage before changing ticket reservation.
- **Scope.** Reserve interface header/heritage/member tickets from the canonical source manifest and
  remove only the shallow interface census.
- **Acceptance / witness.** Backlog-84 function/arrow-local interface rows match tsc; existing
  interface event order remains exact; non-interface reservation and type publication are unchanged.
- **Touch points.** `checker/lexical_events.rs`, manifest plumbing, and WU3a fixtures/tests.

### WU3b — cut type reservation and publication over to the inventory (effort L)

- **Problem.** All four `reserve_type_decls*` routes still depend on the parallel
  `walk_type_decls` traversal.
- **Verify first.** Freeze WU2 receipts for standalone, combined library/user, external-module,
  namespace/global, and selected collision routes. Confirm `TypeDeclTable` remains the only
  construction table and the published type environment remains the only semantic authority.
- **Scope.** Drive all four reservation variants, fill preparation, combined library/user handling,
  and selected replay from the bound inventory. Iterate group fragments in binder order. A
  local/delta group selected for the current construction pass must finish `Ready` or
  `Unavailable`; an unselected frozen-prefix group retains its existing terminal. Missing payload
  for an authenticated selected owner is a route/schedule failure, not `Unavailable`; syntax the
  binder rejected creates no group terminal.
- **Acceptance / witness.** Script-continuation local interface/alias/class controls check to
  completion and mismatches report `TK2322`; unsupported local enum policy is explicit and
  non-panicking. Complete/private routes and selected replay preserve exact terminal and owner sets.
- **Touch points.** `checker/decls/mod.rs`, reservation call sites in `checker/mod.rs` and
  `checker/library_compiler.rs`, and focused route/replay tests.

### WU3c — move final consumers and remove the legacy walker (effort M)

- **Problem.** Class attachment and trusted-prelude validation remain independent semantic
  consumers even after reservation moves.
- **Verify first.** Confirm WU3b has no legacy/new disagreement for every supported declaration.
- **Scope.** Move class binding attachment and trusted-prelude checks to the inventory. Remove
  `walk_type_decls`, its scope mirror, parent-root filter, and the test-only parity adapter only
  after `rg` proves no consumer remains.
- **Acceptance / witness.** Named/anonymous default class controls and named class-expression
  negative controls remain exact; trusted prelude and class allocation order are unchanged; the
  published-registry assertion remains and no reachable test trips it.
- **Touch points.** Final consumers in `checker/mod.rs`, declaration walker removal in
  `checker/decls/mod.rs`, route plumbing, and focused tests.

### WU4 — independent adversarial architecture review (effort M)

- **Problem.** A declaration inventory can silently change merge order, replay selection, or
  publication completeness even when ordinary examples pass.
- **Verify first.** Freeze the WU3 diff and provide the reviewer with WU0, the binding/publication
  invariants, ADR-0020/0021 routes, backlog 84, pinned tsc, and a pre-change binary.
- **Scope.** Attack same-name merges across sources; repeated offsets; nested functions, blocks,
  loops, switch, try/catch, namespaces, and global augmentations; library/user collisions;
  selected replay closure; class/interface composition; recursive interface SCCs; unsupported
  local enums; event order; and missing-inventory recovery. Reject any fallback that adds late
  publication, a second registry, name-based lookup, or assertion removal.
- **Acceptance / witness.** No unresolved HIGH or MEDIUM finding. Every bound type group is proven
  terminal before semantic checking. Current ordinary diagnostics and event order remain exact
  outside admitted local-type fixes.
- **Touch points.** Read-only review plus focused adversarial fixtures/tests.

### WU5 — integration, lifecycle closure, and B44 handoff (effort M)

- **Problem.** The refactor changes declaration discovery and therefore touches every compilation
  route even though its visible feature target is local types.
- **Verify first.** Run the narrow gates first; inspect every delta before broad CPU-leased gates.
- **Scope.** Run full tests, ordinary conformance, complete shared/cold parity, clippy, format, docs,
  official-suite regression check, and the required randomized differential seeds if inference
  output changes. Update architecture/reference text to describe the single inventory. Delete or
  rescope backlog 84 only after its acceptance passes, then archive this sprint.
- **Acceptance / witness.** Zero unclassified regression; exact inventory/order receipts on every
  route; no reachable local-type publication panic; docs lifecycle clean. Rebase or cherry-pick the
  saved B44 WIP only after this sprint is archived, then rerun the entire `as const` integration
  gate on the new architecture.
- **Touch points.** Project gates, `docs/reference/architecture.md`,
  `docs/reference/invariants.md` only if the binding contract changes, backlog 84, a new ADR, and
  sprint/archive indexes.

## Out of scope (explicit)

- Local class body checking remains backlog [`21`](../backlog/21-local-class-checking.md). This
  sprint guarantees discovery/publication integrity, not complete class-body semantics.
- Local enum breadth does not expand automatically. Existing unsupported placements remain typed
  `Unavailable` until separately specified.
- No relation-engine, type-representation, evaluator, cache, or late-publication change.
- No new package/module-resolution breadth, binding-pattern semantics, computed keys, or
  `as const` behavior.
- The saved `as const` implementation is not amended inside this sprint.

## Decisions

- The binder's `DeclId` and `TypeGroup::fragments` are the declaration identity and ordering
  authorities. Source syntax contributes AST payloads and owner sites but does not reconstruct
  scope or merge order.
- The inventory is an input manifest for the existing construction pipeline, not a semantic type
  registry. `TypeDeclTable` construction and one atomically published type environment remain.
- A local/delta group selected for the current construction pass must finish `Ready` or
  `Unavailable`. An unselected frozen-prefix group retains its existing terminal. Missing payload
  for an authenticated selected owner is a route/schedule failure; source syntax rejected by the
  binder creates no group terminal. The published-registry assertion is retained.
- WU2 is the falsifiability gate. If exact order and replay ownership cannot be reproduced without
  a second authority or broad binder rewrite, stop before WU3 and reconsider the narrow root-parity
  alternative.

## Sequencing

1. WU0a lands alone as the binding-pattern RED specification.
2. WU1 fixes, reviews, enables, and commits that cluster.
3. WU0b lands the separate inventory RED corpus and GREEN route/order oracle.
4. WU2 builds the read-only inventory and must pass its equivalence gate before any consumer moves.
5. The ADR freezes the proven seam.
6. WU3a moves interface owners; WU3b moves reservation/publication; WU3c moves final consumers and
   deletes the legacy walker. Each lands separately.
7. WU4 uses an independent reviewer after the full cutover diff freezes.
8. WU5 closes the lifecycle, then hands control back to the `as const` sprint.

## Run log

- **WU0a — complete.** Commit `433f333` added the disabled binding-pattern owner spec. Commit
  `1cc6681` corrected it to isolate the owner defect from existing object-parameter and catch
  accounting.
- **WU1 — complete.** Commit `df1f755` recursively reserves binding-pattern default expressions
  for declarators, parameters, and catches. Independent review passed. The enabled ordinary gate
  passed with receipt `573/689/573/0`; inventory order was
  `0d4555f21f30b471` before WU0b added its GREEN control.
- **WU0b RED evidence.** `b84_interface_occurrence_owners` fails before WU3a because a nested
  interface header has no exact preallocated owner. `b84_declaration_inventory` fails before WU3b
  because selected local groups remain absent or pending at atomic publication. Both corpora are
  disabled. TypeScript 6.0.3 reports exactly their inline marker diagnostics.
- **WU0b GREEN oracles.** Active tests pin the trusted prelude (`18` fragments/groups/owners/
  terminals), legacy library-only (`4` fragments in `3` groups), complete library+user prefix and
  suffix, shared-provider user suffix, sparse selected-replay equality, multi-user module
  ownership, same-scope merge versus sibling shadowing, source-only class syntax, and existing
  named/anonymous default-class project admission. The active namespace/global corpus moves the
  frozen inventory to `574/690`, whole-flat `427/427`, fingerprint `1194c8aeed8cb968`.
  The sparse receipt selects user-origin fragments, including the user `Array` reopening inside a
  frozen-prefix group, and namespace/global interface occurrences retain their exact ticket order.
  The ordinary gate passed with receipt `574/690/574/0` in 143.55 s.

<!-- Append as work proceeds: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-08-18 — The official `as const` gate exposed two independent pre-existing declaration-owner
  failures. The binding-pattern default-expression owner fix was reviewed and preserved with the
  rest of the B44 WIP on `wip/b44-as-const` at `996eb5a`. The second minimized repro binds a local
  type group but omits it from the frozen published registry because the checker declaration walk
  starts from a different script-continuation root and silently drops the block.
- 2026-08-18 — Independent decomposition review split the two RED clusters, made WU2 strictly
  behavior-neutral, divided the consumer cutover into interface-owner/reservation/final-consumer
  commits, narrowed terminality to selected local/delta groups, expanded the route/syntax matrix,
  and moved the ADR before semantic cutover.
