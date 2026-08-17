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

<!-- Append as work proceeds. Graduate durable decisions/findings to ADR/backlog and trim this log
     to pointers. -->
