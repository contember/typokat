# Sprint — AST-directed `as const` contexts (2026-08-17)

**Goal.** Ship the `as const` half of backlog `44`: preserve literal values, mark literal-owned
object properties readonly, infer array literals as readonly tuples, and stop const propagation at
referenced values.

## Current delivery boundary — user-approved reduction, 2026-09-06

The user explicitly authorized shipping the useful bounded feature rather than reaching complete
TypeScript compatibility in this step, with remaining issues assigned to backlog owners. This
supersedes the expanded completion chain below where it made general compatibility work mandatory.

Keep primitive/enum literals, literal-owned readonly objects/tuples/nesting, parentheses, shorthand
and reference boundaries, contextual suppression and instance class-field publication. Keep ordinary
regression gates, publication safety, single-owner effects and independent review. Do not accept a
new false clean merely by giving it a backlog number.

Stop implementation of C1b/C1c, closed-use certificates, registered callable construction and general
constraint-order migration. Their owner is
[120](../backlog/120-semantic-constraint-construction-order.md); the limited intersection proof and
all experimental failures remain preserved. Landed C0/C1a capabilities stay in place. ADR-0034 remains
proposed. The optional workspace probe build failure has separate maintenance owner
[121](../backlog/121-optional-project-binder-probe-test-build.md).

Before selecting the next candidate, classify the retained generic and core-feature failures against
the pre-feature binary and frozen M1/R1a snapshots. Prefer a new clean worktree reconstruction before
the rejected R1b planner experiments; never reset the preserved WIP or erase experiments in place.
Record exact fixture dispositions before changing corpus registration. Preserve oracle assertions
for deferred cases under their backlog owner. Core literal, arrow/contextual, assignment, default,
enum and class regressions remain required wherever the feature introduces them. General pre-existing
gaps can be deferred explicitly; no blanket scoreboard rebaseline or diagnostic suppression follows.

### Resumed repair baseline and next unit

Leader reconstructed inherited WU1 + frozen M1 + independently reviewed R1a in a new delivery
worktree on `68edd1a2`. All 40 M1/R1a file hashes match their retained manifests; landed binder and
lifecycle-scanner code is unchanged. `cargo check --all-targets` passes. No R1b/R1b2/tracer production
code is in this candidate. It is a repair baseline, not an integration receipt.

Independent source comparison assigns general construction-order gaps to 120, but confirms two
non-deferrable regressions: `recursive_constraint_fallbacks.ts` and
`constraint_instantiation_order_supplied.ts` matched pre-feature diagnostics and now gain `TK2589`.
Repair them without importing the rejected planner. Preserve acyclic exact dependent constraints,
constrained candidate modes and the corrected reverse-dependent control. Other introduced depth
errors also require explicit disposition; old missing union errors are not permission for new losses.

R2a starts now, independently of construction order: one reserved arrow producer, default-expression
tickets exactly once, explicit return annotation separate from return-expression context, actual
source return publication, and ordinary outer callback obligation ownership. Keep all assertions in
`contextual_arrow_checks.ts` and the retained `contextual_return_contexts.ts`. Ready contextual
unannotated defaults use the same default owner; no separate post-body checker. The sole previously
approved old-test amendment changes the ordinary object callback parameter from `any` to `number`
and renames that assertion while retaining its actual `string` return, one error and no incompletes.

The old mixed bare-return/fallthrough assertions moved intact into
`b46_return_paths/contextual_mixed_returns.ts`; they are not R2a acceptance under the reduced scope.
General return-contribution aggregation also remains with 46/111. Empty/bare-only source returns,
value-returning contextual `void`, explicit annotation rejection, nested callback return context and
single-return const identity remain mandatory. No diagnostic truncation, fabricated contextual
return type, context removal, duplicate parameter/default/body walk or whole-Pass snapshot is allowed.

R2a changes only arrow/contextual production and its directly owned body/default state in the checker,
plus focused tests. It does not implement row correlation, generic fixing policy, assignment-flow
availability, class demand or a new relation engine. Prove candidate rollback and selected/rejected
owner counts. Then independently review against strict tsc and run the contextual differential gate.
R1c, minimal assignment/default/class repairs and the newly exposed dependent-rest FPs follow as
separate clusters. All four ordinary official regression files remain blockers; eight new rest-body
property errors cannot be waived by deferring the broader rest feature.

Independent triage also assigns pre-existing semantic assignment/loop-product gaps to
[51](../backlog/51-narrowing-tail.md), mixed return aggregation to
[46](../backlog/46-return-path-analysis.md) with 111, and broader candidate fixing/compound return
roles to [122](../backlog/122-signature-candidate-fixing-policy.md). Those owners retain the exact
witnesses and limits. Apart from the explicit mixed-return split, corpus registration and all
mixed-fixture assertions remain untouched pending the respective minimal repair specs.

### R2a review repair — contextual literal members

Independent review rejected the first reduced R2a candidate: its return helper preserves only
all-literal contexts and newly widens `"a"` under `"a" | undefined` to `string`. Boolean context
also loses exact `true` source returns through `satisfies`. Both expression and single-return block
bodies regress against frozen R1a and strict tsc 6.0.3. This is required single-return context
preservation, not deferred return-path analysis or generic construction.

Leader acceptance is `contextual_return_literal_members.ts`: preserve the actual matching source
literal for mixed literal/undefined/null/other-primitive unions and boolean contexts, through both
declaration and `satisfies` routes. Ordinary broad string/number contexts must still widen; invalid
literal targets still reject. Do not substitute the contextual union for the source return or keep
all literals unconditionally. Reuse the existing source-aware contextual literal rule; preserve the
reserved producer, lexical owner tests and candidate rollback. Freeze the repaired delta for a
second independent review and rerun the focused, contextual differential and ordinary gates.

The second review confirms the original 26 regressions fixed, but rejects the repair for a new
shared-caller false clean. Object/array/tuple producers now preserve mixed-union literals without
regularizing their widening mode, so a copied property/element accepts an invalid later write.
Leader acceptance `contextual_container_literal_modes.ts` pins downstream-copy and direct-write
checks across those routes and nested object returns, both union orders, optional/boolean/numeric
members, valid same-value writes and broad same-kind controls. Keep syntax-directed ownership and
reference boundaries: regularize the admitted source contribution at its producer, never recursively
rewrite a referenced container. The source-aware decision and publication mode must agree in every
affected shared caller. This repair stays in the same contextual-literal cluster; no new inference,
source-construction or return-path dependency is authorized.

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

### Autonomous completion authorization — 2026-09-05

The user authorized the leader to make further sprint decisions without asking and finish the
whole sprint. This supersedes the feasibility-only approval gates and file fences below; those
entries remain historical evidence, not current execution limits. Soundness, immutable publication,
single-owner effects, independent review, and all closure gates remain mandatory. The frozen
feature exclusions still apply: this does not silently absorb general lazy declaration resolution,
module breadth, or unsupported const operands.

The second F1 review confirmed that the wrapper representation was not disproved. Its producer
boundary was too narrow: namespace and readonly class producers publish ordinary and annotated
exact literals with the same ID, while shared declaration and inference widening broadens both.
The leader therefore chooses a coherent producer/consumer migration, not a compatibility shim or
another literal-origin boolean. The committed producer witness extends the existing B44 matrix.

| Sequential unit | Territory | Contract and gate |
|---|---|---|
| M1 — complete immutable literal modes | Experimental type-store, query/relation, checker producers, inference, display, and their tests | Migrate ordinary producers including namespace/readonly class fields; preserve exact annotation/const types; remove post-fix inference repairs; independently verify model, coordinator, ordinary controls, and B44 literal transport |
| M2 — semantic assignment flow | Checker CFG, assignment facts, declaration/contextual memo boundaries, and tests | Separate assignment result from declared-target reduction and invalid-write recovery; prove back-edge/candidate-local answers cannot become durable |
| M3 — remaining B44 fidelity | Binding defaults, enum authentication, safe published class reference projection, and tests | Close the retained default/enum/class regressions without new general declaration or construction-query capabilities |
| M4 — integration and closure | Leader-owned registration, official harness, docs, manifest, and scoreboard | Full-diff adversarial review; unchanged-tree workspace/conformance/lint/format, differential and official gates; audit each transition before lifecycle closure |

One implementation owner works in the existing detached experiment, with sequential Cargo under
CPU lease. The primary WU1 diff remains preserved until verified integration. Read-only reviews and
later-unit design may run separately, but cannot mutate the implementation territory. A failed
review causes root-cause classification and a new bounded remediation unit, not scope cuts or
repeated blind patches. The leader resolves new design boundaries under this authorization and
records significant choices in an ADR before production integration. No push, publish, or deploy.

- M3 spec extension: inferred default contextual callbacks, declaration-kind widening, null versus
  undefined, and annotation authority; ready enum keys and child-effect cardinality; reversed class
  order, reference mutability, scalar negative controls, and missing/private members. Strict tsc
  confirms these witnesses. A missing identifier enum key reports `TS7015`, not `TS2339`; keep it
  under the existing `expr-infer/element-access/implicit-any-index` owner rather than add an
  unrelated no-implicit-any diagnostic to this sprint.
- 2026-09-06 — M2 acceptance now forces a smaller loop closure than the declared union, including
  a later local initializer, an exact call, a cyclic RHS, and a failing call child whose effect
  must appear once. Additional transfer controls cover fresh booleans, whole-source union coverage,
  mutable declared union members, and nested assignment-result separation. Strict tsc matches all
  ten markers; the authenticated pre-change checker drops all four loop-copy errors. A bounded
  source-owned local initializer demand is the selected availability direction, subject to the
  independent architecture step review; it is not a general backlog-76 cutover.
- M2 step review corrected two proposed contracts: the expression result retains actual contextual
  typing, and loop discharge follows TypeScript's query-local incomplete-flow policy rather than a
  simultaneous equation fixed point. New source witnesses pin both corrections. The adopted
  literal-mode and source-product decisions are
  [ADR-0031](../decisions/0031-preserve-literal-widening-in-immutable-type-identity.md) and
  [ADR-0032](../decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md).
- M1 independent review returned FAIL: contextual arrows skipped parameter defaults and treated
  contextual returns as annotations; merged inference candidates leaked widening modes; recursive
  constraint fallbacks expanded on every stabilization step. The leader's full suite additionally
  exposed enum identity transport and existing contextual callback ownership controls. These are
  acceptance blockers, not permission to weaken the older tests. Remediation starts with terminal
  candidate modes (R1a), then recursive fallback identity (R1b), then contextual arrow ownership and
  dependent rest parameters. Return-contribution ordering is an older as-const gap now pinned as an
  explicit in-sprint obligation rather than left as an unrecorded review caveat.
- The M1 official gate failed four files. Two mapped-string/numeric deltas share the M2 target-flow
  cause, not a string-intrinsic relation bug. The known inferred-default delta remains M3. Newly
  contextualized rest-tuple arrows expose missing positional correlation and generic rest
  projection; their true/false branch controls are now separately tsc-verified. The scoreboard is
  unchanged. Root source guards also require a new-tag audit and a clean-worktree fix: `examples/`
  exists locally but is untracked, while the production lifecycle scanner currently requires it.
- R1a has narrow independent GO; the leader repeated all 68 inference tests successfully. This
  accepts the prepared multi-candidate result correction only, not M1 integration. R1b addresses
  recursive fallback identity next; enum direct-return transport remains a separate inference
  policy dependency.
- R2 decisions: preserve the ordinary object callback's outer-obligation owner/span. Replace only
  the old callback source-parameter expectation `any` with `number` and rename that unit test;
  independent tsc source-type inspection proves `(value: number) => string`. Keep its actual
  `string` return, one-error assertion, and no-incomplete assertion. R2a also owns mixed bare-return
  and fallthrough source types; they cannot be replaced by a contextual return annotation.
- Contextual tuple-row correlation is disabled when a participant is written anywhere in the
  callback, including later or captured writes; shadowed and unrelated writes do not disable it.
  Generic source signatures must retain their binders and positional indexed types. A leader tsc
  declaration probe confirms that a `satisfies` source remains generic and accepts valid explicit
  type arguments, so a broad non-generic source approximation is not accepted. Any required shared
  projection extension is limited to closed finite tuple-row constraints and static positions,
  not general generic indexed-access breadth. New source tests pin both boundaries.
- The complete frozen-M1 differential replay classified all 111 divergent programs: 110 are
  contextual object-arrow return relocation (R2), and one is union-member display order after
  introducing the widening wrapper. No line/code/cardinality/exit/incomplete delta occurred in
  that generated inventory. This classification does not override the separately reproduced
  default/empty-return false negatives or approve the changed route. No scoreboard or allowlist
  changed; final acceptance still requires rerunning the repaired tree.
- R1b's first review found a cyclic-seed ordering defect: tuple constraints traverse their elements
  in instantiation order, while anonymous object constraints do not eagerly traverse properties in
  that order. The separately committed `constraint_instantiation_order` fixtures cover both tuple
  orders, the object counterexample, supplied candidates, and the partial constructor fixer.
  R1b2 is the active remediation; no integration is approved on the first R1b result.
- Leader oracle consolidation at `3ca4f7d6`: all 45 B44 source files match TypeScript 6.0.3 at the
  expected 162 diagnostic line/code sites, including the declared `TS1355`/`TS7015` counterparts of
  incomplete markers. This verifies the acceptance corpus, not candidate conformance. The new
  generic-return fixture also pins direct, union, nested-object, and tuple return policy for regular
  and widening primitive/enum contributions before R1c implementation.
- R2b independent design review gives conditional GO with mandatory corrected seams. The adopted
  symbolic-signature, source-write census, bound-view, row-relation, and invocation ownership
  contracts are [ADR-0033](../decisions/0033-separate-contextual-row-views-from-symbolic-signatures.md).
  Explicit literal payload specialization and aggregate-rest arity controls extend the source spec.
- Implementation order after R1b2 review: R2a's single arrow/default/actual-return producer first,
  then signature-aware R1c candidate policy and contextual fixing through that producer, then R2b.
  Return contributions are retained until unioning before endpoint widening as part of R2a's
  return publication owner. This avoids adding two separate contextual arrow pipelines. M2/M3 and
  all full-diff/differential/official closure gates remain pending.
- R1b2 review returned FAIL despite its ten-probe screen and 81 inference tests passing. Scheduling
  every retained alias argument visits erased arguments; canonical union/intersection storage and
  structural interface lowering also lose mapper-order distinctions. The new semantic-identity and
  erased-supplied fixtures pair valid and invalid leaves, including both fixers, against tsc.
  Stop local planner remediation here. A read-only architecture reset must establish the exact
  information loss and its authoritative immutable owner before more implementation. In particular,
  do not assume written union order equals TypeScript's lazy type-identity allocation order.
  No R1b/R1b2 acceptance or integration is implied by terminating without `TK2589`.
- Architecture reset decision: use the existing immutable declared-expression ownership route for
  an intersection-only tracer, not a second general type graph. The two read-only analyses agree
  that the current descriptor input loses information. They do not establish a union producer:
  normal TypeScript sorts unions by lazily allocated semantic type identities, shares their order,
  and differs after forward signature demand. New construction/forward-demand source controls pin
  that gate. Do not implement per-occurrence written union order as if it solved this requirement.

#### R1b immutable-owner tracer contract

This is an isolated architecture proof, not R1b completion or permission for another inference-only
dependency heuristic. All existing acceptance fixtures remain unchanged.

1. Extend the existing acyclic `DeclaredRecipeNode` hierarchy with an immutable ordered intersection
   expression. Complete child handles participate in hash identity, free-parameter summaries,
   reference census, substitution, and freeze/fork. Do not change canonical structural intersection
   algebra, add AST/origin flags, or create cyclic recipes.
2. With held named binder IDs B/C, construct two retained intersection constraints in opposite
   orders. Require distinct retained and owning generic-signature IDs, equal canonical structural
   projections, opposite ordered mapper requests, and unchanged original nodes after substitution.
3. The existing query/materialization owner supplies the retained instantiation operation and
   structural projection. Do not inspect flattened `TypeTag` members in inference to reconstruct
   order or add a private evaluator. Relation still uses coordinator publication/poison/overlay
   preflight and must give equal answers in both cold/warm relation orders.
4. Transport the retained expression through outer substitution of a nested generic signature and
   one real project-local descriptor publication path. Keep inner binders shielded. This producer
   proof is limited to that explicit occurrence; no wholesale annotation or library cutover yet.
5. Execute reference/freeze/fork witnesses, missing/poisoned publication and genuine exhaustion
   rollback, and an adapter test through the same full/partial seed owner. Bare ordinary constraints
   and the R1a/reverse-dependent controls must remain valid inputs, not acquire invented provenance.
6. Before wider producer migration, independently accept erased-versus-retained alias forms and a
   deterministic union-construction policy including the committed forward-demand control. If that
   policy needs a larger availability change, decide it from evidence rather than smuggle it into
   the tracer. Neither broader backlog 76 nor TypeScript's whole lazy allocation system is granted
   by this tracer contract.

#### Union producer gate — blocked after independent scheduling probes

The independent continuation checked 46 fresh source programs through both compiler API and the
normal TypeScript 6.0.3 CLI; all outputs agreed. First textual call, first value observation, first
signature observation plus parameter ranks, and source-ticket order are each insufficient. Callable
relations, type-only consumers, demanded versus deferred function-expression bodies, and composite
union-constituent creation affect the result. The current early signature publication occurs before
those observations and carries no deterministic semantic construction trace.

The missing capability is a source-owned construction authority separating value acquisition,
signature resolution, header checking, and demanded/deferred work. It must establish relevant
constituent-creation identity before publishing usable callable semantics. A query-chosen once-cell
would still make meaning query-order-dependent; a second semantic prepass would duplicate work.
Neither is an accepted workaround. This does not establish that copying TypeScript's entire lazy
allocation system or absorbing backlog 76 is necessary or justified.

Finish and independently review the already authorized intersection tracer. Do not expand it into
a new source scheduler, waive the union witnesses, or claim R1b/B44 completion from its local gates.
The producer boundary requires a separate architecture unit with an executable ownership and
publication contract before further cutover. No production integration or closure gates are waived.

#### Intersection tracer review and bounded remediation

Independent review returned FAIL despite passing 152 type, 161 query, 82 inference, and 43 publisher
tests. Four concrete failures remain: an unavailable class hidden inside an admitted declared-array
operand bypasses identity preflight; finite same-recipe/different-mapper nesting is called a cycle;
shared intersection DAGs expand as trees; and the product-signature seam admits open composite
operands that the adapter cannot consume. The last issue causes real valid product-backed calls to
become incomplete, not just an untested hypothetical extension.

The active remediation is the complete immutable-application traversal cluster:

- `R = Intersection(T, B)`, `inner = R[T := C]`, `outer = R[T := inner]` must materialize through
  both raw and derived APIs and public demand to the same canonical `B & C`. Preserve both owners
  and their mapper identities; genuine active re-entry and malformed recipes remain non-clean.
- For `Intersection(never, Declared(Array(Type(unpublished-class))))`, cold and warm identity,
  assignability in both directions, and demand must all exhaust before success. Cover the complete
  admitted recipe-child closure, including mapper substitutions, not only direct intersection
  children. Repeat with a poisoned publication and after an earlier failed demand.
- For `D0 = Intersection(B, C)` and `Dn = Intersection(Dn-1, Dn-1)`, completed projection work is
  shared by complete immutable application. Raw/derived materialization and public demand must not
  expand the linear DAG exponentially. Assert work/visit counts or a bounded deep witness, not a
  wall-clock threshold. Preserve taint and derivation identity; incomplete results are not completed
  memo entries. The retained ordered operation remains separate from canonical projection.

Do not modify the overbroad publisher seam during that cluster. Its separate admission repair must
preserve the real source-product witness and all valid open object/array/tuple/function controls,
without introducing a fixture hook, permissive recovery, or claiming broader form coverage. Both
clusters need review before any tracer acceptance; neither changes the union-producer STOP.

T1's three original traversal repros now pass in the isolated candidate. A new native-handoff
control remained RED because its setup offered Boolean identity but neither ready nor demandable
Object identity. Independent paired direct/retained probes agree: that setup returns structural
`No`; making both identities demandable requests `[6, 4]`; supplying ready Object requests `[4]`.
The decisive handoff code is unchanged from pre-T1. Approve only adding ready Object context to
the new test; preserve its exact `[4]`, repeated-attempt, and five-cache rollback assertions.
This corrects a test premise, not production behavior. T1 still requires independent review, and
the separate T2 publisher regression remains unfixed.

T1 closure: independent production review found no new defects and gave narrow GO. The authorized
four-line Object-context amendment preserves every original assertion. Leader verification matched
all 1,997 final file hashes and passed the exact native control, 167 query tests, 156 type tests,
and 83 inference tests (existing measurement tests remain ignored). T1 is accepted only as the
bounded traversal remediation in the isolated candidate. Advance to the already specified T2
admission repair; no whole-tracer acceptance, production integration, or union-gate clearance follows.

T2 admission decision: select only whole, finite product intersection trees whose leaves are
resolved parameters owned by the immediate generic signature or existing modeled ground
intrinsic/literal terminals. Parentheses are already transparent in the product; nested intersection
structure must remain retained. Exclude outer parameters, aliases/applications, unions, operators,
and all compound leaves from this minimum. Static admission is not a readiness certificate.

Choose the whole-root producer before lowering any leaf. Non-selected roots use the existing
canonical producer exactly once. Never partially retain a mixed root, select by fixture/name/flag,
retry canonically after an error, or put a compatibility fallback in inference. Validate the selected
plan's resulting leaf identities against the signature-local binder set; a mismatch is a construction
failure. Selection is query-free and uses existing product reference resolution, not spelling.

Keep the unrestricted model constructor and its closed-object outer-substitution witness unchanged.
The bounded product subset is stable under outer signature substitution because all of its open
leaves are shielded local binders. This does not promise retained-adapter execution after an arbitrary
outer parameter becomes an open composite; such a producer occurrence is not selected here.

T2 acceptance adds real product/session controls for open object/array/tuple/function operands,
nested and parenthesized intersections, unprotected outer-parameter roots, and mixed roots. They
must remain complete through their original producer, with no partial retained descriptor. Preserve
the real full/partial B/C-order witness and add outer substitution of an admitted published generic
method with local binders. Verify opposite retained orders, selected reductions, original model
transport, and T1's complete traversal witnesses. No broader alias/union/source cutover is approved.

T2 independent review found the original over-admission repaired and 31 source controls matching
tsc diagnostic-code multisets, but returned FAIL on new transient space amplification. A validated
shared product DAG becomes an owned plan tree before lowering. The old producer repeated CPU work;
the simultaneous exponential plan storage is new and must not be accepted.

T2a acceptance: construct a validated product with `D0 = unknown & number` and
`Dn = Intersection(Dn-1, Dn-1)`. Planning storage and selected construction visits must be bounded
by unique admitted product nodes and ordered edges, not unfolded tree occurrences. Keep repeated
child edges and their order while sharing completed nodes; cloning cached subtrees is insufficient.
Use deterministic counters or plan-size assertions, not memory or wall-clock thresholds. Prove
the same property for shared local-parameter leaves and preserve both retained B/C orders.

Keep planning and construction state local to the complete constraint's immediate-signature
resolution context. An active node is not a completed node. A genuine selected cycle must fail;
a shared completed child must not. Reusing a product key under a different signature/context must
not reuse another scope's admission or lowered binder identity. Whole-root exclusion must still
lower canonically once with zero partially retained children, even after a large eligible sibling.
Selected construction errors remain errors; no memoized error or canonical retry is permitted.
This repairs only T2's transient producer plan and construction, without persistent type metadata,
grammar expansion, shared query/inference changes, or reopening T1 and the union-producer gate.

T2/T2a closure: independent review found no new defects. Leader reran the six T2a counter/identity
tests, all 53 publisher tests, and all-target Clippy successfully; all 1,997 source hashes match
the frozen candidate. Independent validation also passed 167 query, 83 inference, and 156 type
tests and all 31 source controls. Accepted T1 plus T2/T2a closes the four original tracer findings
and the additional transient-plan finding. The intersection-only ownership experiment is accepted
within its recorded contract, not as R1b/M1/B44 completion. Its implementation still depends on the
unaccepted isolated migration; this proof is not a standalone production integration receipt.
Keep the frozen candidate intact while settling the narrower source-construction spec/ADR.

#### Source-construction blueprint — not approved for implementation

A separate read-only blueprint proposes immutable registered callable references, source-owned
header/signature/return/body completion states, ordered `NeedsSource` handoff after pure-query
rollback, and relative constituent creation identities frozen before callable publication. This
is not a second checker or a query-chosen once-cell. Expression-arrow return demand additionally
needs owned checked-expression receipts; block return demand cannot run the whole body early.

Independent review gives GO only for a narrower spec/ADR, not the blueprint's current profile.
Even a nullary declared-return signature with no extra source producer can construct shared types
in different orders: for `A extends C[] | B[]` and return `[B[], C[], A, B, C]`, a forward call and
a forward callable assignment produce opposite constituent orders and rejected leaves. Both
normal CLI results match diagnostic-first TypeScript API checks. Therefore the first proof must
exclude return construction that affects relevant constituent identity, or explicitly model
demand-lazy signature fields. “No additional source demand” is not a sufficient admission rule.

Registered-reference safety, immutable publication snapshots, exact source permits, query-only
retry, effect ownership, resource limits, and static admission require an ADR before source cutover.
No class/library/overload/multi-file expansion follows from the blueprint. Keep this architecture
gate separate from T1/T2 and do not treat the written plan as a passing implementation experiment.

The narrower profile received independent GO for spec/ADR only: direct own-parameter/ground returns
plus leaf/selector constraints that cannot demand a selector through another bound. Its proposed
construction/publication contract and unresolved real-route admission gate are
[ADR-0034](../decisions/0034-own-callable-construction-before-publishing-signatures.md).
The `source_construction_*` fixtures pin twelve admitted source observations plus the bound-chain
frontier. That frontier remains required for eventual sprint closure but is outside the first proof.
No producer-only classifier may silently extend the profile to unsupported pre-completion consumers.

#### Closed-use capability — selected next dependency

Read-only feasibility establishes a route through the existing declaration visitor and finalized
binder join. It does not grant source-construction cutover. Capture every occurrence and syntax
role during that visitor, authenticate real scopes/slots after binder normalization, and build
direct top-level immutable const-copy edges. Any outside use disqualifies the complete reachable
producer before registration; no consumer-time fallback is needed. Final library group **and
declaration** mappings, gateway storage lists, module edges, and script global-object acquisition
belong to the closure proof. Automatic raw global-property transport must cause no source births.

Implement sequentially, with one active code unit and independent review per unit:

| Unit | Territory | Done-check |
|---|---|---|
| C0 structural facts | binder declaration visitor and a source-reference fact module | one traversal; exact occurrences, roles, regions and honest coverage; no behavior cutover |
| C1 authenticated census | binder finalization and reference resolution | scalar/batch parity, scope holes, final identities, bounded joins |
| C2 closed-use certificate | checker inventory and product/global boundary receipts | all twelve positives selected, every outside acquisition rejects the complete producer before construction |
| C3 registered reference protocol | immutable model and shared query/publication seams | ADR-0034 execution contract, snapshots, safety, permits and rollback |
| C4 source construction | existing source producers and shared union/array operation owners | real CheckerSession source fixtures and once-only header/bound effects |

Only C0 is authorized for implementation now. C1–C4 require their own leader-authored acceptance
spec and step review where the interfaces change. Use a clean detached worktree based on current
committed source for C0; do not attach this independent structural capability to the unaccepted
literal-model migration. Preserve the accepted frozen intersection candidate and primary WIP.

##### C0 acceptance contract

Add immutable, AST-free source-use facts to `SourceDeclarationManifest` from its existing visitor.
Record dense occurrence/operation/region identities, exact spans, reference lookup space and edge
role, structural read/write kind, owning callable/class/namespace/header region, scope anchor and
direct-program-body placement. Retain call optionality, explicit type arguments, argument count,
annotation/result-context role, and exact single-binding unannotated const-copy syntax. Parentheses
are transparent through retained edges, not repeated ancestor/span searches. No binder lookup,
semantic type, RHS evaluation, effect, construction rank, or certificate is produced by C0.

The leader-owned `spec/fixtures/source-reference-manifest.ts` marks every `producer` occurrence
that is a local reference candidate. Assert those exact spans and lookup/write categories; declaration
names, labels, static property keys, exported spellings and import qualifiers must not become false
local reads. Keep separate operation facts for import/qualified/global/dynamic channels. For marked
edge roles, retain either the exact role or a complete constant-work parent-edge path that proves it;
do not rediscover syntax after traversal. The file is structural parser input, not a tsc-clean program.
The TSX fixture distinguishes component roots, member names, intrinsic tags and expression children.

Cover assignments and updates, destructuring defaults, explicit call type arguments, type queries,
computed keys/spreads, decorators/static blocks, dead/captured regions, ambient/module/export syntax,
and scopes not currently modeled by the binder. Structural coverage does not authenticate a scope;
retain anchors/uncertainty for C1 rather than assigning the enclosing scope. Test shadowed references
as separate regions without pretending C0 knows their resolved identities.

Facts must be emitted independently of the optional global-binding census and its early returns.
Each generated AST visit and each structural edge is processed a bounded number of times. Existing
manifest-build/node counters and long parenthesis/reference/region controls must exclude a second
walk or quadratic ancestor reconstruction. Preserve all old declaration/type-fragment/global-census
results. `empty_for_program` must mark reference coverage unavailable, not vacuously complete.
An interrupted/invalid structural projection cannot claim complete coverage. Be explicit about
parser diagnostics not present in the Program input; C0 must not invent parser-recovery certainty.

Thread no new checker path, producer selection, resolver, library schema, or public unsupported
notice in C0. Freeze the fact API only after independent review. C1 later needs binder-owned batched
resolution or an honest non-linear bound; scalar lookup is not assumed constant time.

C0 independent review returned FAIL on three immediate-edge classification gaps despite all 11
new tests and 142 binder tests passing. The source-use projection otherwise preserves its single
visitor and owned-data boundary. C0a repairs only these structural classifications:

- `satisfies` assignment targets propagate `Write` for assignment/destructuring/loop targets and
  `ReadWrite` for update/compound assignment. Member receivers, computed keys and RHS remain reads.
  `satisfies` remains opaque to const-copy/result-context transparency; do not infer this from its
  target propagation. Preserve ordinary assertion/non-null controls.
- Computed keys of both `TSPropertySignature` and `TSMethodSignature` are value-expression edges
  in aliases, interfaces, variable annotations, type arguments and return annotations. Noncomputed
  names are not references. Nested key-expression type arguments and property annotations still use
  type space; a computed key must not change the whole signature's lookup space.
- Ordinary `for` initializers, tests, updates and bodies retain exact optional-slot roles. The three
  padded `source-reference-for-*` fixtures have equal parent/reference spans but different slots;
  their manifests must differ in those roles, including a distinct body edge. An omitted slot must
  not shift another child into that role. No semantic scope assignment or source-text scan is needed.

The separately committed `source-reference-write-wrappers.ts` and `source-reference-type-keys.ts`
fixtures use unrelated names. Assert every reference row's lookup/write classification, not only
the old `producer` filter, and check the unchanged exact TS/TSX marker corpus. Reproduce the three
REDs before implementation, then rerun binder tests, feature checks, and consumer compilation.
The fact API remains unfrozen until independent re-review; no C1/certificate work starts yet.

C0/C0a received independent GO; leader binder and ordinary conformance gates pass (`583/699/583/0`).
The default complete test run fails only the pre-existing lifecycle scanner's unconditional
`examples/` directory read in a clean worktree. No example is tracked. Close this test-tooling
blocker separately before C0 integration: only absent optional `examples/` is skipped; present
examples remain recursively scanned/sorted, missing required roots still fail, and wrong-type or
broken optional roots are errors. The new lifecycle inventory tests pin those cases without changing
the lifecycle prohibition assertions. Do not create an untracked placeholder directory to pass.

The broader `cargo test --workspace` additionally enables an optional binder-probe test path whose
`BoundProjectBinder.normalized` field no longer exists. A clean pre-C0 baseline reproduces that
compilation error. It is not part of the default-member gate or a C0 regression; keep its failure
record separate rather than changing unrelated projection semantics during the scanner repair.

C0 integration closure: C0a independent review found no remaining issues; the separate optional-root
scanner repair also received independent GO. Leader reran `cargo test --no-fail-fast`, exact ordinary
conformance (`583/699/583/0`), all-target Clippy, and full formatting on their unchanged combined
candidate; all pass. After transferring only the four reviewed binder files and scanner helper to
the primary tree, leader repeated 148 binder unit tests plus its integration test, all 11 lifecycle
tests, all-target Clippy and formatting successfully. Integrated files match the tested candidate;
the original 14-file WU1 diff retains hash
`177d58ca77073296f8ce80bbea0946c66c6503a3ff777766566ee6aec3178b03`.
The structural fact API is frozen. C1/C2 and ADR-0034 source construction remain unimplemented;
the full-suite PASS applies to the clean committed-source candidate, not the unrelated primary WIP.

##### C1 step review and corrected sequencing

Independent step review stopped the first C1 design on two authentication defects and a lifecycle
misstatement. Adopt these corrections before implementation:

- Validate an existing lookup basis against all current dependency rows **before** refreshing any
  immutable backing witness. Public mutation followed by freeze must not authenticate stale lookup
  summaries. Refresh only after every required family freeze succeeds; partial failure invalidates
  C1 readiness. Ordinary binder freeze errors and return behavior remain unchanged.
- Root attachment records the actual statement-entry scope and its creation parent. The existing
  normalization owner records the permitted root-parent transition; it does not replace the entry
  scope with a publication/global scope. Ordinary child receipts retain their actual creation parent.
- Fresh product instantiation is the production lifecycle under ADR-0023. Count one prefix
  projection per compilation batch. Freeze/fork is compatibility coverage, not a new production
  shared Binder. Defer prefix acceleration; sharing an index alone does not prove zero prefix work.

Split C1 sequentially into C1a consumed-input/scope receipts, C1b final basis/borrowed views, and
C1c shared scalar policy/batch resolution. Only C1a is authorized now. Intermediate receipts are
construction evidence, not authenticated resolved rows or a ready census. C1b must enumerate its
exact dependency read-set before implementation. Its domain check precedes generic lookup, so an
unsupported inner binder cannot expose a same-named outer parameter. C1c must account for total
basis, scope, generic, namespace, participant and output work, not only the final lookup operation.

##### C1a acceptance contract — consumed input and scope receipts

Keep C0's public fact accessors unchanged. Privately share its AST-free projection through `Arc`:
a manifest clone shares occurrence origin; rebuilding over the same Program or an equal parse does
not. The allocation identifies the reference input, not an authenticated declaration table or a
bound instance. Record its consumption for each module/source pair, including reference-only and
empty sources. The same input may be consumed in two modules, but their receipts remain qualified
by their distinct module/source identities. Reinstalling a receipt for one module is invalid.
Retain no AST, source text, AST address, numeric pointer token, or process-wide origin registry.

At existing binding operations, retain exact `(AstType, full span)` anchor, input operation/region,
actual entered ScopeId, ScopeKind and creation parent. Index C0 anchors once per input; lookups must
not search containing spans or walk the AST again. Record root, function/arrow, ordinary block,
for/for-in/for-of head, switch and catch scope creation. FunctionBody itself creates no extra scope.
Binder-skipped functions/blocks remain explicit missing attachment observations; neither the nearest
C0 region nor a same-start function/block map entry can manufacture an attachment. Duplicate exact
anchors, conflicting attachments and malformed operation relationships are typed invalid evidence.

Root consumption covers ordinary source, source-compiled library, fresh product users, and frozen
continuation users through their existing entry points. For source-compiled scripts, record the
shared global entry when that is what `bind_library_statements` receives. For ordinary/product
sources, record the actual syntax module. Distinguish creation and final parent through a receipt
emitted at the existing normalization mutation, with exact source/module, old parent and authorized
new parent. Never infer permission afterward from whichever parent happens to be present.
Root transitions do not rewrite function/block parents or certify an unrelated graph mutation.

The `source-scope-receipts.ts` fixture proves the current binder's scope choices, not new TypeScript
semantics: ordinary-for callbacks belong to its head; for-in/of source callbacks and switch
discriminant callbacks belong to the enclosing function; case tests belong to switch scope. Catch
and catch body are separate scopes. Parameter-default callbacks belong to their function scope.
The `var` initializer's callback remains in the lexical block even though its storage is hoisted.
Assert full anchor and parent identity against the existing binding owner, not lookup by spelling.

The `source-scope-holes.ts` fixture contrasts bound field/method/control functions with currently
skipped conditional/computed-key/member/decorator/static-block/type-key callbacks. Preserve every
C0 reference and distinguish missing attachment from no runtime scope required. Do not extend the
binder's visited grammar to make the receipt appear complete. Header/generic resolution, class/type
domain support and per-reference lookup contexts remain C1c work, not partial scope guesses here.

Direct tests must cover consumed clone versus rebuilt/equal manifest, reference-only/empty input,
same spans in different sources, dual consumption, duplicate installation, unknown/wrong-module
requests, dropped parser arenas, exact-anchor collision and conflicting attachment injection.
Verify legitimate root normalization in ordinary/library/product/continuation routes and original
scope choices in nested loops/switches. Fork/resume clears source-consumption authority; freezing
does not reissue it or publish lookup readiness. C1a introduces no reusable prefix index or seal.
Unavailable C0 coverage cannot become a complete input receipt. Test instrumentation may inspect
private receipts; no public success API may let a caller treat them as a resolved census.

Count the new work: one linear anchor index per input, bounded exact probes per actual bind hook,
and linear receipt finalization. Repeated span-starts, many sibling functions, and many sources must
not trigger per-hook full-map scans or a second manifest build. Preserve C0 counters and all existing
binder/reference inventories. New private receipt fields must participate honestly in Binder's
internal data-reference audit; they are not semantic query or persisted type references.

Territory: binder declaration storage, bind hooks, the existing root normalizer, new private
source-input receipt module and its specs, and necessary binder module wiring. No checker, product
schema, type-model, scalar resolver, library provider, scoreboards, or C2–C4 path changes. Implement
on a new clean detached worktree from this spec, not on the isolated literal experiment or primary
WU1 edits. After review and leader gates, integrate this behavior-neutral receipt unit separately.

C1a candidate passes its 15 direct specs and ordinary conformance, but the existing exact-site
shape guard rejects the new private `source_inputs: SourceInputs` field. Authorize only that explicit
addition to its private-field list and an exactly-one assertion. Preserve the complete public-field
list, all other private-field exclusions, and the sole declaration-table exact-site index checks.
The new receipt's identity edges must remain covered by the reference audit. This is a guard contract
update for the already specified owner, not C1a acceptance or permission to add another lookup index.

C1a independent implementation review returned FAIL on source-zero origin validation and incomplete
regression assertions for the added identity audit. Handle them sequentially; neither opens C1b.

**C1a-R1, origin authentication.** Consume `source-scope-reference-only.ts` twice with one manifest,
distinct modules and User ordinals 0/1, first with source key 7 and then with source key zero. Both
conflicting-origin cases must reject the second receipt. The actual trusted prelude still works;
a source number alone never grants its exemption. Identify the prelude by the existing trusted
module/entry role, not by an origin flag guessed from user input. Ordinary source/library/product
inputs must retain their module/source/origin join even at reserved keys, or record typed invalidity
for the reserved-key use. Preserve ordinary binder behavior, the consumed Arc and all reference rows.
Keep dual consumption with valid distinct source keys, valid source-compiled prelude, empty prelude,
product and continuation controls. Test a user with zero and no origin conflict as well: it must not
take the prelude exemption, regardless of whether full validation accepts it or reserved-key policy
rejects it. No namespace error suppression or changes to public source-key allocation are allowed.

**C1a-R2, reference-audit gate.** After R1 is green, strengthen tests without changing enumeration.
Keep the old 317-edge assertion and add exact new-edge multiset/count-and-target assertions qualified
by consuming module. Cover the existing field contract completely:

| Fields | Required relationships |
|---|---|
| 1–3 | input module/source/creation parent |
| 4–7 | normalization module/source/old parent/new parent |
| 8–11 | attachment scope/creation parent/anchor operation/region |
| 12–14 | operation identity/parent/region |
| 15–19 | callable/class/namespace/header owner and result context |
| 20–22 | region identity/parent/anchor |
| 23–25 | reference identity/operation/region |
| 26–28 | copy declaration/reference and boundary operation |
| 29–30 | anchor-index region and collection-error module |

Use the existing rich C0 fixtures plus a controlled invalid collection entry so every field occurs.
Every operation/region/reference target must be within its consuming input's own domain. Include
two equal-span inputs in different modules and local/full views around freeze/fork; no numeric local
ID may select another module's facts. Counts alone are insufficient: compare target multiplicities
and reject substitution of another valid in-range target. Expected edges must come from the retained
fixture facts/explicit relationships, not from calling the production enumeration to construct its
own expectation. An external omission of operation-parent field 13 must fail the new test (the
reviewer removed 272 such edges while all old tests passed). Cover representative owner, region,
copy/boundary and error omissions too. Do not rebaseline the old count or exempt the private field.

R1 may touch only source-input finalization, its existing binder call-site handshake if needed, and
receipt specs. R2 may touch only reference-audit test code and test-only invalid-input setup. Keep
the authorized exact-site guard unchanged. Freeze each delta/evidence separately, then obtain an
independent re-review of their combined candidate before leader integration gates.

C1a closure: independent re-review gives GO with both origin/audit findings closed. Leader verified
all 2,026 frozen file hashes, then passed the complete default-member test suite, exact ordinary
conformance (`583/699/583/0`), all-target Clippy and formatting on the unchanged candidate. Integration
transferred only its eight reviewed binder files; leader repeated 169 binder unit tests plus one
integration test, all 11 lifecycle tests, Clippy and formatting successfully in the primary tree.
The original WU1 diff still matches `177d58ca77073296f8ce80bbea0946c66c6503a3ff777766566ee6aec3178b03`.
The complete-suite receipt applies to the isolated committed-source candidate, not to unrelated WIP.
Consumed-input identity, actual scope receipts and their audit are accepted. No final lookup basis,
borrowed authenticated census, C1c resolution, C2 certificate or source-construction cutover follows.
Next is the separately specified C1b exact dependency/read-set and finalization contract.

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

### Approved F1 continuation — 2026-09-05

F1's first independent review returned STOP: four model tests plus twelve other enum tests
passed independently, but the checker failed to compile at eleven matches in nine files. The
partial experiment proved identity, selected union/enum rules, and substitution transport only.
It did not prove source transport, inference, relation, display, or cache behavior. The user
approved one bounded continuation and another independent review, not F2 or full migration.

**Local semantic contract (experimental only):**

- Storage, substitution, member/element projection, semantic demand, and inference candidate
  collection preserve complete mode-bearing IDs. A shared evaluation overlay must not erase mode.
- Relations compare regular literal identities only after coordinator publication/poison checks
  and overlay/frontier handling, before relation identity/cache/cycle success. Local equality and
  narrowing predicates compare the underlying value or canonical enum identity, but retain the
  original mode-bearing result. Class construction gains no new query capability.
- Actual widening broadens widening literals contribution-by-contribution; regular literals stay
  exact. Same-value regular/widening unions select the regular literal in either order. Enum
  wrappers retain the existing unique canonical owner. Display does not expose the mode.
- Source producers on the selected path must distinguish ordinary literals from annotated and
  const-context literals before shared inference. Candidate fixing applies its widening policy
  before final dependent-constraint validation. No call-side post-fix repair supplies the answer.
- Existing regular IDs cannot silently be reinterpreted as ordinary widening literals. Before
  changing shared widening, inventory its producers/consumers and prove compatibility of ordinary
  controls. No transitional boolean, fixture dispatch, mutable metadata, or alternate inference
  pipeline may hide an incomplete migration. If compatibility requires broader producer migration
  than the selected path, STOP with the concrete boundary.

**Expanded territory.** In the scratch worktree only, add these paths under
`crates/typokat-check/src/check/` to the existing F1 grant:

- `checker/classes/construction.rs`, `checker/decls/mod.rs`,
  `checker/eval/instantiation.rs`, `checker/eval/keyof.rs`, `checker/eval/mapped.rs`, and
  `checker/eval/mod.rs`: justified closed-leaf/identity handling for the representation.
- `flow.rs`: mode-insensitive primitive/enum comparison predicates and adjacent identity
  comparisons only; no assignment facts, target reduction, loop resolution, or memo redesign.
- `query/mod.rs`: representation admission and coordinator-owned relation preflight; no new
  general declaration demand. Existing `checker/calls.rs` classification remains within F1.

Model tests stay in type-store modules; inference tests stay in inference modules. Checker
integration witnesses may be added to `checker/tests/assertions.rs`, and coordinator witnesses
to existing `query/` test modules. These are explicit test ownership, not additional production
capabilities. A newly required production owner remains a stop-and-ask boundary.

**Acceptance before another verdict.** Retain the committed B44 source matrix unchanged. Prove
one real primitive declaration/member/tuple/substitution-to-generic path, both mixed/equal-value
union orders, ordinary widening controls, and the negative dependent-constraint case. Add model
witnesses for enum value collisions/families, open numeric enums, intersections, frozen-store
fork/dedup/reference census, and equal display. Add coordinator witnesses for both relation
directions, nested operands, poisoned/unavailable enclosing surfaces, exhaustion, overlay order,
and no tainted cache promotion. Once a runnable candidate exists, authenticate it and run the
required inference differential seeds 1/2/3 × 400 and committed repro gate; classify every change
without rebaselining. Remaining B44 flow/class/default failures stay separately attributed.

The first F1-only patch is retained with SHA-256
`596cf551ef1de20ef2b4304675153936ae7723956dfe5d0a5963b3b95d0e977b`.
This continuation consumes F1's single remediation round. Another failed independent review
means STOP. No experiment is integrated or committed as production implementation here.

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
