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

### WU0 — commit disabled RED crash and parity specifications (effort M)

- **Problem.** The first official-suite failure is a missing lexical owner for a default expression
  inside a binding pattern. After that is fixed, a block-local type used by an annotation or
  assertion reaches a binder group absent from the published registry.
- **Verify first.** Reproduce both panics at `709cf26`; cross-check admitted rows with pinned
  TypeScript 6.0.3; record the exact binder declaration, scope, group, lexical owner, and published
  group count before writing production code.
- **Scope.** Add behavior-neutral disabled or unit-level RED witnesses for object/parameter/catch
  defaults; script-continuation blocks; function and arrow bodies; external-module controls;
  interface, alias, class, and enum kinds; namespaces/global augmentations; duplicate source
  offsets in continued binders; and selected collision replay.
- **Acceptance / witness.** The spec commit contains no production change. Every intended row is
  RED for the measured cause, not parser failure. The pre-existing declaration/group/replay order
  for currently admitted declarations is captured as an exact comparison oracle.
- **Touch points.** Focused checker/binder test modules, a disabled conformance corpus if needed,
  `tests/conformance.rs`, and this run log.

### WU1 — land binding-pattern lexical reservation completeness (effort S)

- **Problem.** Semantic checking evaluates `AssignmentPattern.right`, but lexical reservation does
  not recursively reserve expressions inside declarator, parameter, and catch binding patterns.
- **Verify first.** Recover only the reviewed lexical-events hunk and focused test from `996eb5a`;
  confirm no `as const` semantic or harness hunk enters the commit.
- **Scope.** Recurse object/array/rest/assignment binding patterns in source order. Reserve the
  assignment right-hand side through the existing initializer and expression reservation paths.
  Do not add computed-key semantics or change unsupported binding-pattern accounting.
- **Acceptance / witness.** Object, parameter, and catch defaults no longer panic. Existing
  unsupported array-pattern output is byte-identical. The ordinary conformance gate has no delta
  beyond the new witnesses.
- **Touch points.** `crates/typokat-check/src/check/checker/lexical_events.rs` and colocated tests.

### WU2 — build and prove a canonical bound type-declaration inventory (effort L)

- **Problem.** The binder knows declaration identity and scope, while the checker independently
  rediscovers AST payloads and scope ancestry.
- **Verify first.** Prototype a read-only inventory and compare it against the WU0 oracle without
  switching any production consumer. Stop if it cannot reproduce existing fragment order, owner
  identity, and selected-replay membership.
- **Scope.** Extend the existing source declaration visitor with a type-fragment projection and
  interface header/heritage/member occurrence sites. After binding, join each source fragment to
  its exact `LexicalDeclaration` and build a checker-private dense mapping keyed by `DeclId`. Use
  `TypeGroup::fragments` as the only fragment-order authority. Do not add a second exact-site index
  or a second publication registry.
- **Acceptance / witness.** For every selected source, each admitted binder type fragment has
  exactly one AST payload and owner set; each source type fragment either joins exactly one bound
  declaration or is explicitly rejected. Current supported rows reproduce identical `DeclId`,
  `ScopeId`, `TypeGroupId`, fragment order, and replay ownership. The known local-block misses are
  the only intentional delta.
- **Touch points.** `crates/typokat-binder/src/binder/declaration.rs`, a checker-private inventory
  module under `checker/decls/`, `checker/lexical_events.rs`, and focused contract tests.

### WU3 — cut reservation and publication over to the inventory (effort L)

- **Problem.** `reserve_type_decls*`, class-owner attachment, and trusted-prelude validation still
  depend on the parallel `walk_type_decls` traversal.
- **Verify first.** Freeze WU2 receipts for standalone, combined library/user, external-module,
  namespace/global, and selected collision routes. Confirm `TypeDeclTable` remains the only
  construction table and the published type environment remains the only semantic authority.
- **Scope.** Drive reservation, fill preparation, class binding attachment, trusted-prelude checks,
  and selected replay from the bound inventory. Iterate group fragments in binder order. Before
  semantic checking, make every bound group terminal either through the existing supported
  construction policy or an explicit `TypeDecl::Unavailable`. Remove `walk_type_decls`, its scope
  mirror, its parent-root filter, and the shallow interface occurrence walker after all consumers
  move.
- **Acceptance / witness.** The script-continuation local interface/alias/class controls check to
  completion; supported mismatches report the expected `TK2322`; unsupported local enum policy is
  explicit and non-panicking. Backlog-84 function/arrow-local interface witnesses match tsc. The
  assertion in `decls/resolve.rs` remains unchanged and reachable tests never trip it.
- **Touch points.** `checker/decls/mod.rs`, its consumers in `checker/mod.rs` and
  `checker/library_compiler.rs`, class-owner attachment, lexical owner lookup, and focused route
  tests.

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
  rescope backlog 84 only after its acceptance passes. Record the accepted design as an ADR if WU2
  validates the proposed inventory boundary, then archive this sprint.
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
- Missing inventory coverage fails closed as an explicit unavailable/incomplete outcome before
  semantic checking. The published-registry assertion is retained.
- WU2 is the falsifiability gate. If exact order and replay ownership cannot be reproduced without
  a second authority or broad binder rewrite, stop before WU3 and reconsider the narrow root-parity
  alternative.

## Sequencing

1. WU0 lands alone as the RED specification and parity oracle.
2. WU1 lands as an independent lexical-reservation fix.
3. WU2 builds the read-only inventory and must pass its equivalence gate before any consumer moves.
4. WU3 performs the single consumer cutover.
5. WU4 uses an independent reviewer after the diff freezes.
6. WU5 closes the lifecycle, then hands control back to the `as const` sprint.

## Run log

<!-- Append as work proceeds: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-08-18 — The official `as const` gate exposed two independent pre-existing declaration-owner
  failures. The binding-pattern default-expression owner fix was reviewed and preserved with the
  rest of the B44 WIP on `wip/b44-as-const` at `996eb5a`. The second minimized repro binds a local
  type group but omits it from the frozen published registry because the checker declaration walk
  starts from a different script-continuation root and silently drops the block.
