# Sprint — AST-directed `as const` contexts (2026-08-17)

**Goal.** Finish the `as const` half of [44](../backlog/44-satisfies-as-const.md): retain literal
values, make literal-owned object properties readonly, infer readonly tuples, and stop const
propagation at references. The [satisfies sprint](../archive/sprint-2026-08-17-satisfies.md) shipped.

**Standing.** No complete feature candidate is accepted. Implementation is isolated; green receipts
from earlier candidates are not current acceptance. No push, publish or deploy is authorized.

## Current delivery boundary

The user authorized autonomous completion and bounded compatibility, then explicitly approved
[ADR-0036](../decisions/0036-bound-contextual-class-publication-migration.md)'s class migration
reduction. Historical F1 approval limits and C1b–C4 scheduling below the old log are no longer active.

| In scope | Outside this delivery |
|---|---|
| Primitive/enum literals; nested literal-owned objects and arrays; readonly properties/tuples | Spread/elision: 71; computed keys, const-context object methods and invalid-operand TS1355: 75 |
| Parentheses, shorthand and reference boundaries; suppression of outer context inside const syntax | General assertion compatibility, module breadth and general lazy declaration resolution |
| Existing value-return aggregation timing for all inferred callable-body consumers | New bare-return/fallthrough/reachability discovery: 46 |
| Literal-owned `as const` in instance fields, ordinary readonly modes, annotations and ready-context controls | General contextual class-initializer preparation: 123 |
| Introduced ordinary-checker regressions, exactly-once effects and immutable publication | General construction-order compatibility: 120; no C1b–C4 restart |

Existing unrelated gaps need explicit backlog ownership. Deferral does not license new false cleans,
blanket unsupported admission, removed assertions, diagnostic suppression or bulk scoreboard changes.
Retained class contextual semantics are legacy and incomplete, not proof of prepared-context readiness.
The class scope reduction does not accept the unreviewed class cutover.

Binding method: [dev-method](../reference/dev-method.md) and [invariants](../reference/invariants.md).
Leader-authored spec commit → implementation subagent → independent adversarial review → leader
verification → explicit-path integration commit. Keep one active root-cause cluster.

## Accepted work and candidate boundary

| Work | Accepted scope |
|---|---|
| C0/C0a, `93058337` | One-pass structural source-reference facts; not resolved-use authentication |
| Lifecycle scanner, `7088f7c6` | Skip only absent optional `examples/`; required/present roots remain strict |
| C1a, `ae145bc8` | Consumed-input identity, actual scope receipts and exact reference audit; not a final lookup basis |
| Isolated R1a | Prepared multi-candidate result before final constraint validation; not feature integration |
| Intersection T1/T2/T2a experiment | Bounded retained-expression/traversal proof only; excluded from the delivery candidate |

C0/C1a clean candidates passed full default tests, ordinary conformance `583/699/583/0`, Clippy and
formatting. These receipts do not cover the primary WU1 edits or the current feature candidate.

The delivery baseline reconstructs inherited WU1 + frozen M1 + reviewed R1a, retaining landed binder
and scanner changes. R1b/R1b2 and later planner/tracer production experiments must not be copied in.
M1's immutable literal-mode migration and all subsequent feature implementation remain unaccepted.

## Active unit — Pass-owned effective publication contexts

Implement [ADR-0035](../decisions/0035-finalize-literal-types-at-publication-owners.md), narrowed only
at the class boundary by ADR-0036. The first role-aware candidate passes focused publication tests
but introduces seven B32 `keyof` false positives: child checking prepares context, then publication
uses the original deferred context instead of the effective literal union.

**Independent review: FAIL.** Candidate `036f7863…` repairs ready-rest types but still withholds a
fixed slot when an unused opaque-union rest rejects full-signature projection, losing a bad-call error.
The new conservative external/self-dependent errors are explicitly accepted by
[ADR-0038](../decisions/0038-reject-inconsistent-contextual-class-publication.md), not called legacy
restorations or backlog-123 completion. That amendment does not waive the ready-rest producer defect.
**Review contract:** align already-admitted class-arrow positional parameters with ready array/tuple contexts;
preserve inferred body and completed function identities, explicit parameter/return authority and
[ADR-0037](../decisions/0037-retain-class-initializer-validation-sources.md)'s source ownership.
Move existing pure positioning into `checker/signature_projection.rs`; Pass delegates unchanged,
and a private `SurfaceTypeFactory` wrapper gates ready class inputs before sharing the same algorithm.
Territory: that new module, `checker/{mod,calls}.rs`, `classes/{initializer,surface_types}.rs`, tests.
The leader explicitly reduces class admission when a requested position needs an opaque rest container
(`T`, nested rest `T`, or a union containing one), even if constrained/defaulted: report the existing
initializer incomplete, retain lexical diagnostics, discard that initializer's receipts. This is new
admission behavior, not an ADR-0038 waiver. Fixed symbols and symbolic array/tuple elements remain ready;
explicit source annotations bypass unused projection. Gate only positioning container edges; preserve
Pass opaque fallback, optional-slot counts, declared IDs and all query/inference/reporting ownership.
The repair is limited to `signature_projection.rs` and direct projection/producer tests: return a
known fixed declared ID independently of unused rest alternatives. Pin identity/literal/mixed source
forms in `publication_class_unused_union_rest.ts` (six strict-tsc errors, zero incompletes). Keep the
ordinary Pass entry unchanged; do not broaden requested opaque-rest admission or alter failure recovery.

### Preparation and transport contract

- Reuse `Pass::evaluate_type`, the existing coordinator and savepoints. A checker-local prepared
  product retains original target, evaluated publication context, apparent shaping context and
  contextual `this`. It is not a second evaluator, expression graph or provenance registry.
- Original targets remain authoritative for generic calls and declared/excess/satisfies obligations.
  Preserve symbolic parameter identity in publication context, including union alternatives; follow
  bounds only for shaping. Extract `ThisType` at the existing normalized-intersection boundary.
- Prepare outer operator chains and contextual alternatives in one traversal, not an identity pass
  followed by an apparent pass. Projected children become roots when the existing source walk reaches
  them. Do not eagerly normalize every property, element or return of an annotation tree.
- Publication consumers retain effective context alongside source/span; other consumers may keep the
  old tuple wrapper. Never re-demand the original target at finalization after checking the child.
- Share preparation through transparent parentheses/value branches within one contextual execution
  attempt. Conditional tests precede arm preparation; logical `&&` keeps its context-free left side;
  call inference receives the original target. Selected-candidate reconstruction is a separate attempt.
- Use a body-local preparation slot at the first existing return-context consumption boundary, then
  reuse it for later value returns and aggregate publication. Do not prepare eagerly at body entry,
  finalize individual returns or introduce an arrow-only legacy collection path.
- Distinguish unprepared, absent, prepared and failed states. Real `unknown` and symbolic types are
  not unavailable context. Preserve typed exhaustion and existing declaration-owned failure reporting.
- Commit preparation writes only for a successfully prepared product. The ordinary transaction helper
  commits every `Ready(_)`, so `Ready(Unavailable)` is insufficient. Unavailable and exhausted outcomes
  both roll back but remain distinct; neither can produce a successfully finalized partial aggregate.
- Audit satisfies-arrow shortcuts, contextual rewalks and `ContextualWalkKey`: equal shaping views
  must not alias different original/publication identities. Extend the existing key, not cache topology.

The pure role finalizer returns a completed `TypeId`. Matching applies to the whole expression source;
finalize elements before collection and inferred returns after existing value aggregation. Shallow
regularization and wrapper-only widening must not transform referenced container members. Full role
algebra, broad-context absorption and annotation authority are specified in ADR-0035, not duplicated here.

### Acceptance witnesses

Paths below are under `tests/cases/b44_as_const/` unless stated otherwise.

| Family | Required witnesses |
|---|---|
| Role/grouping/mode algebra | `publication_grouping.ts`, `publication_union_sources.ts`, `publication_return_aggregation.ts`, `publication_class_roles.ts` — 27 strict-tsc line/code pairs |
| Effective context | `publication_effective_context.ts` — 9 pairs; `publication_context_views.ts` — 7 pairs; unchanged B32 forward/reverse/class/alias/heritage/mutual controls |
| Prior failures | `contextual_arrow_checks.ts`, `contextual_return_contexts.ts`, `contextual_return_literal_members.ts`, `contextual_container_literal_modes.ts`, `return_literal_contributions.ts` |
| Class compatibility | `publication_class_context_controls.ts`, `publication_class_self_context_control.ts`; no blanket deferred-context rejection |
| Review regressions | `publication_class_scalar_satisfies.ts` — 8 strict-tsc pairs; `publication_enum_contexts.ts` — 10 pairs; direct identities and valid mutable-copy controls remain required |
| Class source alignment | `publication_class_lexical_source.ts`, `publication_class_validation_{boundaries,authority,failures}.ts` — 14 explicit legacy TK1360, 16 strict-tsc errors, 8 unchanged incompletes; exact ownership/identity controls in ADR-0037 |
| Direct Rust tests | Exact mode IDs; symbolic versus bound contexts in both union orders; preparation counts; late unavailable/exhausted alternatives roll back earlier writes; memo identity/ThisType; selected/rejected effects and defaults |

Preserve all 13 prior arrow/default owner tests and the 116 classified callback diagnostic-owner
records. The only approved old callback test change is parameter `any` → `number` plus its name;
actual `string` return, one diagnostic and no incompletes remain asserted.

B61 `field_initializers.ts:9` keeps TK2322 and target `[number, string]`; its source display changes
from `[1, 2]` to `[number, number]`, independently verified against the exact tsc 6.0.3 initializer.
This is one explicit message correction, not blanket rebaselining.

The class-control file is clean in three feature candidates; the pre-feature binary is incomplete
at its `as const` expression. Only the separately verified broad controls have pre-feature parity.

## Remaining critical path

Close the active publication cluster before starting another implementation front. Each later unit
needs its own bounded spec, independent review and exact regression disposition.

| Order | Required work / retained blocker |
|---|---|
| 1 | Repair the three independent publication findings; re-review the cumulative cutover, including bounded class changes under ADR-0036 |
| 2 | R1c: retained callee return role, original candidate position and actual contextual parameter consumption; fix B42 `structural_transport.ts:11` and direct enum-result over-widening without an enum exception |
| 3 | Minimal recursive fallback repair: `recursive_constraint_fallbacks.ts` and `constraint_instantiation_order_supplied.ts` gain TK2589 versus pre-feature/tsc; general source-order gaps stay with 120 |
| 4 | M2-min: declared-target transfer, invalid-write recovery and compound result modes; `compound_assignment_does_not_inherit_rhs_const_provenance` remains RED |
| 5 | M3: inferred defaults retain context without annotation authority; remove `undefined`, retain `null`; authenticate ready enum keys at the direct enum root; safe class-reference publication |
| 6 | R2b: eight newly exposed official rest-body property errors; preserve symbolic generic signatures, arity and readonly constraints under ADR-0033 |
| 7 | Reconcile mixed/deferred fixtures, literal-tag guards and frozen inventory; full-diff review, unchanged-tree closure gates and lifecycle updates |

M2 follows [ADR-0032](../decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md):
expression result, assigned source and declared-target transfer are distinct; keep non-union declared
surfaces and whole-source union coverage. Exhaustion is not successful reset. Flow discharge is
query-local; no duplicate semantic walk, durable provisional answer or whole-Pass snapshot.

R2b follows [ADR-0033](../decisions/0033-separate-contextual-row-views-from-symbolic-signatures.md):
positional body views do not replace source binders/signatures. Any participant write, including
captured/dead/later writes, disables correlation; unrelated/shadowed writes do not.

The retained official regression set is four files, not permission to rebaseline:

- `dependentDestructuredVariables.ts`: new TK2339 at 205/208/237×2/240×2/324/327.
- `literalTypesAndTypeAssertions.ts`: new TK2322 at 12/13.
- `numericLiteralTypes3.ts`: new TK2322 at 15/31.
- `stringMappingOverPatternLiterals.ts`: dropped TK2322 at 25/26/27.

## Deferred owners

| Owner | Boundary |
|---|---|
| [46](../backlog/46-return-path-analysis.md) | Bare returns, fallthrough and reachability; disabled `b46_return_paths/contextual_mixed_returns.ts` keeps its assertions |
| [60](../backlog/60-fresh-literal-union-targets.md), [69](../backlog/69-signature-rest-parity-tail.md) | Verified old union-initializer FP at `assignment_semantic_reduction.ts:29` and eight outer rest-relation FPs; M2/R2b neighboring repairs and recovered errors stay required |
| [51](../backlog/51-narrowing-tail.md), [76](../backlog/76-lazy-value-type-resolution.md) | Pre-existing flow/loop-product and general declaration-availability gaps, not newly introduced regressions |
| [110](../backlog/110-contextual-return-literal-inference.md), [111](../backlog/111-structural-inference-literal-provenance.md), [122](../backlog/122-signature-candidate-fixing-policy.md) | Broader inference/fixing policy beyond required feature repairs; mixed B44 fixtures are not wholesale disabled |
| [120](../backlog/120-semantic-constraint-construction-order.md) | Source/constraint construction order; rejected R1b planners; stopped C1b/C1c/C2–C4; ADR-0034 remains proposed |
| [121](../backlog/121-optional-project-binder-probe-test-build.md) | Pre-existing optional `project-binder-probe` test compile failure; distinct from default-member gates |
| [123](../backlog/123-contextual-class-initializer-publication.md) | General class context preparation and preserved `b123_class_contextual_publication/` assertions; release-blocking, not sprint-blocking under ADR-0036 |

## Execution and evidence

- Primary WU1 and user-owned audit edits remain untouched. Do not stage `docs/INDEX.md`,
  `docs/ideas/README.md` or `docs/ideas/checker-semantic-boundaries-and-maintainability.md` with sprint work.
- Candidate: `/tmp/opencode/typokat-b44-delivery`. One implementation owner holds the Cargo queue;
  serialize Cargo with `cpu-lease run -n 2 --` and shared target
  `/home/matej21/projects/oss/typokat/target`. Do not run competing builds or leave owned jobs unsettled.
- Evidence root: `/tmp/opencode/typokat-b44-feasibility-evidence/`. Use frozen binaries, never the
  shared target executable, as comparison authorities. Local evidence paths are retained receipts,
  not portable substitutes for committed acceptance fixtures.
- `as-const-delivery-reconstruction.json`, `m1-*`, `r1a-*`: baseline authentication. `r1b*` and
  `tracer-*`: excluded experiments. `c0*`, `c1a-*`, `lifecycle-scanner-*`: accepted infrastructure.
- `publication-conformance-ownership-triage.{md,json}` and `publication-conformance-unclassified-followup.*`:
  frozen 182-record matrix; leader assigned its nine old FPs to 60/69. Not a current count; 26 mixed
  files cannot be disabled wholesale. Required neighbors and original evidence remain unchanged.
- `r2a-*-independent-review.md`: rejected helper repairs. `publication-contract-receipt.md` and
  `publication-contract-leader-authentication.json`: frozen first role-aware candidate; 2,040 source
  hashes, patches and binary verified, but the candidate remains rejected for context readiness.
- `publication-context-*`: frozen supporting seam; leader authenticated 2,043 input and 2,044 output
  files, the four-file change set, both patch reconstructions and the binary. Reported gates retain
  1,200 byte-identical differential outputs, 72 strict-tsc transitions and the unchanged repro gate;
  independent review rejected the cumulative candidate. Preserve corpus hashes and classify every new delta.
- `publication-class-scope-leader-spec-oracle.json`: three committed class-scope witnesses checked
  against strict tsc and four frozen checkers. Scope and repair specs through `c453b53f` were synced
  after handoff under `publication-review-repair-leader-spec-sync.json`; production hashes stayed unchanged.

## Verification and closure

1. Freeze the implementation's input/output manifests, owned delta, cumulative patch and executable;
   authenticate the outside-change set and completed jobs. Run focused owner/mode tests and fixtures.
2. Obtain independent adversarial review of the complete cumulative change, not only the last helper.
   Hunt the prior 26/20/18 failure families, grouping and copy transitions, class/reference boundaries,
   annotations, readiness and effects. A bounded GO is not whole-feature GO.
3. Leader reruns relevant gates on the unchanged candidate: checker tests, all-target Clippy, fmt,
   ordinary conformance, full default tests, differential/repro checks and official `run --check`.
   Every mismatch needs an oracle, cause and explicit disposition. Run the manifest/docs guards too;
   the leader's post-sync backlog-123 manifest gate passed 11/11 before the role repairs started.
4. After all repairs settle, reconcile `tests/conformance.rs`, `tests/type_tag_exhaustiveness.rs`
   (including the `flow.rs#5` wildcard adjudication) and `tests/surface/inventory.toml` explicitly.
   The inventory test is the count/order authority; do not reuse old expanded-corpus receipts.
5. Run closure commands under the Cargo lease, plus Clippy/fmt and contextual differential gates:

   ```sh
   cargo test
   cargo test --test conformance conformance -- --exact --nocapture
   cargo test --test conformance conformance_full_repeated_standalone_parity -- --ignored --exact --nocapture
   ```

6. Audit all ten retained prospective `syntax:as-const` official transitions individually before
   removing that gate. Keep excluded child forms explicitly nonclean; save only reviewed scoreboard
   movements. The frozen `m4-official-gate-*` screen is evidence, not closure.
7. Integrate only reviewed files while preserving unrelated WIP. Update coverage, divergence and
   surface/fixture indexes, completion manifest and roadmap. Complete `A-satisfies-as-const`, close
   backlog 44, stamp OUTCOME and archive this sprint only after all gates pass. Backlog 123 stays open.

Record a fair source-size comparison at closure: TypeScript
[#29510](https://github.com/microsoft/TypeScript/pull/29510) has three production files `+90/-47`
(checker `+81/-47`), versus seven test/generated files `+925/-0`. Its readonly foundation
[#29435](https://github.com/microsoft/TypeScript/pull/29435) has six production files `+164/-111`.
Separate typokat semantic/model changes, fixtures, generated output and gate costs; state the bounded
surface exclusions rather than claiming full upstream parity.

## Run log

- Infrastructure C0/C1a and scanner landed separately; their commits and scope are listed above.
- Immutable modes, assignment products and row views → ADR-0031/0032/0033. Source-construction
  experiments and C1b design constraints → backlog 120 and proposed ADR-0034.
- Role-aware publication and effective Pass contexts → ADR-0035; the active candidate is not accepted.
- User-approved class migration reduction → ADR-0036, backlog 123 and spec commit `0f4a69f6`.

### R2a review repair — contextual literal members

Three helper repairs were rejected: lost contextual primitive literals, retained widening modes in
containers, then union-source copied-write false cleans. Their fixtures and frozen reviews remain
mandatory evidence; the active ADR-0035 cutover replaces that repair strategy, not its assertions.

The full historical log, old file fences, experimental contracts and prior gate receipts remain in
Git: `git show 0f4a69f6:docs/sprints/sprint-2026-08-17-as-const.md`. Consult that version only for
historical evidence, not current execution authority. Do not append superseded plans back here.
