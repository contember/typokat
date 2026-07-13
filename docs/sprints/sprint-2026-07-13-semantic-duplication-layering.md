# Sprint — semantic duplication and layering (2026-07-13)

**Goal.** Remove approved duplication at narrow semantic seams and eliminate only measured repeated
execution, without merging policy-bearing checker machines or changing observable TypeScript
semantics.

**Theme.** Similar-looking code is not automatically the same operation. This sprint first records
which effects each path owns, then shares only neutral mechanics. Code deduplication is accepted on
behavioral equivalence; execution optimizations need separate operation-count or timing evidence.

**Prerequisite satisfied.** The rewrite/hotpath sprint's WU7 and WU8 are complete and archived; its
three evaluator walkers are now independently hardened and are a boundary this sprint preserves,
not an active blocker —
[`../archive/sprint-2026-07-13-rewrite-hotpath-hardening.md`](../archive/sprint-2026-07-13-rewrite-hotpath-hardening.md).

## Refs re-verified at HEAD (2026-07-13, `24881c7`)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The archived rewrite/hotpath outcome records WU7/WU8 complete, independently reviewed, and
  ratcheted; it explicitly keeps `InferRewrite`, `InferenceConstraintEvaluator`, and
  `MappedRewrite` as three private machines — `docs/archive/sprint-2026-07-13-rewrite-hotpath-hardening.md:3-15`,
  `:202-236`.
- ⚠ Class signatures are lowered during `collect_class_own_members`, then member bodies call the
  ordinary function reserve/fill machine and re-lower signatures. Static methods subsequently
  delete duplicate signature-level `TK2302` diagnostics by span while retaining body diagnostics —
  `src/check/checker/classes/mod.rs:363-657`, `src/check/checker/classes/members.rs:83-173`.
- ✔ Class member meaning is spread across deliberate, differently ordered walks: overload grouping,
  parameter-property construction, member-kind classification, abstract completeness, override
  spans, and body checking — `src/check/checker/classes/mod.rs:379-657`, `:680-760`, `:883-1014`,
  `src/check/checker/classes/inheritance.rs:121-155`.
- ⚠ `tests/manifest.rs` and `tests/surface.rs` each implement the same string/array TOML subset, but
  their table names, result models, and unknown-header cursor behavior are not byte-for-byte
  identical — `tests/manifest.rs:26-152`, `tests/surface.rs:34-156`.
- ⚠ Parameter loops repeat name/annotation/optional/default/rest shaping across strict signature,
  permissive signature, contextual arrow, and body-binding paths. Their effects differ: strict
  failure, error recovery, contextual fallback, declaration publication, destructuring access
  checks, and initializer checking — `src/check/checker/annotations/functions.rs:196-261`,
  `src/check/checker/annotations/signatures.rs:186-222`, `src/check/checker/calls.rs:1834-1895`,
  `:1949-2050`.
- ✔ Single-file and project checking duplicate prelude parse/reserve/fill/check/handoff code while
  intentionally diverging afterward in module setup and scheduling — `src/check/checker/mod.rs:80-202`,
  `:274-430`. Intrinsic marker seeding already has one helper for project mode at `:686-706`, while
  single-file mode still spells the same table inline at `:152-169`.
- ✔ Call and construct overload selection duplicate the same speculative loop, first-constraint
  retention, arity aggregation, committed rebuild, and final diagnostic choice. Receiver semantics
  are the intentional difference — `src/check/checker/calls.rs:451-544`, `:1430-1521`.
- ⚠ Candidate selection currently rebuilds the winning candidate after a speculative match and the
  archived counters prove contextual rewalks, but speculative inference also touches diagnostics,
  type-parameter allocation, interning/evaluation, and relation work. No reuse boundary was approved
  by that measurement — `src/check/checker/calls.rs:482-528`, `:1460-1506`, archived sprint WU4c.
- ✔ Conditional `infer` freshens and rewrites both the extends operand and true branch before knowing
  whether the branch is selected — `src/check/checker/eval/extends.rs:527-577`. Inference constraints
  are demand-evaluated independently while fixing signature parameters — `src/check/infer/mod.rs:719-790`.
- ✔ Mapped replacement and constraint evaluation expose test-only counts but have intentionally
  different cycle, memo, pending, budget, and identity policies. The binding invariant forbids a
  generic walker that obscures those boundaries — `src/check/checker/eval/mapped.rs:3-223`,
  `:712-1068`, `src/check/checker/eval/extends.rs:38-129`,
  `docs/reference/invariants.md:23-33`.
- ✔ The mandatory build loop remains spec → delegated implementation → different independent
  adversarial reviewer → leader verification/commit — `docs/reference/dev-method.md:20-50`.

## Semantic operation matrix

The matrix is the review contract. A shared implementation may cover only the **neutral seam**;
the listed policy/effects stay with their caller.

| Surface | Duplication class | Neutral seam permitted | Policy/effects that must stay local | Proof before sharing |
|---|---|---|---|---|
| Class members | code duplication, repeated signature lowering, and recursive application publication | immutable `ClassInstance` plus a lifetime-free member callable surface consumed by the body pass | declaration-SCC construction, one-layer projection, class/static type-param barriers, overload source order and implementation hiding, parameter properties, visibility/nominal metadata, initializer/body timing, ordered diagnostic replay | class characterization + recursive-application corpora and exact diagnostic identity/order/cardinality |
| Test TOML | code duplication only | quoted string/array value parser and, only if equivalent, a parameterized table scanner | schema names, validation rules, paths, error aggregation and cursor recovery | both existing fixture suites retain exact accept/reject/error behavior |
| Parameters | code duplication; some callers also re-lower | pure parameter shape/enumeration helper | strict vs recovery policy, contextual target, `DeclTypes` writes, destructuring access, default-expression check timing | direct shape tests + function/arrow/method/constructor corpus |
| Prelude bootstrap | code duplication only per public entry path | parse/reserve/fill/check and lifetime-free handoff for one trusted prelude | single-file vs project module construction, import/export placeholders, module scheduling, user diagnostic channels | single/project prelude, shadowing, intrinsic-marker and declaration-index tests |
| Call/construct selector | code duplication **and** repeated candidate execution | common ordered selection control flow with an explicit receiver adapter | call receiver trial/final diagnostics, type-argument syntax source, constraint/arity/no-overload order, speculative vs committed effects | shared-selector parity corpus and effect ledger; no reuse in this WU |
| Conditional `infer` | repeated execution, not a helper-extraction target | defer true-branch rewrite until a matched branch demands it | fresh binder allocation/order, extends rewrite and relation, memo/cycle/budget behavior | false-heavy operation counter plus true/false semantic controls |
| Mapped/constraint walkers | similar structure and possible repeated execution | measurement helpers/counters only | all walker traversal, cycle, memo, pending, exhaustion, identity and cleanup policy | reviewed counter placement; no shared walker |

## Performance and equivalence gates

- **Code-duplication WUs (WU1-WU5):** no speedup claim is required. Diagnostics, incomplete records,
  obligations, returned `TypeId` structure, declaration publication, source order, and test-only
  operation counts must be unchanged unless the WU's spec explicitly says otherwise. Relevant
  release microcorpora may regress by no more than 2%; a noisy timing result is not evidence of
  regression when deterministic operation counts are unchanged.
- **Repeated-execution WUs (WU6-WU7):** select an optimization only if it removes at least 20% of
  the directly relevant deterministic operations at both 10k and 100k scale, or improves the
  isolated high-resolution median by at least 10% over five fresh-process repetitions. Every
  paired control must show no more than 2% regression in its deterministic operation count (or the
  same high-resolution timing metric when no direct count exists).
- Graph construction, parsing, and fixture generation happen outside timed regions. Record raw
  samples, median/range, environment, revision, exact command, and counter formulas in the run log.
  If a candidate misses its gate, record it and ship no production optimization.

## Work units

### WU0 — characterization corpus and speculative-effect ledger (effort L; spec first)

- **Problem.** A deduplication can preserve types while changing diagnostic cardinality, binder
  lifetime, source order, or speculative side effects. Those are not fully pinned together today.
- **Verify first.** Run focused existing class/call/prelude/evaluator tests and compare new syntax
  probes with `tsc 6.0.3 --strict`. Inventory every mutation performed by speculative candidate
  construction and trial checking.
- **Scope.** Commit behavior-neutral acceptance fixtures before production changes. Pin generic
  method binder scope; exact static `TK2302` cardinality and span; unresolved parameter/return/default
  diagnostics; method and constructor overload order plus implementation-signature hiding;
  constructor parameter properties; current externally visible omitted-return/`void` behavior;
  call/construct constraint, arity, receiver, and no-overload ordering; single/project prelude
  shadowing and intrinsic identity; conditional false/true `infer` paths. Add direct tests where
  display text cannot prove identity or ordering. Record a candidate-effect ledger covering at least
  diagnostics, incomplete records, obligations, type-parameter allocation, interner growth,
  conditional memo/evaluator state, relation queries, contextual AST walks, and test counters.
- **Acceptance / witness.** One separately committed spec change precedes WU1-WU7 implementation;
  current behavior is fully characterized, strict-`tsc` differences are classified, and no
  unowned false negative is accepted. The ledger names which effects are rollback-safe, replayed,
  monotonic, or unknown; any unknown forbids candidate reuse.
- **Review.** An agent independent of later implementation reviews the corpus for false-negative
  blind spots and cross-checks the syntax probes with strict `tsc`.
- **Touch points.** `tests/cases/sr_semantic_duplication/`, focused existing test modules,
  `tests/conformance.rs`, `tests/cases/README.md`, and this sprint's run log. No production source.

### WU0A — class-application architecture gate (effort L; decision/spec only)

- **Problem.** WU0 exposed that the proposed reserved callable surface would sit on the current
  mutable class-object/general-`Instantiation` recursion seam. Extending reservation to mutable
  function rows would violate hash-consed `TypeId` identity, while eagerly constructing concrete
  class applications would diverge on `Box<T[]>`.
- **Verify first.** Trace recursive class applications through objects, collections, callbacks,
  unions/intersections, deferred conditional/mapped types, indexed access, generic method metadata,
  constructors, mutual declaration order, and repeated relation demands. Cross-check every public
  syntax witness with `tsc 6.0.3 --strict` and review the proposal specifically against the type-store
  and provisional relation-cache invariants.
- **Scope.** Add the disabled recursive-class-application acceptance fixture and record the selected
  immutable `ClassInstance` design in
  [`ADR-0006`](../decisions/0006-immutable-class-instances-and-scc-publication.md): a finite
  checker-side declaration graph, no-evaluation/no-relation surface mode, SCC publication barrier,
  an explicit immutable `DeferredIndexedAccess`, bounded one-layer projection planning before a
  read-only relation phase, query-local projection/evaluation overlays, typed exhaustion,
  heritage-only poison propagation, transactional normalized cache/cycle keys, and ordered
  diagnostic events. Explicitly reject mutable function rows. This unit changes no Rust and does not
  enable the corpus.
- **Acceptance / witness.** The new fixture covers every listed composite and demand path, mutual
  references in both declaration orders, direct recursive relation in both source/target orders,
  non-regular recursion, private/protected nominal origin, constructor parameter properties/
  overloads, static cardinality and deliberate mismatches. Its 58 markers have exact line/code
  parity with strict tsc; marker parser, divergence and docs validators pass, and no unowned false
  negative is accepted.
- **Review.** Architecture review must attack infinite application-graph expansion, partial SCC
  visibility, cache poisoning, hidden evaluator work, loss of structural class relation, and
  diagnostic duplication/order before WU1 production edits begin.
- **Touch points.** `tests/cases/sr_semantic_duplication/`, `tests/cases/README.md`,
  `docs/decisions/`, docs indexes, and this sprint only. No production source or Rust tests.

### WU1 — immutable class-application rollout (effort XL; staged umbrella)

- **Problem.** Class fill builds callable member signatures, but the body pass calls
  `infer_function` and lowers them again; static methods then filter duplicate `TK2302` records. The
  current mutable reserved class object and alias-style instantiation fallback cannot soundly carry
  the recursive application surfaces characterized by WU0A.
- **Verify first.** Trace each WU0/WU0A method, constructor and class application through reserve,
  declaration graph construction, SCC publication, one-layer projection, body checking and relation.
  Record signature-lowering count, projection count, source order, overload visibility, type-param
  frame, relation/cycle keys and diagnostic replay points.
- **Hard stops.** No mutable function row, partially visible class projection, transitive concrete
  application expansion, relation/evaluation during surface construction, span-based diagnostic
  deletion, declaration-only relation cycle key, hidden mutable interner inside `Relater`, widened
  relation-cache key, or cache/memo write from a provisional, unpublished, binder-aligned or
  projection/evaluation-exhausted query. No durable evaluator result may be the only way read-only
  relation sees a normalized operand, and no generic boolean helper may fold `Exhausted` into `No`.
  Stop and re-review if any stage cannot preserve static barriers, generic method binder identity,
  overload implementation hiding, constructor parameter-property ownership, private/protected
  origin, heritage-poison boundaries, or ordered diagnostics.
- **Stage rule.** WU1a-WU1d are separate spec-first implementation/review units. Each begins with
  focused direct gates, lands atomically, and receives an independent false-negative/cache review
  before the next stage. The disabled conformance directory is enabled only by WU1d; an earlier
  stage must use direct Rust tests without weakening or partially enabling the acceptance fixture.

#### WU1a — immutable representations and construction capabilities (effort L)

- **Scope.** Add the distinct hash-consed `ClassInstance` tag/payload/side table and the immutable
  `DeferredIndexedAccess { object, index }` form. Cover structural hash/equality, substitution,
  rendering and every identity walker. Add the construction-state/capability API so evaluator,
  projection and relation entrypoints that can encounter a class instance require published state;
  `ClassSurfaceBuilder` exposes none of those operations. Do not yet redirect class consumers.
- **Direct gates.** Prove equal complete application keys deduplicate, different class identities or
  ordered arguments do not, and `ClassInstance` never enters alias-`Instantiation` dispatch. Prove
  deferred indexed access hashes/substitutes/renders by both ordered children, evaluates only one
  demanded outer layer, and is identical-only while unevaluated. Direct API tests pin
  `DemandOutcome<T> = Ready(T) | Exhausted` and
  `RelationOutcome = Yes | No | Exhausted`; exhaustive matching must not expose a binary
  exhaustion-folding helper. Every public evaluation, projection and relation entrypoint must reject
  a pre-publication application; test tripwire counts and durable cache/memo writes remain exactly
  zero.
- **Review / touch points.** Independently audit all type-tag matches and walkers for an accidental
  alias fallback or missing child. Touch only the type representation/interner/substitution/display,
  the narrow evaluator form, capability definitions and direct tests.

#### WU1b — declaration graph, SCC publication, heritage, and events (effort XL)

- **Scope.** Move mutability into the checker-owned finite declaration graph. Build surfaces in
  no-evaluation/no-relation mode; attach typed pending obligations/events; extract every identity
  edge named by ADR-0006; process the SCC condensation DAG dependency-first; publish non-heritage
  SCCs atomically; reject/poison heritage cycles; propagate poison only through derived heritage
  edges; compose acyclic published bases before derived publication; and freeze class type-parameter
  constraints/default side columns at publication. Add the total diagnostic-event key and retain
  each method/constructor callable surface for later body reuse.
- **Direct gates.** One edge-extraction table must exercise every identity child, including nested
  static, constructor overload/parameter-property, `ClassInstance` argument/target and deferred
  indexed-access operands. Prove both mutual declaration orders publish the same complete SCC;
  base-before-derived composition is stable; a cyclic heritage SCC exposes no partial members and
  owns its cycle incomplete reason. One- and two-level derived chains from that SCC must each remain
  unpublished and own one `class/class-heritage/poisoned-base` event at their respective `extends`;
  an otherwise identical ordinary property/signature reference must publish and must not propagate
  poison. A frozen type-parameter descriptor rejects later mutation. Construction tripwires remain
  zero. Synthetic shuffled completion must replay events by
  `(module_ordinal, source_start, event_ordinal, record_ordinal)`.
- **Review / touch points.** Independently inspect the complete graph-child list, condensation order,
  poison propagation, state transitions, pending obligation ownership and one-time surface lowering.
  Touch class construction/declaration scheduling, event storage/replay and direct tests; do not add
  relation normalization yet.

#### WU1c — bounded demand projection and read-only relation protocol (effort XL)

- **Scope.** Add pass-local application-to-projection memoization and the explicit
  `ProjectionPlanner -> ProjectionPlan -> Relater` protocol from ADR-0006. Each public query gets a
  fresh 128-distinct-application budget; the planner interns missing one-layer projections before
  relation and records demanded conditional/mapped/`keyof`/alias/deferred-indexed evaluations in a
  query-local `TypeId -> TypeId` overlay. Relation sees only immutable `Store` plus that overlay and
  normalizes the concrete operand pair before the unchanged three-word cache/cycle key. Buffer
  evaluator, projection-memo and relation writes transactionally and discard every write when
  exhaustion or another existing non-cacheable context taints the plan/query.
- **Direct gates.** Count one projection per complete `ClassInstance TypeId` and prove memo hits reuse
  the exact projection id without buying depth in a later query. Before any durable memo commit,
  read-only relation must consume query-local evaluated ids for deferred indexed access,
  conditional, mapped, `keyof` and alias results; a probe where the raw deferred ids would mismatch
  must relate through the overlay. Evaluator/projection memo lengths remain unchanged after planning
  and during relation, then advance only at the explicit successful transaction commit. Alternating
  bad/good and repeated same-pair whole-type relations must be order-independent in both directions.
  A non-regular chain admits exactly 128 distinct
  applications and returns typed `Exhausted` at the 129th with
  `incomplete[relation/class-projection-budget]`; a sibling mismatch after demanded exhaustion must
  not replace that outcome. A mismatch proven before an unreachable frontier remains `No`. Both
  cases leave durable evaluator/projection/relation cache insert counters unchanged when planning
  discovered exhaustion. Conditional-extends exhaustion preserves the exact original deferred
  conditional `TypeId`, selects neither branch and writes no conditional memo. Cycle-stack probes
  must show concrete normalized pairs, never a declaration `ClassId`.
- **Review / touch points.** Independently audit ownership and borrow boundaries, budget accounting,
  memo visibility, taint propagation and every cache promotion. Touch only projection planning,
  class-aware relation normalization, required evaluator transaction seams and direct tests.

#### WU1d — consumer integration, callable reuse, and corpus enablement (effort XL)

- **Scope.** Route annotation/member/index/`new`/call, inheritance, contextual typing,
  destructuring, `keyof`, static access and constructor consumers through the published projection
  API. Reuse the retained binder frame, receiver, parameters, declared return and overload records
  for body checking without rebuilding the public callable type. Preserve separate field/accessor
  behavior and ordinary function hoisting. Enable the disabled semantic-duplication corpus only in
  this final stage.
- **Direct gates.** Exercise contextual callbacks, `const { value }` destructuring, `keyof` over a
  class instance, static access and class-type-parameter barriers, constructor overload/access/
  parameter-property paths, acyclic inheritance composition, cyclic-heritage rejection and both
  poisoned-derived chain depths. Pin private/protected origin through nested carriers. Final
  assignment/argument obligations convert typed exhaustion to conservative `No` plus one incomplete
  record. An exhausted inference contributes no candidate/substitution. An exhausted
  first overload candidate aborts selection and does not choose an otherwise matching later
  overload; an earlier definitive winner still avoids later candidates. Member/index/`keyof`/call/
  construct shape paths propagate exhaustion to their explicit recovery boundary rather than using
  a partial/error shape. Every method/constructor signature is lowered once.
  `class_diagnostics_preserve_current_raw_vector_order` is replaced by the exact six-record
  source-event vector required by ADR-0006; five static `TK2302` events remain distinct and lexical.
  Both WU0/WU0A fixtures then pass, including all 58 recursive-application markers, and ordinary
  structural/nominal behavior remains unchanged.
- **Review / touch points.** A different adversarial reviewer hunts dropped errors, duplicate
  diagnostics and order dependence across every fixture demand path, inspects projection/cache
  counters, and cross-checks fresh probes with strict tsc. Touch class consumers/body reuse and the
  conformance enablement; do not redesign unrelated evaluator, relation or function-hoisting paths.

### WU2 — extract the test-only TOML subset helper (effort S)

- **Problem.** Two integration validators maintain nearly identical value and table parsing.
- **Verify first.** Diff parser behavior for unknown headers, keys before a header, duplicates,
  malformed values, multi-error aggregation, strings and arrays across both fixture suites.
- **Scope.** Extract only behavior proven identical into a test-support module. Keep manifest/surface
  schema models and validators local. If table scanning differs, share `Value`, `Table`, and
  `parse_value` only rather than adding policy flags to force a larger abstraction.
- **Acceptance / witness.** `cargo test --test manifest` and `cargo test --test surface` preserve all
  positive/negative fixture results and error substrings; no production dependency or general TOML
  parser is introduced.
- **Review.** Lightweight independent diff review confirms the helper did not weaken either schema.
- **Touch points.** `tests/manifest.rs`, `tests/surface.rs`, and a narrow `tests/support/` helper.

### WU3 — share neutral parameter shape mechanics (effort M)

- **Problem.** Four parameter-lowering loops repeat AST enumeration and shape construction while
  mixing it with intentionally different checker effects.
- **Verify first.** Build a caller matrix for ordinary/rest, optional/defaulted, missing/unresolved
  annotation, destructuring, contextual target, and declaration binding. Record which caller emits,
  recovers, binds, or checks executable defaults.
- **Scope.** Share only pure enumeration/name/shape mechanics (including one
  `parameter_from_shape`). Leave annotation lowering and every mutable checker effect at explicit
  caller sites. Keep strict `Option` failure distinct from recovery to the error type.
- **Stop gate.** Stop if the helper requires policy booleans/closures that obscure diagnostic,
  binding, contextual, or initializer timing; smaller duplication is preferable to a policy engine.
- **Acceptance / witness.** WU0 parameter/class corpus and existing signature tests retain exact
  behavior; no cast/`any` workaround; generated `ParameterType` order and flags are identical.
- **Review.** Independent reviewer compares all four callers against the operation matrix.
- **Touch points.** `src/check/checker/annotations/{functions,signatures}.rs`,
  `src/check/checker/calls.rs`, and focused tests.

### WU4 — centralize trusted prelude bootstrap (effort M)

- **Problem.** Single-file and project entry paths duplicate the trusted prelude lifecycle and have
  already drifted on intrinsic seeding shape.
- **Verify first.** Snapshot prelude type-declaration indices, `DeclTypes`, next type-param id,
  intrinsic markers, clean diagnostics, and user shadowing for both entry paths.
- **Scope.** Extract one bounded bootstrap/handoff routine for parse, reserve, fill, flow, check,
  lifetime-free type-param placeholders, resolved types, values, next id, and intrinsic seeding.
  Keep project binder construction, import/export placeholders, per-module fill, and user-pass
  scheduling outside it.
- **Stop gate.** Stop if the helper requires moving an OXC AST across lifetimes/threads, changing the
  run-local type universe, or creating a general compilation-unit abstraction.
- **Acceptance / witness.** WU0 prelude tests pass in single/project modes; declaration indices,
  shadowing, values and marker ids are identical; prelude diagnostics remain internal and clean.
- **Review.** Independent reviewer audits slot shadowing and handoff identity, not just test green.
- **Touch points.** `src/check/checker/mod.rs` and existing prelude/utility tests only.

### WU5 — unify call/construct selection control flow (effort L)

- **Problem.** Call and construct selectors duplicate a large ordered control-flow machine and can
  drift in failure precedence, yet call receivers are a real semantic difference.
- **Verify first.** Use WU0 to compare single-signature, overload match, constraint failure, mixed
  arity/mismatch, explicit/defaulted generic argument, contextual callback, and receiver cases.
- **Scope.** Extract one ordered selector kernel over an explicit request that supplies type
  arguments and receiver semantics. Keep signature discovery and final return handling in the call
  and construct wrappers. Preserve speculative build → trial → committed rebuild exactly.
- **Hard stop.** No candidate, contextual-result, diagnostic, relation, or inferred-map reuse in this
  WU. Any reuse proposal stops until WU0's effect ledger proves complete rollback/replay semantics
  and receives separate approval.
- **Acceptance / witness.** WU0 selector diagnostics, source order, receiver checks and operation
  counters are unchanged; the call and construct wrappers both exercise the shared kernel; no cache
  key or relation behavior changes.
- **Review.** Independent adversarial reviewer focuses on first failure, diagnostic cardinality,
  speculative leaks, callback rewalks, receiver absence/presence, and false negatives.
- **Touch points.** `src/check/checker/calls.rs` and narrow call measurement/tests.

### WU6 — lazily rewrite conditional-`infer` true branches (effort M)

- **Problem.** `run_extends_test` rewrites the true branch before the extends test, even when the
  false branch wins.
- **Verify first.** Add deterministic counts for infer-rewrite visits on false-heavy and true-heavy
  conditional corpora. Confirm the caller never observes the returned true branch on a failed test,
  and pin next-type-param allocation plus memo/cycle/budget outcomes.
- **Scope.** Defer only true-branch infer rewriting and final substitution until a successful extends
  test demands the branch. Keep fresh binder allocation, extends rewrite, candidate collection,
  relation, and selected-branch semantics unchanged.
- **Performance gate.** The false-heavy corpus must clear the repeated-execution gate; true-heavy and
  no-`infer` controls must not regress beyond the control threshold.
- **Stop gate.** Stop if laziness changes fresh binder ids visible to later work, evaluator memo
  entries, cycle/exhaustion behavior, or diagnostic ordering.
- **Acceptance / witness.** Strict-`tsc`-cross-checked WU0 cases are identical; false branches perform
  no true-branch rewrite visits; true branches retain exact inferred types and failure behavior.
- **Review.** Independent evaluator reviewer checks binder scope, cycle paths, unchanged identity and
  counter placement.
- **Touch points.** `src/check/checker/eval/{extends,instantiation,tests}.rs`, WU0 fixtures, and
  test-only measurement fields.

### WU7 — mapped/constraint execution measurements only (effort M)

- **Problem.** Similar private walker code and repeated visits are visible, but neither code shape nor
  raw `TypeId` repetition proves a sound reuse boundary.
- **Entry condition.** WU6 is committed and independently reviewed so this unit measures one settled
  `extends.rs` implementation and does not overlap edits to its shared tests.
- **Verify first.** Reuse the archived WU4b baselines, then add paired mapped-property fanout and
  inference-constraint shared-DAG/cycle/exhaustion corpora at 10k and 100k scale.
- **Scope.** Measure root calls, child visits, memo hits/inserts, re-entries, pending evaluations,
  identity returns, re-interns, exhaustion and per-property mapped contexts. Keep counters
  `cfg(test)` and construction outside timing. This WU selects or rejects future candidates; it does
  not implement one.
- **Hard stops.** No universal `TypeId` walker; no global/durable `TypeId → TypeId` cache; no cache
  shared across mapped properties or inference-constraint evaluations; no transfer of SCC taint,
  pending, exhaustion, budget, or partial-cycle policy between walkers.
- **Acceptance / witness.** Reviewed raw counters and controls explain repeated work without claiming
  an optimization. Any future candidate must name its complete context key and cleanup boundary,
  clear the performance gate, and receive explicit approval as a new WU/backlog item.
- **Review.** Independent reviewer validates every counter against the actual production edge and
  checks that instrumentation does not create a second traversal.
- **Touch points.** Test-only counters/microcorpora in `src/check/checker/eval/{mapped,extends,tests}.rs`
  and this sprint run log. No production behavior change.

### WU8 — independent final review, ratchet, and closure (effort L)

- **Problem.** Individually plausible deduplications can compose into reordered diagnostics,
  binder leakage, or a false-clean path.
- **Verify first.** A reviewer independent of WU1-WU7 starts from WU0 and audits the whole diff
  against the semantic operation matrix and binding invariants.
- **Scope.** Hunt false negatives and order/cardinality drift across class members, prelude
  shadowing, parameters, call/construct selection and conditional infer; validate all measurement
  claims and stop gates. Return concrete PASS/FAIL repros; failures go back to the responsible
  implementation agent and are re-reviewed.
- **Acceptance / witness.** `cargo fmt --check`; focused test crates; `cargo test`; `cargo clippy
  --all-targets -- -D warnings`; `cargo build --release`; selected release measurement corpora; and a
  fresh pinned official-suite `run --check` all pass with zero regressions/missing entries. Record
  commit map, raw measurements, reviews and deferrals in the outcome section, then archive and update
  indexes.
- **Touch points.** Read-only whole sprint diff, focused regression fixtures for confirmed failures,
  official-suite ratchet, sprint outcome/archive, and affected living reference docs only if current
  architecture wording changed.

## Out of scope (explicit)

- A universal type visitor/rewriter or shared evaluator task machine. The three hardened walkers'
  different policies are binding architecture, not cleanup debt.
- Global relation, substitution, evaluator, inference, mapped, or constraint caches keyed only by
  `TypeId`; cross-run caching, stable structural hashing, and incrementality remain separately owned.
- Candidate-local reuse, speculative transactions, or rollback machinery without the completed WU0
  effect ledger and separate explicit approval.
- Changes to relation cache width/kinds, provisional-cycle policy/reason chains, CFG narrowing,
  class nominal identity, diagnostic codes, or deliberate divergence policy. WU1 owns only the
  ADR-0006 class-projection normalization before the unchanged three-word relation key.
- A general TOML crate/dependency, a general compilation-unit/prelude framework, a bytecode VM, or
  cosmetic extraction whose only result is fewer lines.
- Optimizing mapped/constraint work in WU7. Measurement may propose follow-up ownership; it does not
  authorize implementation.

## Decisions

1. Classify every candidate as **code duplication**, **repeated execution**, or both. Line-count
   reduction uses equivalence gates; performance claims use measurement gates.
2. Share neutral data/mechanics, not semantic policy. If an abstraction needs many mode flags or
   mutable callbacks, retain local code and document parity instead.
3. The archived rewrite/hotpath sprint is closed. Its WU7/WU8 result is the entry baseline and the
   reason mapped/constraint walkers remain separate; there is no active-WU dependency.
4. Candidate selection may share control flow, but its speculative and committed executions remain
   distinct until a complete effect ledger proves reuse sound and the user separately approves it.
5. A raw repeated `TypeId` is never sufficient cache identity. Binder scope, substitutions,
   mapped-property value, pending evaluation, budget/exhaustion, cycle state, and cleanup lifetime
   must be explicit where applicable.
6. Every checker-affecting WU starts from a separate spec commit and ends with review by an agent who
   did not implement it. Review hunts false negatives first and cross-checks fresh syntax probes with
   `tsc 6.0.3 --strict`.
7. Class applications follow
   [ADR-0006](../decisions/0006-immutable-class-instances-and-scc-publication.md): immutable
   application nodes distinct from alias instantiation, immutable deferred indexed access, finite
   declaration-SCC publication with heritage-only poison propagation, a 128-application projection/
   evaluation overlay before read-only relation, typed `Yes | No | Exhausted` outcomes,
   transactional normalized relation keys and ordered diagnostic events. Mutable function rows are
   forbidden.

## Sequencing

| Order | Unit | Gate |
|---:|---|---|
| 1 | WU0 | Behavior-neutral spec commit and reviewed effect ledger before production edits. |
| 2 | WU0A | Architecture/spec commit and independent cache/termination review before class production edits. |
| 3 | WU1a | Establish immutable representations and enforce construction capabilities before graph work. |
| 4 | WU1b | Publish complete declaration SCCs and event ownership before any projection/relation consumer. |
| 5 | WU1c | Land bounded projection planning and cache transactions before class consumers switch over. |
| 6 | WU1d | Integrate consumers/reuse, pass both class corpora, then enable the directory. |
| 7 | WU2 and WU3 | May run independently after WU0; one writer per disjoint file set. |
| 8 | WU4 | After WU1d so the bootstrap helper consumes settled class-surface behavior. |
| 9 | WU5 | After WU0 ledger; selector sharing only, with candidate reuse forbidden. |
| 10 | WU6 | Gated optimization lands and receives independent review before shared evaluator files move again. |
| 11 | WU7 | Measurement-only follow-up starts from committed WU6; no overlapping `extends.rs`/test edits. |
| 12 | WU8 | Independent whole-diff review, full gates, ratchet, outcome and archive. |

Each production WU, including each WU1 stage, is an atomic implementation commit after its own
focused spec/test commit. These remain separate from WU0's characterization spec and WU0A's
architecture/spec commit. The leader verifies and commits; implementation and adversarial review
use different agents. No failing performance candidate is folded into a neighboring cleanup commit.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-13 — Plan grounded at clean HEAD `24881c7`. The rewrite/hotpath sprint is archived with
  WU7/WU8 complete; this sprint treats its private-walker boundary and measurements as shipped input.
- 2026-07-13 — WU0 selector probes were cross-checked with `tsc 6.0.3 --strict`: constraint-before-
  later-arity, pure arity, and receiver cases agree. Mixed mismatch/arity differs only as
  `TK2769` vs `TS2345`; all-constraint failure text preserves typokat's first candidate while tsc
  renders the last. Both cosmetic differences are recorded in `reference/divergences.md`.

### WU0 candidate-effect ledger

This ledger describes one overload candidate build/trial, not just its returned `CallCandidate`.
The labels are operational: **rollback-safe** state is restored before the next candidate,
**replayed** work intentionally runs again for the selected candidate, **monotonic** state persists,
and **unknown** means there is no complete transaction proving isolation.

| Effect | Classification and current sites |
|---|---|
| Diagnostics | **Partly rollback-safe/replayed, otherwise unknown.** Explicit-constraint diagnostics are copied then truncated (`calls.rs:617-633`), the first failure is later appended (`:535-536`, `:1513-1514`), and a winner is rebuilt in committed mode (`:514-528`, `:1492-1506`). Trial excess-property and contextual diagnostics are truncated (`calls.rs:820-835`; `expr.rs:705-743`). But speculative parameter evaluation can emit `TK2589` through `evaluate_type` (`eval/mod.rs:37-65`) without a whole-candidate diagnostic snapshot. |
| Incomplete records | **Monotonic/unknown.** Raw spread records happen once before selection (`calls.rs:170-178`), but candidate-local annotation lowering and contextual AST rewalks can call `record_incomplete`; neither `instantiate_signature_candidate` nor `infer_contextual_source_after_walked` snapshots `Pass::incomplete` (`context.rs:277-281`, `expr.rs:705-743`). |
| Assignment obligations | **Rollback-safe, then replayed.** Speculative contextual rewalks truncate to their saved length (`expr.rs:708-719`, `:731-742`); selected argument checking appends the committed obligations once (`calls.rs:920-963`). |
| Override checks | **Rollback-safe, then replayed.** The same contextual boundaries save/truncate `override_checks` (`expr.rs:708-719`, `:731-742`); any selected contextual body is walked again by committed argument checking. |
| `decl_types` / contextual bindings | **Partly rollback-safe, otherwise unknown.** Speculative contextual arrows clone and restore `decl_types` (`expr.rs:711`, `:720-722`), but the fresh-literal branch restores diagnostics/obligations/overrides only (`:731-743`). There is no transaction covering every binding mutation reachable from a contextual rewalk. |
| `next_type_param` | **Monotonic.** Preliminary/full candidate inference advances the shared counter (`calls.rs:661-718`), and candidate type evaluation can advance it again (`eval/mod.rs:53-59`). Failed candidates are not rolled back, so removing a build changes later binder ids. |
| Interner growth / `TypeId` order | **Monotonic.** Candidate inference, contextual lowering, substitution, `instantiate_function` (`calls.rs:683-721`), and evaluation intern into the run-local store. There is no arena rollback; skipping or reusing a candidate can change later allocation order and identity. |
| Conditional memo/evaluator state | **Monotonic/unknown.** Candidate parameter/receiver evaluation (`calls.rs:732-733`) uses the pass-wide `cond_memo` and shared type-parameter counter (`eval/mod.rs:53-61`). Cycle/exhaustion results avoid durable memoization, but successful speculative results persist and their diagnostic side effects are not transactionally isolated. |
| Relation cache and work | **Ephemeral, deliberately replayed.** Constraint checks, argument trials, and receiver checks construct local `Relater`s (`calls.rs:259-287`, `:837-842`, `:969-1018`); their caches die with each relation phase. Query order still determines work/reason selection and must remain unchanged even though no pass-wide cache is mutated. |
| Contextual AST walks | **Replayed.** Candidate inference rewalks through `contextual_inference_args` (`calls.rs:290-323`), trials rewalk again (`:804-819`), and committed checking rewalks the winner (`:920-963`). The phase split is observable through diagnostics, obligations, bindings, interning, and counters. |
| Test counters | **Monotonic.** Candidate builds/trials, rollback counts, receiver queries, and contextual phase arrays increment at the production edges (`calls.rs:562-569`, `:788-847`, `:969-1018`). `calls_measure.rs` pins the combined call/construct failure formula; reuse would intentionally change this acceptance surface. |
| `flow_memo` | **Monotonic/unknown.** Contextual rewalks can resolve nested references into the pass-wide memo (`flowgraph/nodes.rs:218-283`), while the contextual rollback boundary does not snapshot it (`expr.rs:705-743`). |
| `type_resolved` | **Monotonic.** Every overload can lower explicit annotations (`calls.rs:580-588`), triggering lazy declaration resolution and durable writes (`decls/resolve.rs:23-115`). Resolution order can therefore affect later `TypeId`s and diagnostics. |
| `circular_aliases` | **Monotonic/unknown.** Candidate annotation lowering can discover and persist a surface-cycle verdict (`decls/resolve.rs:122-140`); neither candidate builder nor contextual rollback snapshots the set. |

**WU0 conclusion:** candidate, inference-map, contextual-result, diagnostic, or relation-result reuse is
forbidden. The existing speculative build → trial → committed rebuild sequence is the characterized
contract for WU5; reuse needs a separately approved transaction design covering every monotonic and
unknown effect above.

- 2026-07-13 — The disabled WU0 class corpus is intentionally **RED** against current code: the
  unresolved method parameter and return each appear twice, the invalid generic default appears
  twice, and the unresolved constructor parameter-property type appears three times. These are the
  pre-WU1 reservation/body duplication counts; WU0 makes no production change.
- 2026-07-13 — `tsc 6.0.3 --strict` cross-check: the class probes have verdict parity except the
  backlog-76 external `void` over/under pair; selector probes differ only in the registered cosmetic
  mixed-failure code/span and first-vs-last constraint text. No new unowned false negative was
  accepted.
- 2026-07-13 — Direct characterization now pins five distinct static-binder spans, the current raw
  nine-diagnostic class vector order, single/project trusted prelude identity and shadowing,
  call/construct counters and first-constraint text, plus conditional-`infer` false/true rewrite
  state. The class order witness preserves the current duplicated signature sequence before the
  earlier source-position assignment; it does not choose a new ordering policy.
- 2026-07-13 — Adversarial review initially blocked on ambiguous same-line static spans, shadowing
  masking `Uppercase`, non-discriminating `Uncapitalize`, the missing call-side constraint-order
  mirror, and unpinned raw diagnostic order. Follow-up resolved every item; final WU0 review: **PASS**.
- 2026-07-13 — Leader gates passed: `cargo test` and
  `cargo clippy --all-targets -- -D warnings`. This closes the WU0 test/spec gate only; production
  implementation starts in later work units.
- 2026-07-13 — The pre-WU1 architecture direction was selected at HEAD `8b08d84` →
  [ADR-0006](../decisions/0006-immutable-class-instances-and-scc-publication.md). WU1 no longer
  permits mutable reserved function rows or alias-style class recursion.
- 2026-07-13 — WU0A's disabled recursive-class-application fixture has exact line/code parity with
  `tsc 6.0.3 --strict`: 58 expected diagnostics across every required composite, demand, direct
  relation direction, declaration order, nominal-origin and construction-cardinality path, with no
  additional or unowned false negative. Whole-corpus error/incomplete marker parser checks pass; no
  production code or corpus enablement changed.
- 2026-07-13 — Independent ADR review initially returned **FAIL** on the unbounded non-regular
  relation path, an implicit mutable-interner dependency in relation, incomplete identity-edge and
  heritage publication rules, cache-taint persistence, pre-publication enforcement and rollout
  proof. ADR-0006 and WU1a-WU1d now specify the 128-application planner, read-only relation phase,
  transactional zero-write exhaustion, exhaustive SCC identity edges and direct heritage-cycle
  handling, capability tripwires, immutable deferred indexed access and direct gates. The completed
  targeted review sequence below closes this architecture gate with **PASS**.
- 2026-07-13 — Targeted second ADR review returned **FAIL** on three remaining protocol gaps:
  evaluated deferred nodes were not carried into read-only relation before durable memo commit;
  exhaustion was still folded into binary relation failure; and poison propagation through derived
  heritage chains was unspecified. ADR-0006 and WU1a-WU1d now add a query-local evaluation overlay,
  exhaustive `Yes | No | Exhausted` caller policies, and dependency-first heritage-only poison with
  owned incomplete events and one-/two-level chain gates. Third targeted architecture re-review:
  **PASS** — those three corrected protocols have no remaining pre-WU1 architecture blockers.
