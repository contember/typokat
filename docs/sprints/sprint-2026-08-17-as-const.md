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
| Introduced regressions and the bounded first-source-demand construction protocol in ADR-0040; exactly-once effects and immutable publication | Construction-order breadth beyond ADR-0040: 120; no general C1b–C4 restart |

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

## R1c — accepted in isolation

Frozen `028486f4…` has independent cumulative GO under ADR-0039 with no findings. Leader verified
2,078 unchanged input entries, both patch reconstructions, all 62 native markers, 1,092 checker tests,
Clippy/fmt, full tests and ordinary conformance. Only the five named inherited failures and the exact
same 165 mismatch records remain. The completed nonincremental Cargo rerun is authoritative; an earlier
timeout and subsequent incremental linker failures are retained, not counted as completed tests.
`r1c-returned-context-leader-closure.json` and `r1c-returned-context-independent-review.md` carry the
receipts. Production remains isolated; this does not accept M1/WU1 or close the feature.

Implement [ADR-0039](../decisions/0039-own-inference-modes-and-context-mappers-at-native-call-boundaries.md)
as one unit: native contextual inputs, phase-correct contribution preparation, then selected-candidate
checking. It replaces the rejected observation/memo/composition strategy; the old contract is retained
only in Git at `c534a2a2`. Independent review disproved static A/B/C without phase evidence and the
nested-call mapper. The accepted decision includes the required bounded independent-input/upper-bound
subset of backlog 122, not general variance, scheduling or conditional substitution modeling.

Baseline: authenticated `r1c-single-fresh-input` (accepted publication d1dab plus original R1c specs).
Remove only authenticated agent-owned R1c hunks; preserve primary WIP and `claude-out.md`. Keep real
request purposes, retained return and original candidate target. Replace facts-only tests through the
explicit semantic-obligation mapping in ADR-0039, preserving baseline diagnostic/memo assertions.

Territory: `infer/{mod,tests}.rs`, `checker/{calls,expr,context,mod}.rs` and focused tests;
`checker/statements.rs` only for native block-return context forwarding. No Store/binder/query/relation,
class/publication, shared projection or rest-aggregation changes. P/R are immutable contextual authority,
not facts or a second evaluator; their content participates in both raw/contextual memo environments.

Acceptance: prior 34 R1c markers plus 19 replacement markers, the accepted publication fixtures,
conditional line 31 and contextual-body line 26, exact contribution modes/order, compatible independent
upper bounds, P/R priority/shielding, selected/rejected/retry isolation and unchanged owner records.
The replacement oracle is `r1c-replacement-leader-final-spec-oracle.json`. Baseline scaling proves PASS,
not exact 6/20/72; measure baseline/candidate with unchanged bounds. Restore old count vectors only
where sharing is actually restored and verify any deliberate environment-induced difference separately.

The accepted packet retains 1,200 differential outcomes, 604 publication probes, 116 owner records,
874 official rows and the unchanged repro ratchet. Review reused authenticated unchanged mechanisms.
Native scaling is 7/30/136 with original growth bounds and sharing assertions. The three first-review
regressions and returned-callback residual are closed by `inference_native_boundary_regressions.ts`
and `inference_returned_callback_parameter_context.ts`; keep invalid writes and valid reads separate.

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
| 2 | R1c accepted in isolation at `028486f4…`; retain native phase/P-R ownership and all regression controls |
| 3 | M2-min next: declared-target transfer, invalid-write recovery and compound result modes; `compound_assignment_does_not_inherit_rhs_const_provenance` remains RED |
| 4 | Recursive fallback: introduced TK2589 remains required; design the user-approved owner-local dependency/re-entry mechanism in parallel with M2, without concurrent production edits |
| 5 | M3: inferred defaults retain context without annotation authority; remove `undefined`, retain `null`; authenticate ready enum keys at the direct enum root; safe class-reference publication |
| 6 | R2b: eight newly exposed official rest-body property errors; preserve symbolic generic signatures, arity and readonly constraints under ADR-0033 |
| 7 | Reconcile mixed/deferred fixtures, literal-tag guards and frozen inventory; full-diff review, unchanged-tree closure gates and lifecycle updates |

Recursive preparation is complete, not implementation: `recursive-fallback-current-report.md` traces
the same feedback defect in full/partial fixers. Each substitutes through its previous derived fallback,
creating new nested TypeIds until a stabilization budget expires. All 35 depth rows and two lost-error
rows remain in the current 165-record baseline. The new `recursive_constraint_seed_identity.ts` has
eight strict-tsc markers and finite recovery/default/reverse-dependency controls. Keep original seeds
separate from completed fallback results; do not restore global one-pass fixing or change query budgets.

Shape-only admission and permanently frozen fallbacks were independently rejected. The first loses
already-erased interface/union ordering; the second loses a still-invalid reverse candidate's corrected
fallback. `r1c-recursive-owner-design.md` specifies a finite `Unseen / Active(seed) / Done(result)`
kernel and proves why canonical TypeIds alone cannot supply its dependency order.

The user subsequently included first-semantic-demand construction order and approved the complete
[ADR-0040](../decisions/0040-complete-registered-callables-at-source-demand-boundaries.md) protocol.
It supersedes proposed ADR-0034: registration, source-owned field construction, pre-ready query views,
persistent snapshots, early assignment decisions and detached header adoption form one contract.
Independent static admission covers all 37 rows in 22 unchanged fixtures; it is not an executed selector
or source cutover. Complete model/query lifecycle proof precedes real-source integration. No extra AST
walk, reconstructed TypeId order, query-controlled construction, runtime missing-record fallback or
silent incomplete is authorized. Full C1b–C4, general lazy values/bodies and class rescheduling stay out.

Preserve original mode-prepared seeds, exact completed TypeIds, full/partial absence, fresh exemptions,
violations, native P/R ownership and typed exhaustion rollback. Specify defaults that reach an active
earlier binder; declaration order alone does not prove completion. Pin opposite tuple/union orders,
interface/direct-object and retained/erased-alias controls, both reverse candidates and immutable
transport/memo identity. Classify all 37 retained rows; no all-37 acceptance follows from kernel traces.
`recursive_constraint_use_order.ts` and `recursive_constraint_use_order_supplied.ts` separately pin
16 strict-tsc markers and the opposite clean reads, including both full/partial still-invalid recovery.
The leader oracle is `recursive-owner-leader-order-spec-oracle.json`; these specs do not make the
new source protocol implemented and stay out of the active M2 input. The agreed three stages are:
leader specs/ADR, model/query lifecycle proof, then source/native completion with independent review
and unchanged-tree gates. M2 acceptance precedes overlapping source/query integration.
The portable 22-file/37-row acceptance table is in `tests/cases/README.md`. Additional source-frontier
and recursive-default fixtures have 11 leader-verified strict-tsc markers; the frontier's complete
diagnostic set already matches all three frozen baselines. The authoritative oracle is
`registered-callable-leader-spec-oracle.json`, not its earlier draft containing an unrelated old FP.

### Registered-callable implementation sequence

The three approved stages are subdivided into the sequential units below. Shared-file ownership
transfers only after independent acceptance. `registered-callable-implementation-map.md` supplies
the frozen source audit; the contract remains ADR-0040, not the external sketch's incidental spelling.

| Unit | Deliverable / write boundary | Required proof before the next unit |
|---|---|---|
| U1 | Neutral registered identity in types; immutable safety/snapshot data; query/relation views, ordered attempts and explicit lookup adapters; no source producer | Direct real-query lifecycle tests below; unchanged source behavior |
| U2 | Complete constraint/default use metadata at existing producers and descriptor rewrites | Actual ordinary/retained-class descriptors; exact hash/remap/erasure/circular-clear behavior |
| U3 | Current-binder/manifest admission join and complete const-copy use plans | All intended 22-file producers selected; explicit future-use/authentication rejection; no registration on rejection |
| U4 | Full/partial native completion and default continuations | Original/active/completed identity, violations, exact reverse recovery and default controls; differential gate |
| U5 | Header/call/early-assignment field service and atomic source cutover | Real CheckerSession timing, no reporting replay, held effects once, all 37 rows and clean reads |
| U6 | Independent integration review and unchanged-tree closure | Complete feature gates; no model-only or static-table acceptance |

U1 starts only after M2 acceptance and authentication of the integration base. Neutral model IDs stay
in `typokat-types`; source keys, permits and continuations stay checker-owned. Extend the existing
combined `PublishedClassLookup` with a required snapshot accessor and explicit empty adapters for
genuinely classes-only contexts. Preserve its class/alias publication token; track callable domain and
persistent availability separately. No second generic lookup parameter, mutable publication table,
new source executor or native-fixer algorithm belongs in U1.

#### U1 leader direct-test specification

Use the real Interner, relation engine and SemanticQueryCoordinator. A model test may supply validated
immutable field facts, but must not impersonate a lexical source executor. These are acceptance
assertions to encode in the direct test cluster, not claims that tests already exist or pass.

| Test boundary | Required observation |
|---|---|
| Safe transport | Copy, identity and unknown assignment preflight the full original closure; every shape field remains missing |
| Hidden dependency | Poison/unavailability in an unused bound, return component or erased argument defeats identity/top/warm enclosing success |
| Authentication | Wrong domain, argument arity, endpoint and binder alignment fail distinctly from a valid missing field |
| Ordered request | Each missing binder/parameter/return/bound read returns one exact application/field request, never a native batch or verdict |
| Receiving transaction | Ready nested work followed by a missing field restores pre-existing and overwritten query entries, including ancestor cache state |
| Snapshot persistence | Reuse one query state across old/new/old/new snapshots; each sees only its own missing, completed or failed answers |
| Partial field availability | A completed return tuple is usable while the descriptor remains absent; no partial FunctionType is interned |
| Bound ownership | A selected-owned parameter inside that tuple consults snapshot ownership before Store; Missing requests a bound, Absent does not |
| Completion identity | Repeated completed reads return the same TypeId with no allocation/query; conflicting publication cannot overwrite a field |
| Ancestor caches | Success using pre-ready fields cannot promote its own or enclosing tuple/object relation into durable completed-callable caches |
| Inspection | Rendering is inert; an unpermitted query retains its source request and cannot mint execution authority |
| Transport/census | Equal applications hash equally; changed domain/registration/argument differs; substitution preserves identity and real outer edges; invalid fork/import domains fail |

Predeclare `RegisteredCallable` as a new append-only tag and adjudicate each guarded production
match, including rendering and reference traversal. The already-present `WideningLiteral` guard drift
is a separate baseline obligation, not permission to rebaseline both tags blindly. Source requests
must propagate losslessly through source-capable attempt APIs; no panic, unreachable branch, fake
exhaustion or successful eager fallback stands in for unimplemented field service. Public source
registration remains unreachable in U1, so its source diagnostic gate must be exactly unchanged.

M2 follows [ADR-0032](../decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md):
expression result, assigned source and declared-target transfer are distinct; keep non-union declared
surfaces and whole-source union coverage. Exhaustion is not successful reset. Flow discharge is
query-local; no duplicate semantic walk, durable provisional answer or whole-Pass snapshot.
The current bounded contract is `m2-min-current-contract.md`: seven declared-target transfer rows and
one compound-result row, all reproduced on `028486f4…`. `flow_target_reduction.ts:15` consumes an
already-published exact declaration and needs no future-local producer. Preserve all mixed-fixture
assertions and existing 51/76 dispositions. `assignment_operation_result_modes.ts` pins arithmetic,
string concatenation and logical result boundaries before implementation. Carry reduction exhaustion
through the existing flow resolver and identifier adapter; never cache failed child or parent results.
No new RHS traversal, declaration scheduling, loop solver or post-publication flow repair is authorized.
Independent step review accepts this bounded implementation with one walk-local pending flow-write
table and one enclosing query transaction. Commit only successful outer demands; discard pending writes
before native retry or exhaustion, and always restore loop provisional/depth state. Preflight both
endpoints through the coordinator even for non-union recovery or identity. Arithmetic compound reads
use the target's pre-RHS flow position exactly once; extract result classification without enabling
deferred compound diagnostics. Existing identifier demand reporting remains the sole failure owner.
M2 candidate `0e83daab…` fixes the eight required rows and four neighbors, but independent review is
NO-GO: boolean constituents inside unions lose exact false/true recovery, and correct object narrowing
exposes an old excess/missing-property double report. Repair both at their current owners; the two
`assignment_{boolean_union_modes,excess_diagnostic_ownership}.ts` fixtures pin 14 strict-tsc markers,
clean reads, broad/fresh controls and independent RHS errors. Do not weaken reduction or suppress
committed diagnostics. The typed flow transaction/loop cleanup review found no additional defect.

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
| [120](../backlog/120-semantic-constraint-construction-order.md) | General construction breadth beyond accepted ADR-0040; rejected R1b planners and general C1b–C4 remain excluded |
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
