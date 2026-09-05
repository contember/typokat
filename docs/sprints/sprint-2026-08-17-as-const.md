# Sprint — AST-directed `as const` contexts (2026-08-17)

**Goal.** Ship the `as const` half of backlog `44`: preserve literal values, mark literal-owned
object properties readonly, infer array literals as readonly tuples, and stop const propagation at
referenced values.

**Why this is a separate sprint.** TypeScript implemented const contexts in
[`microsoft/TypeScript#29510`](https://github.com/microsoft/TypeScript/pull/29510), independently of
the later `satisfies` operator. This is an inference/provenance change, not an assignability operator.
It therefore needs a different corpus, implementation review, and complexity comparison.

This sprint starts only after the ordered `satisfies` sprint. Success means supported literal-owned
shapes match tsc, ordinary literals still widen, referenced values are not deep-frozen, readonly
writes report `TK2540`, and the official `syntax:as-const` gate is audited. When both sprint outcomes
are complete, this sprint closes backlog `44` and criterion `A-satisfies-as-const`.

## Upstream comparison baseline

The upstream const-context PR reports 10 files and `+1,015/-47`:

- Seven test/generated-baseline files account for `+925/-0`.
- Three `src/` files account for `+90/-47`; `checker.ts` is `+81/-47`, diagnostics `+4`, and one
  utility is `+5`.
- The checker change suppresses contextual typing in const contexts, propagates a parent-based const
  predicate through owned literal syntax, preserves non-widening literals, creates readonly tuples
  and object members/index information, and validates allowed operands.

The preceding foundation PR,
[`microsoft/TypeScript#29435`](https://github.com/microsoft/TypeScript/pull/29435), added readonly
array/tuple syntax and relations. Its raw `+65,420/-324` is dominated by generated baselines; its six
`src/` files are `+164/-111`, including checker `+139/-106`. Typokat already has the comparable
foundation: `PropertyType::readonly`, `TypeTag::Readonly`, hash-consed readonly wrappers, and the
mutable-to-readonly relation direction. The fair comparison is therefore primarily #29510, with any
new representation/relation work called out as a missing-foundation delta rather than hidden in the
const-context total.

The original TypeScript PR also covered spread operands, object literals with computed keys and
methods, and invalid-operand `TS1355`. The approved bounded shared-capability scope keeps those
families with their explicit backlog owners instead of conflating them with const-context cost. The
current typokat backlog promises
literal/object/array/nested semantics but assigns some child shapes to backlogs `71`/`75` and keeps
grammar-range diagnostics outside its normal boundary. That is not full upstream-PR parity, and the
outcome must state the difference instead of presenting an apples-to-apples feature-surface claim.

## Refs re-verified at HEAD (2026-08-17, `857ef6f`)

- `infer_assertion` recognizes `as const` only after ordinary inference has widened object members
  and collapsed arrays — `crates/typokat-check/src/check/checker/expr.rs:704-732` and
  `crates/typokat-check/src/check/checker/expr.rs:1076-1122`.
- Required representation already exists in `crates/typokat-types/src/types/repr.rs` and
  `crates/typokat-types/src/types/intern/composites.rs` —
  `crates/typokat-types/src/types/repr.rs:43-45`,
  `crates/typokat-types/src/types/repr.rs:539-590`, and
  `crates/typokat-types/src/types/intern/composites.rs:193-206`.
- The relation already allows mutable array/tuple sources for readonly targets and rejects readonly
  sources for mutable targets — `crates/typokat-relate/src/relate/relation/mod.rs:1549-1565`.
- Member assignment already emits `TK2540` from `PropertyType::readonly`, including union bases —
  `crates/typokat-check/src/check/checker/assignment.rs:444-520`.
- Class-surface initializer inference is a second publication path. Its readonly literal mode resets
  at array children and its assertion helper lowers `const` as an annotation —
  `crates/typokat-check/src/check/checker/classes/initializer.rs:51-66`,
  `crates/typokat-check/src/check/checker/classes/initializer.rs:105-123`, and
  `crates/typokat-check/src/check/checker/classes/initializer.rs:257-266`.
- The official scoreboard currently contains ten `OOS:syntax:as-const` rows. Removing the gate may
  reveal independently unsupported literal-child shapes; those must remain explicit.

## Work units

### WU0 — commit the bounded disabled RED corpus (effort M)

- **Frozen scope.** The user approved bounded shared-capability parity on 2026-08-17: valid
  primitive/enum, object, array, nested, parenthesized, shorthand-property, and reference-boundary
  shapes promised by backlog `44`. Spread/elision remain backlog `71`; computed keys, object methods
  under const context, and invalid-operand `TS1355` remain backlog `75`. This is a measurement
  boundary, not a claim of full #29510 surface parity.
- **Verify first.** Cross-check every admitted row with pinned tsc 6.0.3 and use declaration emit for
  inferred-type witnesses. Force-run the disabled corpus on current typokat and prove intended rows
  RED. Record unsupported identities for every excluded original-PR case.
- **Scope.** Add disabled `tests/cases/b44_as_const/` coverage for primitive/negative/enum literals;
  deep literal-owned objects; readonly fixed tuples; nested arrays/objects; parentheses; shallow
  reference boundaries; `TK2540`; readonly-to-mutable failures; mutable-to-readonly controls;
  ordinary widening; shorthand properties; an outer contextual target that must not leak into the
  const operand; query/order permutations; and instance class-field publication. Add explicit
  incomplete controls for the excluded upstream families without implementing or rebaselining them.
- **Acceptance.** The spec commit is behavior-neutral, tsc-verified, and forced RED for the expected
  causes, with no parser failures or rebaselined controls.
- **Touch points.** `tests/cases/b44_as_const/**`, `tests/conformance.rs`,
  `tests/cases/README.md`, backlog `44` only if scope expands, and this plan. Commit separately.

### WU1 — implement one syntax-directed const-context policy (effort L)

- **Verify first.** Trace one primitive, nested object/array, shallow referenced object, and class
  field through both ordinary checking and class-surface publication. If a shape cannot publish one
  exact complete type on both paths, stop before changing publication boundaries.
- **Scope.** Preserve literal types without widening; set `readonly` on properties created by an
  object literal; infer array literals as fixed tuples and wrap them in the existing readonly type;
  recurse through admitted literal-owned syntax; infer referenced expressions normally before
  placing their types in readonly containing properties; and make parentheses transparent. Apply
  the same policy to class-surface initializer inference. Do not extend the policy to the excluded
  spread/elision/computed/method/invalid surface or add a parallel post-hoc transform.
- **Acceptance.** Enable WU0. Exact/downstream/class rows match tsc; ordinary literals still widen;
  referenced members remain mutable; readonly properties report one `TK2540`; collection relation
  direction and rendering remain correct; and diagnostics are neither dropped nor duplicated.
- **Touch points.** Primarily `crates/typokat-check/src/check/checker/expr.rs` and
  `classes/initializer.rs`. Representation/interner/relation files are test-only unless WU0 proves a
  missing invariant-preserving foundation.

### WU2 — independent adversarial review and inference gates (effort M)

- **Verify first.** Freeze the WU1 diff. Dispatch an independent reviewer that did not implement it,
  with WU0, the selected parity boundary, pinned tsc, the type-store/class-publication invariants,
  and the pre-change binary.
- **Scope.** Hunt accidental deep freezing across identifiers; widening leaks; mutable tuple results;
  readonly identity loss through substitution/union/member access; class publication mismatch;
  bypasses through parentheses/wrappers; contextual typing accidentally retained inside const
  context; unsupported child diagnostics dropped; and cache/order-dependent behavior.
- **Acceptance.** No unresolved HIGH or MEDIUM finding. Run focused B44/surface tests, CPU-leased
  full tests, ordinary exact conformance, clippy, format, and diff checks. Run isolated pre-change
  differential repro and seeds 1/2/3 × 400; classify every delta.

### WU3 — open the official gate, close backlog 44, and record comparison (effort M)

- **Verify first.** Re-fetch the pinned official corpus and enumerate every row whose first bucket
  changes when `syntax:as-const` is removed. Freeze the current ten rows and record expected
  diagnostics, typokat output, remaining incomplete identities, and their owners.
- **Scope.** Remove only the `as-const` regex. Fix admitted failures and keep excluded child shapes
  explicit. Update the syntax-bucket unit test to expect no gate and remove the stale gate from the
  harness README. Run `run --check`, then `run --save` only after identity review. Update coverage,
  divergence text, surface inventory, completion manifest, roadmap counts, and fixture index. After
  both sprint outcomes are verified, delete backlog `44`, complete `A-satisfies-as-const`, stamp the
  closure header, and archive this sprint.
- **Comparison receipt.** Record semantic production files/lines, model touch points, fixture lines,
  generated/scoreboard lines, review findings, and gate cost separately. Compare the semantic slice
  to #29510's `src/` `+90/-47` and checker `+81/-47`. State whether any extra typokat cost comes from
  duplicated class publication, missing readonly foundation, or the selected parity boundary.
- **Acceptance.** Full official-suite check has zero regressions; every moved row is classified;
  manifest and living docs show Track A has only backlog `76` incomplete; workspace and docs gates
  pass after lifecycle closure.
- **Touch points.** `tooling/official-suite/tsofficial.py`, `test_tsofficial.py`, `README.md`, and
  `scoreboard.txt`; coverage/divergence/surface/manifest/backlog docs; this sprint/archive path and
  docs indexes.

## Decisions

- Use AST-directed const context, not a `TypeId` deep transform. Syntax ownership defines the
  reference boundary; a structural transform would freeze too much.
- Reuse readonly property metadata and the readonly collection wrapper. A second readonly model
  would split hashing, substitution, rendering, relation, and write behavior.
- Treat upstream implementation size as source slices, not raw PR totals. Generated baselines and
  parser/emitter integration are reported separately from the type-model change.

## Out of scope (frozen)

- General `as T` / angle-assertion compatibility (backlog `75`).
- Spread/elision gaps owned by backlog `71`; computed keys, object methods under const context, and
  `TS1355` policy owned by backlog `75`. Shorthand properties are in scope.
- Optional/variadic tuple breadth, generic deferred indexed access, project/module work, or a new
  readonly type tag.
- Backlog `76` lazy declaration/value-type resolution.

## Sequencing

1. Start after the `satisfies` sprint has archived.
2. Commit WU0's bounded disabled corpus.
3. Route WU1 to one implementation subagent.
4. Route WU2 to a different independent reviewer.
5. Complete WU3 and close the shared backlog/manifest criterion.

## Run log

### Approved feasibility reset — 2026-09-05

The user approved only spec expansion, bounded literal-model and assignment-flow feasibility,
and an independent GO/STOP decision. Full checker migration and backlog closure require a later
approval. The existing uncommitted WU1 implementation remains preserved in the primary worktree;
experiments use a detached scratch worktree with an authenticated copy of that diff.

**Frozen input.** HEAD `0717d579`; WU1 diff SHA-256
`177d58ca77073296f8ce80bbea0946c66c6503a3ff777766566ee6aec3178b03`.
The leader reran full tests, exact conformance `599/715/599/0`, clippy, formatting, and differential
seeds 1/2/3 × 400. These passed, but third independent review remained FAIL with two soundness and
four fidelity/publication findings. The official-suite gate independently found two surplus
`TK2322` identities in `literalTypesAndTypeAssertions.ts`; the same gate passed with the clean
`10c16781` reference binary. The binding-default regression is pinned by spec commit `0717d579`.

**Re-verified constraints.** Literal interning currently keys only on the value
(`crates/typokat-types/src/types/intern/mod.rs:1218`); inference arguments and extracted candidates
retain bare `TypeId`s (`crates/typokat-check/src/check/infer/mod.rs:105`). A whole-result boolean
cannot retain mixed literal contributions. CFG assignment resolution consumes syntax-derived facts
(`crates/typokat-check/src/check/checker/flowgraph/nodes.rs:175`), while assignment checking schedules
the actual relation later (`crates/typokat-check/src/check/checker/assignment.rs:327`). Class
construction must still obey ADR-0008's no-general-query capability boundary.

The pinned TypeScript 6.0.3 implementation distinguishes fresh/regular literal types, widens union
members individually, and derives assignment flow by reducing the declared target. Its functions
`getFreshTypeOfLiteralType`, `getWidenedLiteralType`, `getTypeAtFlowAssignment`, and
`getAssignmentReducedType` are the comparison authority, not a new local syntax whitelist.
The proposed target-flow contract qualifies the existing assigned-value shorthand in the binding
invariants; this experimental contract is not a silent production-invariant amendment.

| Unit | Territory | Acceptance | Dependency |
|---|---|---|---|
| F0 — independent spec | B44 corpus and this plan | Strict-tsc line/code oracle; current WIP RED; separate spec commit | none |
| F1 — literal-model feasibility | Scratch type-store, relation/query boundary, inference, and the smallest selected checker path | Immutable mode-bearing literals survive real union/member/substitution/inference paths; enum canonical ownership remains unique; rendering and assignability agree; no call-side override repair | F0 |
| F2 — assignment-flow feasibility | Scratch CFG, semantic assignment query, candidate/memo integration | Separate expression/target/recovery results; valid and invalid writes, mutable target, back-edge, and speculative rollback witnesses; no provisional durable answer or duplicate body effects | F1 GO |
| F3 — independent verdict | Read-only review and retained receipts | Explicit GO/STOP per foundation, unresolved owners, migration size, and required next approval | F1/F2 |

**F1 contract.** Prefer immutable widening/non-widening literal identity over a parallel semantic
provenance graph. Decide same-value mixed-mode union normalization and enum member ownership before
editing the model. Existing object-literal excess-check freshness remains a separate concept. Use
real interning, substitution, and inference consumers rather than a standalone toy model. The
prototype may enable only a selected vertical path, but its report must identify every remaining
consumer and must not call a partially migrated checker complete.

**F2 contract.** Keep structural CFG construction. Semantic assigned-expression facts must retain
their occurrence, candidate environment, dependencies, and transitive provisional state. Reads of
unfinished back-edge facts must have an explicit query/cycle policy; clearing a memo after a
diagnostic or inferred type was published is not a solution. Relations stay coordinator-owned and
diagnostics stay single-owner. If this requires the general lazy declaration query from backlog
`76`, stop and request a separate architecture decision.

**Independent remaining obligations.** Inferred binding defaults combine source/default types;
explicitly annotated defaults retain contextual checking. Direct enum authentication remains
distinct from ordinary enum widening. Class member/tuple references must use complete,
publication-safe surfaces without admitting a general projection planner during construction.
These obligations stay in the corpus even if a feasibility unit cannot yet implement them.

**Execution boundary.** One implementation agent and one independent reviewer, sequential units,
serialized CPU-leased Cargo. At most one remediation round per feasibility unit; a second failed
review means STOP, not another unbounded patch cycle. No production cutover, scoreboard changes,
new resolver/declaration capabilities, type erasure, diagnostic suppression, or weakened acceptance.
Preserve the primary WIP and record experimental diffs and receipts before requesting migration
approval. Passing feasibility is not evidence that backlog `44` is complete.

<!-- Append as work proceeds. Graduate durable decisions/findings to ADR/backlog and trim this log
     to pointers. -->

- 2026-08-17 — WU0 authored a disabled seven-file acceptance corpus. Admitted rows cover exact
  primitive/negative/enum values, deep owned object/tuple shapes, parentheses, shorthand members,
  shallow reference boundaries, readonly writes and relation direction, ordinary widening,
  suppression of an outer `satisfies` context, query-order permutations, and instance class-field
  publication. The original-PR exclusions remain explicit: spread/elision use the existing backlog
  `71` incomplete identities; computed keys use the existing backlog `75` identity; object methods
  and invalid const operands reserve `expr-infer/as-const/object-method` and
  `expr-infer/as-const/invalid-operand` for backlog `75` accounting. No `TK1355` is introduced.
- 2026-08-17 — Pinned `tsc 6.0.3 --strict --noEmit --pretty false --lib esnext` matched every
  admitted marker. The excluded ledger is clean in tsc except for the intentionally deferred
  invalid operand's `TS1355`. Declaration emit confirmed the key inferred surfaces: the deep object
  has recursively readonly literal-owned members but a mutable referenced object; arrays are
  readonly fixed tuples; the two outer-context controls remain `{ readonly kind: "left" }` and
  `readonly ["left", 1]`; and both instance fields publish those same surfaces.
- 2026-08-17 — Forced checks with the current pre-implementation `target/debug/typokat` were RED
  for the intended model gaps, not parsing: object/tuple literals widened, readonly writes were
  absent, readonly controls and class fields lacked complete surfaces, and the two new backlog-75
  accounting identities were absent. The top-level primitive/negative/enum file already passes and
  remains a positive control for the existing transparent assertion path. Existing spread,
  elision, and computed-key incomplete identities emitted on their exact marker lines. The corpus
  remains registered `false`. A fresh isolated-target inventory test passed under a two-vCPU lease
  with the unchanged `572/688` inventory receipt; the isolated target avoids the pre-existing stale
  Rust object in the shared target without deleting shared build artifacts.
- 2026-08-18 — WU1–WU3 implementation and review state is preserved on local branch
  `wip/b44-as-const` at `996eb5a`. Removing the official syntax gate exposed a pre-existing
  binder/publication split for block-local type declarations. This sprint pauses until the
  [`canonical declaration inventory sprint`](../archive/sprint-2026-08-18-canonical-declaration-inventory.md)
  ships; the saved B44 diff must then be reapplied and all integration gates rerun.
- 2026-08-21 — The canonical declaration inventory sprint shipped and its final workspace,
  official-suite, and shared/cold parity gates passed. B44 is unblocked; resume from the preserved
  `996eb5a` WIP only after the post-sprint shared Cargo cache setup.
