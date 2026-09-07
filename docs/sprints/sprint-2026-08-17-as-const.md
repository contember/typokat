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
M1's immutable literal-mode migration remains unaccepted as a shipping baseline. The publication
cluster below is accepted in isolation; it does not accept the baseline's remaining regressions.

## Publication cluster — accepted in isolation

**Bounded GO:** frozen `d1dab67b…` has no independent review findings. Leader reran 26 focused cases,
three projection tests, full tests, ordinary conformance, Clippy and fmt on the unchanged 2,067-file
candidate. Full tests retain only the known compound failure, two type-tag guards, stale 671-versus-599
inventory and the same 182 marker mismatches across 787 sources. Review also retains 1,200 raw-output
matches, 116 complete owner records, 72 strict-tsc transitions and the unchanged repro scoreboard.

Authorities: `publication-fixed-slot-independent-review.md`, `publication-cluster-leader-closure.json`,
and `publication-fixed-slot-leader-authentication.json` under the evidence root. This is not shipping
acceptance of M1/WU1 or backlog 44; production stays isolated until its baseline regressions are closed.

Publication follows [ADR-0035](../decisions/0035-finalize-literal-types-at-publication-owners.md),
the retained-source boundaries of [ADR-0037](../decisions/0037-retain-class-initializer-validation-sources.md),
and the explicit conservative preparation-gap policy of
[ADR-0038](../decisions/0038-reject-inconsistent-contextual-class-publication.md). Requested opaque-rest
admission is separately reduced under spec `c292338a`, owned by backlog 123. Fixed declared IDs are
independent of unused rest alternatives; normal Pass recovery and annotation authority are unchanged.

The accepted Pass preparation/transport contract remains binding through ADR-0035, its 26 fixtures
and the frozen source/spec at `f58604a6`; do not re-demand raw publication targets, conflate unavailable
with absent context, or change grouping, annotation, memo-identity and exactly-once effect boundaries.

## Active R1c replacement — discriminator spec, implementation held

Reject the observation/memo transport and raw-return restoration as the next implementation basis.
The external architectural diagnosis and `r1c-replacement-independent-design.md` agree on native
call/inference ownership, but the suggested static A/B/C replacement is not yet a dispatch contract:
mode evidence is phase-sensitive; fixing distinguishes independent typed evidence from parameter
demands; nested generic calls need an outer mapper while literal returns retain symbolic context.
Keep original signature, ordinary preview P and expected-result-only R as distinct authorities.
Unresolved conditionals stay on the old/opaque policy; no synthetic conditional identity graph.

The three replacement fixtures carry 19 strict-tsc markers, including mixed/omitted ternary arms,
logical operands, literal versus nested-call/factory returns, and object/array-wrapped enum returns.
`r1c-replacement-leader-final-spec-oracle.json` retains exact oracle and frozen-baseline comparisons.
These fixtures approve required behavior, not a replacement algorithm. Complete one native phase/mapper
contract before dispatch; no further patches to the rejected observation strategy. Protect the accepted
publication baseline and unrelated primary work; remove only authenticated agent-owned R1c changes.

### Rejected R1c strategy — historical contract only

Fix the two introduced direct generic-enum errors without implementing backlog 122's full fixing
engine. The obsolete `r1c-design.md` depends on rejected R1b experiments and is not authority.
The transport contract passed independent step review. Its initial memo refusal failed the unchanged
scaling gate (8/52/1,004 walks at depths 2/4/8); the independently reviewed replay amendment below
supersedes that refusal. Candidate `96db4da7…` passes scaling (6/20/72), 30 focused cases and unchanged
differential/official rows, but independent review finds three composition regressions. Repair them
together using `inference_scalar_composition_regressions.ts`: unobserved function parameters must not
mean unused context; transported scalars must preserve rest-callback and enum-return/copy diagnostics.
The leader permits bounded mode corrections at existing contribution preparation/final selection for
these compositions, superseding the byte-identical multi-path requirement only where needed. Preserve
occurrence cardinality, order, relation/query ownership and constraint completion; no enum exceptions,
new inference pipeline, rest aggregation rewrite or post-call repairs. Unobserved raw routes receive
conservative coverage, not a new contextual function implementation. Broader backlog-122 work stays out.
The three measurement-vector updates to `[2,2,1,0,0]` are approved after independent verification of
observing/nonobserving key separation; scaling bounds remain unchanged. During repair use focused tests
and the retained reduced probes. Run expensive gates once on the stable candidate; review changed
mechanisms and reuse authenticated evidence for unchanged code rather than repeating the whole audit.

### Policy and admission

- Extend the existing request with explicit `ContextualPreview { retained_return }`,
  `FinalCall { retained_return, context_facts }`, and `ClassApplication` purposes. Preview/class keep
  existing behavior; final ordinary/construct calls use the bounded policy. No fabricated return ID.
- Eligibility is exactly one surviving ordinary `CallSiteCandidate` occurrence after existing optional
  fallback pruning, with a regular/W scalar literal or enum-member ID. Equal occurrences stay separate;
  union-valued candidates and expected-result evidence do not qualify. Preserve optional-fallback and
  primitive-constraint precedence. Do not change multi-contribution selection or constraint completion.
- Record the original inference target at collection. Classify a binder as direct, structurally
  nested, or unmodeled. Admit only binder paths through static object properties, arrays, fixed tuple
  elements and readonly wrappers. A path through a callable, union/intersection, conditional, rest
  container or unresolved operator is unmodeled, not merely nested. Use ready identities, no new query.
- For admitted scalar target/return roles: preserve direct returned T unless its parameter context was
  consumed; widen a direct target T into a nested result; preserve a structural target contribution.
  Only W contributions broaden; regular IDs remain regular. Unmodeled roles/coverage use the unchanged
  algorithm. Do not freeze later candidates, interpret raw conditional branches, or repair call results.

### Original-context cursor

- Seed an explicit cursor from original signature parameters before preliminary substitution, paired
  with effective argument targets. It carries the original node and this candidate's outer binder set;
  do not redefine `PreparedInitializerContext.original_target` or invert effective IDs to find binders.
- Project only while visiting an existing source child: static object property/index fallback, array
  element, ready tuple/rest position, parentheses/value operand, and callable parameter slot. Use the
  existing positional contract with the same source count. Keep only the current traversal cursor;
  no mirror context tree, declaration walk, eager annotation descent or durable provenance registry.
- Successful unannotated fixed-parameter projection emits binders from its original matched slot.
  For a source rest parameter, mirror the existing first-rest-parameter selection in both signatures;
  do not change its aggregate inference behavior. Explicit annotations and omitted slots emit nothing.
- Admit correspondence only for ready matching structural/callable shapes. Ambiguous, opaque or failed
  projection marks affected original binders unmodeled, never unconsumed. Determine affected binders
  from original free-parameter identity intersected with the active outer set; if a reliable local set
  is unavailable, mark the entire active set. Fixed symbolic elements remain identities, not bounds.
- Raw routes, parameter defaults and callback bodies receive no inherited cursor. Nested calls seed
  their own observation. Container child arrows receive only the cursor projected by contextual descent.

### Facts, effects and memo lifetime

- Produce consumed/unmodeled binder facts in the existing candidate-local `CheckerEffects` tree.
  Nested lexical frames transport them with existing tickets. At the contextual inference capture
  boundary drain facts recursively before discarding diagnostic effects; return them alongside the
  existing source/rewalk result, then aggregate in `PreparedInferenceArgs` for the final request.
- Inner call captures drain only their own facts before outward effect merge. No global set or
  binder-ID-only filtering across attempts. Facts are not observable diagnostics or durable event data;
  preserve records, obligations, raw-batch supersession and nested-ticket ownership unchanged.
- Fully observed success may establish nonconsumption. Known no-use routes add no facts; failed or
  ambiguous routes mark affected coverage unmodeled. Retain actual source failures/diagnostics and the
  recovered nested-body error at `contextual_parameter_fixing.ts:26`; facts cannot turn failure into success.
- Reuse `infer_contextual_source_observing`'s existing fresh capture as the per-walk fact owner.
  Recovered container/lexical child facts belong there; never snapshot accumulated parent facts or
  subtract sibling sets. Nested callees drain their own observations before outward effect merge.
- Extend the existing contextual memo key with owned original-cursor authority: original node and
  outer-binder universe by content equality/hash. No observation differs from opaque observation.
  Keep every existing environment/prepared-context field, dependency check, budget and region lifetime.
- Memo values carry source/span, final rewalk status and immutable consumed/unmodeled facts, never
  tickets, diagnostics, effects or journals. Nonobserving entries have empty facts and a distinct key.
  Preserve raw memo behavior; cloning a served value must not change declaration-write adoption.
- On an observing miss, the inner walker returns a stack-local pending insertion token with the
  original lookup key and pre-execution declaration-log mark. Insert only after the reporting wrapper
  normalizes validation failure and the observing capture drains its complete facts. Assert both fact
  sets belong to the original outer universe. Merge only fact-drained residual effects outward;
  return the facts through the existing triple to `PreparedInferenceArgs`, without a second merge.
- Cache observing outcomes only for final `Rewalked`, non-error source and no typed failure.
  Successful unmodeled coverage is cacheable; all observing `KeptRaw`, failed/exhausted and partially
  completed outcomes are not. Nonobserving insertion stays unchanged. Never resample the key at insertion.
- On a hit, validate/adopt declaration dependencies and net writes once through the existing owner,
  merge cloned facts once into the current observing capture, and let its normal drain return them.
  Replay no diagnostics/events. Retaining walks remain nonmemoized; do not flip retain-checks,
  suppression or observable counts. Native retries borrow immutable facts and keep existing rollback.

Territory after step approval: `infer/{mod,tests}.rs`, `checker/{calls,expr,context,mod}.rs` and focused
tests. No type/binder/query/relation, class construction, publication policy or shared projection edits.
Direct tests must pin fact sets for annotated/omitted/unrelated equal-effective binders, nested
object/array/parenthesized/rest routes, two equal contributions, role fallback, failed preparation,
selected/rejected/nested/repeated calls, cold/warm fact equality with nonempty sibling facts, distinct
original nodes/universes under equal effective contexts, declaration invalidation and exactly-once
write adoption, failure/recovery, retaining-walk exclusion, and native retries. The existing scaling
bounds must pass unchanged; another failure stops implementation rather than relaxing key authority.
The three `inference_single_{scalar_roles,nested_position,consumed_context}.ts` fixtures carry 27 exact
strict-tsc markers. Keep all 26 publication fixtures, 116 owner records, 1,200 differential inputs,
repro and official regression gates. Unchanged broader fixing/variance truth remains backlog 122.

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

Keep the accepted publication cluster as the frozen baseline. Each remaining unit needs its own
bounded spec, independent review and exact regression disposition before implementation acceptance.

| Order | Required work / retained blocker |
|---|---|
| 1 | Publication cluster accepted in isolation; preserve its source/diagnostic contracts during remaining repairs |
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
