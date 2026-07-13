# Sprint — rewrite and hotpath hardening (2026-07-13)

> **Status:** active  
> **Owner:** one Terra writer; independent reviewers named per work unit  
> **Started:** 2026-07-13  
> **Related:** [architecture](../reference/architecture.md), [invariants](../reference/invariants.md), [development method](../reference/dev-method.md), [divergences](../reference/divergences.md), [backlog](../backlog/README.md), [profiling gate](../archive/backlog-13-profiling-gate.md)

## Goal

Resolve the bounded rewrite correctness risks found by the three deep architecture audits, then make only measured, reversible hotpath reductions. Preserve receiver, generic-binder, metadata, diagnostic-order, cycle, and cache-identity semantics throughout.

The sprint starts with an acceptance corpus and adversarial probes. It does not assume that a repeated traversal is a hotpath, or that a cache is sound merely because it improves a benchmark.

## Audit synthesis — verified at `bc60398`

| Class | Finding | Evidence | Sprint disposition |
|---|---|---|---|
| **A — cosmetic duplication** | The template-hole matchers are deliberately separate in relation and inference. | `src/relate/relation/advanced.rs`; `src/check/infer/helpers.rs:23-46` | Do not extract a helper merely to remove duplicate text; parity is a review concern, not a scheduled refactor. |
| **B — semantic drift** | Computed element-member calls can lose the member receiver or generic inference, especially when parenthesized. | `src/check/checker/calls.rs:250-275`, `296-377`, `497-543` | WU0 probes, then WU1 only. |
| **B — semantic drift** | `substitute_infers` recursively rebuilds selected composites without a dedicated rewrite context and currently treats nested generic metadata as opaque or incidental. | `src/check/checker/eval/instantiation.rs:424-620` | WU0 probes, then WU2. |
| **B — semantic drift** | Evaluating a changed function constraint rebuilds it with `type_params: Vec::new()`, risking loss of binders and their metadata. | `src/check/checker/eval/extends.rs:164-200` | WU0 probes, then WU3. |
| **B/D — diagnostic isolation and cache identity** | Rejected overload trials run contextual walks and relation work; diagnostics are manually truncated, so cyclic/deep receiver and parameter paths need a non-leak probe before any reuse or transaction design. | `src/check/checker/calls.rs:327-388`, `604-654`; `src/check/checker/eval/mod.rs:37-66` | WU0 architecture-stop probe. Do not invent a transaction if it fails. |
| **C — repeated work** | Every relation, even without binder context, constructs a stack key by allocating/merging/sorting four maps. | `src/relate/relation/mod.rs:326-328`, `402-462` | Measure in WU4; WU5 may add the empty-context fast path only. |
| **C — repeated work** | Ordered object properties are repeatedly linear-searched; inference also snapshots names and searches those snapshots. | `src/relate/relation/objects.rs:92-115`; `src/types/repr.rs:334-340`; `src/check/infer/context.rs:226-239`; `src/check/infer/helpers.rs:4-13` | Measure in WU4; WU5 may use ordered two-pointer matching while retaining target-first failure order. |
| **C/D — repeated work with semantic boundary** | Substitution has an `in_progress` guard but no completed-result memo; generic binder blocking makes a global memo unsound by default. | `src/types/substitute/mod.rs:16-25`, `41-81`; `src/types/substitute/apply.rs:9-275` | WU2 defines a per-run context; WU5 may use a scoped substitution memo only after review/approval and evidence. |
| **C — repeated work** | Evaluator child discovery and inference/candidate contextual walks can rewalk the same structural graph. | `src/check/checker/eval/extends.rs:54-76`, `106-225`; `src/check/checker/calls.rs:621-652` | Count in WU4; no universal traversal abstraction in this sprint. |

The relation cache invariants remain binding: contextual relations cannot use the durable cache, in-flight keys are assumed true only provisionally, and only sound verdicts are committed ([invariants](../reference/invariants.md#1-invariants-you-must-not-break), `src/relate/relation/mod.rs:330-385`). The existing profiling gate records wall-clock/RSS data but no usable samples for relation, substitution/inference, evaluator, or allocation buckets; it explicitly forbids treating throughput as proof of a hotspot ([profiling gate](../archive/backlog-13-profiling-gate.md#sampling-result-and-decision-buckets)).

## Non-goals / out of scope

- A universal `TypeRewriter`, visitor, or generic child-walker framework.
- Global relation, evaluator, inference, or substitution caches.
- A bytecode VM or evaluator architecture change.
- Cosmetic-only helper extraction, including template-hole matcher unification.
- Polymorphic `this` and all unrelated backlog features.
- Changing cache keys, cacheability policy, reason-chain reporting, or relation first-failure ordering without an explicit architecture decision.

If work exposes an independently valuable issue that needs roadmap ownership, the writer proposes the smallest suitable backlog item in the sprint outcome; this sprint does **not** create a backlog file without reporting and approval.

## Work units

### WU0 — fixture corpus and architecture probes (spec first, separate commit)

**Problem.** The correctness findings and the diagnostic-isolation risk need executable acceptance tests before implementation or measurement influences the design.

**Scope.** Add conformance fixtures and focused Rust tests where the public syntax cannot isolate the invariant. Cover:

- computed `obj[key](...)` and `(obj[key])(...)` calls retaining the method receiver and generic inference;
- `substitute_infers` on recursive structural shapes, plus nested generic constraints/defaults whose traversed children change;
- `InferenceConstraintEvaluator` preserving generic binders, constraints, defaults, receiver, and parameter metadata when evaluation changes a child;
- rejected overloads with cyclic/deep receivers and parameters: no trial-only `TK2589`, no leaked diagnostic, and stable final diagnostic selection/order.

**Acceptance.** Each fixture declares expected diagnostics under the corpus convention; focused tests prove metadata/termination where display text is insufficient. Run the targeted tests and the conformance harness before committing this WU alone. Cross-check the syntax cases with strict `tsc` where they are in scope, documenting any deliberate divergence.

**Architecture stop.** If the rejected-overload probe proves that trials mutate evaluator state or require transactional rollback beyond diagnostics, stop after recording the evidence. Do not add an ad-hoc transaction; bring the design decision to the user before WU1–WU5 touch candidate reuse or trial evaluation.

**Touchpoints.** `tests/cases/`, `src/check/checker/**/tests.rs` only where necessary, `tests/conformance.rs` only if a new enabled directory is required, and `docs/reference/divergences.md` only for a confirmed intentional divergence.

### WU1 — computed-member receiver correctness (bounded, separate commit)

**Problem.** Element-member calls must supply the same receiver and generic-inference context as direct member calls, including parenthesized callee forms.

**Scope.** Trace the existing call-lowering route and repair only the missing receiver/context propagation. Do not redesign call candidate selection, overload ranking, or expression inference.

**Acceptance.** WU0 receiver/generic tests pass; direct, computed, and parenthesized forms have equivalent accepted/rejected behavior and diagnostics; targeted call tests, conformance, `cargo test`, and clippy remain clean.

**Review.** A reviewer who did not write WU1 compares the AST/callee cases against `calls.rs:250-275` and `296-377`, with particular attention to generic receiver inference (`497-543`).

**Touchpoints.** Expected: `src/check/checker/calls.rs`, narrow call-expression helpers, WU0 fixtures/tests. No relation or evaluator changes.

### WU2 — dedicated `InferRewrite` context (separate commit)

**Problem.** `substitute_infers` is an ad-hoc recursive rewrite. It needs termination and reuse scoped to one infer freshening run, while preserving explicit semantics for generic metadata.

**Scope.** Replace only `substitute_infers`' internal recursion with a dedicated `InferRewrite` context containing per-run `in_progress` and completed-result memo state. Define traversal for every applicable child, explicitly including function type-parameter constraints/defaults and preserving all unrewritten metadata. Nested conditional `infer` binders stay opaque unless WU0 proves a required different rule.

**Acceptance.** WU0 recursion and metadata fixtures pass; recursive re-entry terminates without caching a cycle shortcut as a completed rewrite; unchanged shapes retain their original `TypeId`; changed functions retain binder order, ids, constraints, defaults, receiver, parameters, and return shape. No memo survives one top-level rewrite invocation.

**Review.** An independent reviewer verifies the scope/identity proof, not just test green status: memo keys must not omit fresh-binder or lexical context, and `in_progress` results must never become completed memo entries.

**Touchpoints.** `src/check/checker/eval/instantiation.rs`, narrowly related evaluator tests/fixtures. No changes to `src/types/substitute/` in this WU.

### WU3 — inference-constraint function binder preservation (separate commit)

**Problem.** `InferenceConstraintEvaluator::evaluate_function` drops generic binders when a child changes.

**Scope.** Preserve `FunctionType::type_params` and every non-type child field while evaluating only the approved child positions. Decide constraints/defaults traversal from WU0's explicit spec; do not fold this into a general rewrite framework.

**Acceptance.** WU0 binder/metadata cases pass under nested generic functions and changed receiver/parameter/return children. No function's generic metadata is erased merely because an evaluated child changed. Constraint-cycle/exhaustion behavior stays conservative and diagnostics remain unchanged outside the new cases.

**Review.** Separate reviewer compares the rebuilt `FunctionType` field-by-field with the source at `extends.rs:164-200`, then runs targeted evaluator tests plus the full required suite.

**Touchpoints.** `src/check/checker/eval/extends.rs`, narrow evaluator tests/fixtures only.

### WU4 — measurement-only instrumentation and microcorpora (separate commit or explicitly discarded probe)

**Problem.** Existing timing data has no attribution; it cannot select a hotpath safely.

**Scope.** Add temporary or test-only counters and deterministic microcorpora for:

- relation empty-context stack-key construction;
- ordered property comparisons and snapshot/allocation counts in relation/inference;
- substitution visits, repeated visits, and scoped-memo opportunity;
- contextual callback/candidate rewalks;
- evaluator child-walk visits.

Record environment, corpus scale, baseline, raw counters, and run-to-run variance in the sprint run log or a dedicated measurement note. Remove temporary production instrumentation before a product implementation commit; retained test helpers must be deterministic and scoped to tests.

**Decision threshold and reversibility.** A WU5 optimization needs both semantic acceptance and either (a) at least 20% fewer directly relevant counted operations/allocations at 10k and 100k scale, with deterministic directly relevant operation/allocation counters for both the candidate and every control, or (b) at least 10% median improvement in the isolated microcorpus across five clean repetitions, with paired high-resolution timing and the same timing metric for every control. Either path permits no more than 2% regression in the other measured microcorpora. Coarse instrumented wall-clock samples remain context, not a control metric. A counter alone does not justify global caching. Keep each candidate in an isolated, one-purpose commit that can be reverted independently.

**Acceptance.** Measurements identify where work occurs and distinguish empty/contextual relation paths; they do not claim profiler attribution unavailable under the current host restriction. No checker behavior changes.

**Review.** Reviewer validates counter placement against the exact audit paths and confirms the corpus cannot silently exercise a different semantic path.

**Touchpoints.** Test-only benchmark/probe infrastructure and this sprint's run log; product source only behind temporary instrumentation that is removed before WU5 lands.

### WU5 — evidence-backed bounded hotpath changes (one candidate per commit)

**Entry condition.** WU0–WU4 are accepted, no WU0 architecture stop remains open, and the candidate clears WU4's threshold.

**Permitted candidates.**

1. Empty-binder-context relation stack-key fast path, preserving the exact contextual snapshot path and cycle-stack ordering.
2. Ordered two-pointer object-property matching, preserving canonical target-property iteration and the first missing/mismatch reason.
3. Candidate-local fact reuse or a scoped substitution memo only after the writer presents the exact identity/rollback argument and obtains explicit approval.

**Forbidden candidates.** Global relation/evaluator caches, cache-key simplification, cross-candidate diagnostic state reuse, and any visitor framework are out of scope even if a microbenchmark makes them attractive.

**Acceptance.** Each change has a before/after WU4 record meeting the threshold, all WU0 semantics pass, and the full test/conformance/clippy suite is clean. Relation changes additionally preserve the cycle/cache invariants and demonstrate equivalent failure order on ordered properties. If a result misses threshold, record it and do not implement it.

**Review.** Independent adversarial reviewer checks false-negative risks, cache identity, cycle behavior, allocation claims, and the exact measured corpus. A second reviewer is required for any scoped memo or candidate-local reuse.

**Touchpoints.** At most one narrowly identified path per commit: `src/relate/relation/mod.rs`, `src/relate/relation/objects.rs`, `src/check/infer/context.rs`, `src/check/infer/helpers.rs`, or `src/types/substitute/`, plus WU0/WU4 tests. No broad refactor.

### WU6 — independent review, ratchet, and archive

**Scope.** Run an independent adversarial review of all correctness and performance changes, then run the full Rust suite, conformance corpus, and clippy. Build a fresh binary and run the official-suite regression ratchet only with a complete current corpus, following its documented preconditions.

**Acceptance.** Review findings are resolved or explicitly declined with evidence; no in-scope official-suite regression is accepted; measurements and thresholds are recorded; intentional divergences are documented; this plan receives an outcome and moves to `docs/archive/` only after all accepted commits land.

**Touchpoints.** Tests, official-suite scoreboard only if the ratchet changes it intentionally, docs outcome/indexes, and archive move. No unrelated source edits.

### WU7 — heap-backed auxiliary structural walks (accepted review follow-up)

**Problem.** Comprehensive review found that the new function metadata edges in
`InferRewrite` and `InferenceConstraintEvaluator` remain host-recursive on deep
acyclic `TypeId` graphs. Cycle guards stop repeated identities, but neither they nor
the conditional-evaluator budget bounds a unique chain assembled from shallow,
topologically ordered aliases. Receiver/parameter/return children already carried
the same pre-existing risk, so a metadata-only depth guard would leave the root
cause intact.

**Scope.** Convert both complete auxiliary structural walks to private explicit
heap task/value stacks. Preserve each walker's existing child order, postorder
rebuilds, opaque tags, unchanged identity, generic metadata, SCC suffix taint,
memo/exhaustion policy, and test-only metrics. Keep the machines separate:
`InferRewrite` owns lexical fresh binders and a per-run completed memo;
`InferenceConstraintEvaluator` executes pending types and rolls structural results
back on global exhaustion. Do not introduce a universal visitor, new cache, or
metadata-only shim.

**Acceptance.** Commit the disabled syntax corpus first. Direct arena tests then
prove at least 10k unique structural nodes for generic constraints/defaults and the
pre-existing receiver/parameter/return arms without native recursion, while existing
DAG memo, self/mutual cycle, cycle+sibling, pending exhaustion, metadata, diagnostic,
and measurement-count tests retain their exact behavior. Enable the WU7 corpus,
cross-check it with strict `tsc 6.0.3`, run debug and release focused tests, full
tests/clippy/release, independent adversarial review, and the official-suite ratchet.

**Touchpoints.** `src/check/checker/eval/{instantiation,extends,tests}.rs`, narrow
caller tests where direct entry coverage is useful, `tests/cases/sr_rewrite_hotpath_wu7/`,
the conformance registry, and architecture/invariant docs. No relation, call
selection, inference candidate, or general substitution changes.

**Implementation status.** WU7a (`db0a788`) and WU7b (`092ba3d`) are complete
and independently reviewed PASS. `InferRewrite` and
`InferenceConstraintEvaluator` now each use a private explicit heap task/value
stack over their complete structural walks. The direct arena witnesses include a
10,000-deep infer-metadata chain and a 10,005-deep alternating
constraint/default/receiver/parameter/return spine. Their deliberately different
policies remain local: infer rewriting keeps its fresh-binder scope, per-run
completed memo, and SCC-suffix identity taint; constraint evaluation delegates
pending types and returns the original structural result after global exhaustion.
The WU7 syntax corpus is enabled and its completed verification is retained in the
run log. WU8 reopens the sprint for the remaining mapped-value structural walk; its
spec remains disabled until the implementation lands.

### WU8 — mapped-value rewrite work stack (spec first, separate commit)

**Problem.** `MappedRewrite::replace_mapped_value_rec` is the remaining unbounded
host-recursive structural walk reachable from mapped-type evaluation. It is entered
for every concrete mapped property value after `assemble_mapped` selects its source
property. Its existing `in_progress` guard terminates cyclic graphs but cannot bound
a deep acyclic value-template graph.

**Scope.** Replace only `MappedRewrite`'s recursion with a private local heap
task/value stack in `mapped.rs`. Preserve the current traversal set and order:
nested mapped nodes rewrite key/modifiers sources but not their rebound value
template; functions rewrite generic constraints/defaults, receiver, parameters, and
return in source order; instantiations rewrite argument values but not bases.
Preserve the per-call completed memo and the current re-entry rule exactly: an
in-progress revisit returns the original `TypeId`, then normal completion removes
the id and memoizes its result, including a result containing a provisional back
edge. Do not add SCC taint, an evaluator memo write, a budget, or a shared
visitor/task utility.

**Acceptance.** Commit the disabled source corpus first. Direct arena evaluation
then proves a 10k+ acyclic object/property value-template spine replaces its terminal
`MappedValue` without native recursion. A direct cyclic placeholder witness preserves
the current partial-clone semantics: the rewritten root changes, its back edge stays
the original recursive `TypeId`, and repeated fresh evaluations remain stable. Existing
recursive-mapped, nested-mapped binder-boundary, identity, and M26 corpus behavior
remain unchanged. A second source witness and direct arena test prove `MappedValue`
rewrites in generic function constraints/defaults as well as receiver/parameters/
return; this is a behavior correction, so the disabled spec is intentionally RED at
current typokat. Cross-check the source corpus with strict `tsc 6.0.3`; run focused
debug/release tests, full gates, and an independent adversarial review before closure.

**Touchpoints.** `src/check/checker/eval/mapped.rs`, narrow evaluator tests,
`tests/cases/sr_rewrite_hotpath_wu8/`, the conformance registry, this sprint, and
the current architecture/invariant wording. No `src/types/substitute/`, keyof,
template, relation, or evaluator-wide task changes.

## Sequencing and ownership

One Terra writer owns the active worktree and makes one work unit's source changes at a time. The writer never self-approves an implementation WU: a distinct reviewer performs the adversarial review after each WU and before its commit. This prevents concurrent edits from obscuring cache/diagnostic causality.

| Order | Work | Gate |
|---:|---|---|
| 1 | WU0 | Acceptance corpus committed independently; architecture-stop probe must pass or pause the sprint. |
| 2 | WU1 | May proceed after WU0; bounded to call receiver propagation. |
| 3 | WU2, then WU3 | Sequential evaluator rewrite work to keep generic-metadata causality reviewable. |
| 4 | WU4 | May collect measurement after WU0, but no optimization decision until its reviewed baseline exists. |
| 5 | WU5 | One evidence-backed candidate at a time, each independently reversible. |
| 6 | WU6 | Independent comprehensive review and initial full verification. |
| 7 | WU7 | Spec, two local heap task machines, final verification, and independent reviews PASS. |
| 8 | WU8 | Disabled mapped-value corpus first, then one local task machine and independent review. |
| 9 | closure | Re-run full verification/ratchet, record outcome, and archive only after WU8 lands. |

## Decisions / open questions

1. Correctness probes outrank performance work. A failed overload-isolation probe is an architecture stop, not a cue to add rollback machinery.
2. Per-run rewrite memoization is permitted only where its lexical/binder identity is explicit. Durable/global memoization is not part of this sprint.
3. The empty-context relation fast path is the only cache-adjacent change pre-authorized for measurement; contextual stack snapshots remain the reference implementation.
4. The ordered property change must preserve the target's canonical traversal and first failure. Faster lookup with altered reason ordering is rejected.
5. The owner will propose, rather than create, a minimal backlog item if measurement or the overload probe reveals a larger architectural requirement.
6. The user accepted CORR-1 for immediate implementation. WU7 uses two private task
   machines; their memo, opacity, pending-evaluation, and exhaustion policies are too
   different for a sound shared visitor abstraction.
7. WU8 uses a third, separate private machine. `MappedRewrite`'s per-call memo and
   partial-cycle result are deliberately unlike WU7's SCC-tainted infer rewrite and
   exhaustion-aware constraint evaluator. Strict `tsc` probes establish that WU8
   must rewrite generic-function constraints/defaults as well as signature children.

## Run log

- **WU0 fixture design (2026-07-13).** Strict `tsc 6.0.3` accepts direct and
  parenthesized computed-member calls when the receiver satisfies `this`, rejects
  the corresponding bad receiver with `TS2684`, accepts infer substitution through
  nested generic constraints/defaults, and reports `TS2322` for the recursive-shape
  witness. Current typokat loses the computed receiver (including generic result
  inference), leaves nested generic metadata pointing at the stale infer binder,
  and stack-overflows on the recursive infer rewrite.
- **Architecture-stop probe.** A rejected generic receiver overload whose receiver
  constraint recursively instantiates reports depth error `2589` in both strict
  `tsc 6.0.3` and current typokat. No trial-only diagnostic leak or changed final
  selection was observed, so the sprint may proceed without transaction design.
- **Corpus ownership.** `tests/cases/sr_rewrite_hotpath_wu0/` remains disabled until
  WU1-WU3 jointly pass it. No deliberate divergence or separately deferred roadmap
  item was found, so WU0 adds neither a divergence-ledger row nor a backlog file.
- **WU2 review regression.** Direct re-entry initially produced a partial recursive
  clone; a global rollback avoided that clone but discarded an independent outer
  infer sibling in a source-reachable type. `recursive_infer_sibling.ts` pins the
  required boundary: the recursive child retains identity while the acyclic
  `value: U` sibling still freshens. Cycle handling must be path-local, not a
  top-level rollback.
- **WU3 public witness.** `constraint_function_metadata.ts` forces constraint
  evaluation to change receiver/parameter children while a nested generic binder
  remains semantically visible. Its bad call must retain `"ok"` in the reason chain;
  an anonymous rebuilt signature with erased binders is not an acceptable match.
- **WU4a relation/inference baseline (2026-07-13, revision `89b21c6`).** Test-only,
  thread-local counters instrument the actual `stack_relation_key` construction,
  normal `relate_objects` source-property predicate, and `InferenceContext::infer_objects`
  snapshots/predicate. Direct reserved-object builders construct graphs before the timer;
  the timed release test performs 10k and 100k distinct empty-context relations with
  eight ordered properties each. Small exact-formula tests pin target-first scans:
  two width-3 relations/inferences perform 6 target obligations and 12 source predicates.
  The release commands were `cargo test --release measure_relation_hotpaths_release --
  --ignored --nocapture` and `cargo test --release measure_inference_hotpaths_release --
  --ignored --nocapture`, each run five clean repetitions on Linux 6.17.0-40-generic,
  x86_64, rustc 1.95.0. These elapsed values are **instrumented and
  environment-dependent**; the operation counters, not elapsed time, are the selection proof.
  Relation raw counters were stable at 10k/100k:
  `stack_key_builds=empty_context_stack_keys=10,000/100,000`, target properties
  `80,000/800,000`, and source predicates `360,000/3,600,000`. Its five raw elapsed
  samples were 10k `[3, 3, 3, 5, 3]ms` (median 3ms, range 3–5ms) and 100k
  `[37, 43, 43, 44, 40]ms` (median 43ms, range 37–44ms). Inference counters were
  stable at 10k/100k:
  snapshots `20,000/200,000`, entries `160,000/1,600,000`, cloned-name-byte proxy
  `1,760,000/17,600,000`, target properties `80,000/800,000`, and source predicates
  `360,000/3,600,000`. Its five raw elapsed samples were 10k `[7, 5, 7, 7, 7]ms`
  (median 7ms, range 5–7ms) and 100k `[73, 65, 62, 62, 69]ms` (median 65ms,
  range 62–73ms). The name-byte proxy is accumulated during the actual existing
  `property_pairs` clone/map traversal; it performs no separate measurement scan.
  All measurement shapes are clean; targeted formula tests passed, and the existing
  full corpus remains the failure-order control. Counters are `cfg(test)` only and
  create no production path. No WU5 threshold is claimed yet because no before/after
  optimization exists. Ordered matching is the eligible candidate: an equal-shape
  cursor walk would reduce this width-8 predicate count from 36 to 8 per pair
  (77.8%), but must prove the reduction and target-first reason equivalence after a
  separately reviewed WU5 implementation. The empty-context stack-key fast path is
  measured but has no allocation claim and has not cleared the gate.
- **WU4b rewrite/evaluator/substitution baseline (2026-07-13, revision `777e170`).**
  Test-only thread-local counters are placed on the actual recursive entries and
  exits: `InferRewrite` records top-level runs, visits, completed-memo hits/inserts,
  re-entries, and tainted identity returns; `InferenceConstraintEvaluator` records
  evaluate/pending calls, structural entries/re-entries, actual function metadata
  and signature children, identity returns, and re-interns; `Substitution` records
  apply visits, raw repeated `TypeId`s, repeats under the exact sorted blocked-binder
  context, map/blocked-param hits, and existing guard re-entries. The exact-context
  key is test-only and is collected during the real `apply` entry, not by a second
  scan. Production builds do not contain these counters.

  Small direct arena microcorpora pin the paths: an infer shared DAG has
  `visits=4, memo_hits=1, memo_inserts=3`; its recursive infer sibling has
  `visits=3, reentries=1, tainted_identity_returns=1, memo_inserts=1`.
  Function metadata fanout has `metadata_children=4, signature_children=2,
  pending_calls=5, structural_entries=1, re_interns=1`; its recursive
  function/object/sibling witness has `structural_entries=3, reentries=1,
  tainted_identity_returns=2, re_interns=1`. Substitution's same-context DAG has
  `apply_visits=7, raw_repeats=exact_context_repeats=4, map_hits=4`; the blocked
  binder adversary has `apply_visits=12, raw_repeats=7,
  exact_context_repeats=5, map_hits=4, blocked_hits=2`; and the recursive-object
  control has `apply_visits=2, exact_context_repeats=1, cycle_reentries=1`.

  Release probes construct every graph before `Instant::now()` and run five fresh
  repetitions at each scale on Linux 6.17.0-40-generic, x86_64, rustc 1.95.0.
  Commands: `cargo test --release measure_infer_rewrite_hotpaths_release --
  --ignored --nocapture`, `cargo test --release
  measure_constraint_evaluator_hotpaths_release -- --ignored --nocapture`, and
  `cargo test --release measure_substitution_hotpaths_counter_only -- --ignored
  --nocapture`. Infer-rewrite emitted raw fields were stable: at 10k/100k shared
  children, respectively, visits `10,002/100,002`, memo hits `9,999/99,999`,
  inserts `3/3`, and re-entries/tainted returns `0/0`. The listed child-edge totals
  `10,001/100,001` are derived arithmetic (`visits - top_level_runs`), not emitted
  counter fields.
  Its five elapsed samples were 10k `[89.825, 89.865, 90.206, 156.278, 181.765]µs`
  (median `90.206µs`, range `89.825–181.765µs`) and 100k `[906.369, 962.642,
  984.022, 1267.644, 1388.216]µs` (median `984.022µs`, range
  `906.369–1388.216µs`). Constraint evaluation was stable at 10k/100k at
  emitted fields evaluate calls `40,004/400,004`, pending calls `20,001/200,001`,
  metadata children `20,000/200,000`, signature children `2/2`, structural entries
  `1/1`, and re-interns `1/1`; the child-evaluation totals `40,003/400,003` are
  derived arithmetic (`evaluate_calls - 1` for this single-root corpus), not emitted fields. No
  probe had an SCC re-entry, tainted identity return, or exhaustion. Its elapsed samples were
  10k `[1.944009, 2.002517, 4.858936, 4.976132, 5.327228]ms` (median `4.858936ms`,
  range `1.944009–5.327228ms`) and 100k `[12.138980, 14.445246, 19.114924,
  22.380797, 26.549633]ms` (median `19.114924ms`, range `12.138980–26.549633ms`).
  These elapsed samples are instrumented and environment-dependent; the direct
  counters establish only where work occurs.

  Substitution timing is deliberately **unavailable**: the required exact blocked-
  binder-context counter sorts and snapshots the current test-only set at each
  actual `apply` entry, which would materially contaminate a timing result. Five
  counter-only repetitions were identical: at 10k, `runs=1, apply_visits=30,001,
  raw_repeats=exact_context_repeats=29,998, map_hits=20,000, blocked_hits=0,
  cycle_reentries=0`; at 100k the values were `1, 300,001, 299,998, 299,998,
  200,000, 0, 0` in the same field order. No elapsed samples, median, or range are
  claimed for this path.

  **Selection conclusion.** This is measurement only. The infer memo is the already
  scoped WU2 mechanism, not a new WU5 candidate; evaluator fanout has no approved
  reuse boundary. The substitution adversary proves that a raw `TypeId` repeat is
  not a sound memo key (`7` raw repeats but only `5` exact-context repeats), so these
  measurements do not authorize a scoped substitution memo. No WU5 selection or
  architecture widening follows from WU4b.

- **WU4c call/construct baseline (2026-07-13, revision `23c7e38`).** Test-only
  thread-local counters instrument raw argument walks, speculative/committed candidate
  builds, trial outcomes, generic preliminary/full inference, contextual arrow and
  fresh-literal rewalks, speculative diagnostic rollback deltas, and trial/selected
  receiver relations. The `cfg(test)` phase parameter is erased from production builds;
  no contextual result, candidate, diagnostic, or relation fact is reused. Small C=1
  tests pin two-overload callback and fresh-literal paths (each has builds `2+1`, trials
  `2`, and rewalks `[3,2,1,0,0]` for
  candidate-inference/trial/committed-check/class-ctor/other), an explicit-constraint
  rollback, receiver checking, and the structural construct-signature mirror. The ignored
  scaled callback corpus uses nine generic overloads, so each clean
  call has exactly 20 callback rewalks: 500/5,000 calls emit 10,000/100,000. Run
  `cargo test measure_call_pipeline_scaled_callback_corpus -- --ignored`; command and
  counters are deterministic on Linux 6.17.0-40-generic, x86_64, rustc 1.95.0.
  Elapsed timing is deliberately unavailable because this end-to-end corpus includes
  parsing/binding and test-only counters; only operation counts select future work.
  No WU5 threshold or candidate-local reuse is claimed.

- **WU5a ordered normal-object matching (2026-07-13, working tree based at
  `36d8c7b`).** `Relater::relate_objects` now walks the canonical source and target
  property orders with one monotonic source cursor. The source cursor skips names
  before a target member, stops without consuming a name after it, and advances after
  a match. It still iterates target properties and returns their first failure. The
  canonical-order precondition is enforced by both `Interner::intern_object` and
  `Interner::fill_object`, which sort properties by name. A same-name retained match
  preserves the prior first-source-member behavior for malformed duplicate target
  names. No cache, cycle, stack-key, merged-intersection, inference, or shared-helper
  path changed.

  The test-only comparison counter now counts each actual source-name comparison.
  The exact equal-shape formula changes from `count × width × (width + 1) / 2` to
  `count × width`; the width-3 two-pair test therefore changes from `12` to `6`.
  Adversarial coverage verifies source extras before/between/after target names;
  early, middle, and late missing targets; early and late type mismatches; and the
  existing optional-presence and nominal-origin rejections through the cursor path.

  Five fresh release-process repetitions used
  `for run in 1 2 3 4 5; do cargo test --release
  measure_relation_hotpaths_release -- --ignored --nocapture; done` on Linux
  6.17.0-40-generic, x86_64, rustc 1.95.0. The relation counters were identical in
  every run: stack keys/empty-context keys `10,000/100,000`, target properties
  `80,000/800,000`, and source-name comparisons `80,000/800,000` at 10k/100k.
  Against WU4a's `360,000/3,600,000` comparisons, that is a `77.8%` reduction at
  both scales, clearing the ≥20% operation threshold. Raw elapsed samples were 10k
  `[2, 2, 2, 4, 3]ms` (median 2ms, range 2–4ms) and 100k `[32, 36, 33, 31, 34]ms`
  (median 33ms, range 31–36ms), versus WU4a medians of 3ms and 43ms. These timings
  remain instrumented and environment-dependent.

  The unchanged-inference control used the same five-process loop with
  `measure_inference_hotpaths_release`. Its counters remained exactly WU4a's values
  (`360,000/3,600,000` source predicates and all snapshot fields unchanged); raw
  elapsed samples were 10k `[12, 7, 6, 7, 6]ms` (median 7ms, range 6–12ms) and 100k
  `[92, 60, 59, 60, 60]ms` (median 60ms, range 59–92ms). Compared with WU4a's 7ms
  and 65ms medians, the control has no regression (0% at 10k and a 7.7% improvement
  at 100k), satisfying the ≤2% control condition.

- **WU5b ordered inference-object matching (2026-07-13, working tree based at
  `0222182`).** `InferenceContext::infer_objects` retains the existing two
  `property_pairs` snapshots, including their name cloning and test-only snapshot
  counters, and changes only matching over those ordered vectors to a monotonic
  source cursor. It continues to visit target pairs in canonical order; a source-only
  or target-only name contributes no candidate; and a retained same-name match gives
  malformed duplicate target names the same first-source-member behavior as the
  prior linear `find`. `property_pairs` receives the stable name order enforced by
  both `Interner::intern_object` and `Interner::fill_object`. No relation product
  code, relation cache/cycle behavior, call path, evaluator, candidate policy, or
  snapshot allocation path changed.

  The comparison counter now counts actual source-name comparisons. The exact
  equal-shape formula changes from `count × width × (width + 1) / 2` to
  `count × width`, so the two width-3 pairs change from `12` to `6` comparisons.
  Direct tests cover source extras before/between/after target names, a missing
  target name that contributes no candidate while later names still contribute,
  stable internal duplicate source/target names, candidate order in target order,
  and the observable call-site fixing result for duplicate names.

  Five fresh release-process repetitions used
  `for run in 1 2 3 4 5; do cargo test --release
  measure_inference_hotpaths_release -- --ignored --nocapture; done` on Linux
  6.17.0-40-generic, x86_64, rustc 1.95.0. Inference emitted identical counters on
  every run: snapshot vectors `20,000/200,000`, entries `160,000/1,600,000`, name
  bytes `1,760,000/17,600,000`, target properties `80,000/800,000`, and source-name
  comparisons `80,000/800,000` at 10k/100k. Against WU4a's `360,000/3,600,000`
  comparisons, this is a `77.8%` reduction at both scales, clearing the ≥20%
  operation threshold. Raw elapsed samples were 10k `[5, 6, 6, 5, 6]ms` (median
  6ms, range 5–6ms) and 100k `[57, 60, 61, 62, 57]ms` (median 60ms, range 57–62ms),
  versus WU4a medians of 7ms and 65ms. Timings remain instrumented and
  environment-dependent.

  The relation control used the same five-process loop with
  `measure_relation_hotpaths_release`. Its emitted work counters were exactly the
  WU5a values in every run: stack keys/empty-context keys `10,000/100,000`, target
  properties `80,000/800,000`, and source-name comparisons `80,000/800,000`—a 0%
  control regression. Its raw elapsed samples were 10k `[3, 3, 3, 2, 2]ms` (median
  3ms, range 2–3ms) and 100k `[37, 41, 35, 36, 32]ms` (median 36ms, range 32–41ms).
  These overlap WU5a's 2–4ms and 31–36ms ranges, but their coarse millisecond
  medians are not a meaningful ≤2% timing discriminator; the unchanged direct
  operation counter is the control conclusion.

- **WU6 comprehensive-review follow-up (2026-07-13).** One medium finding,
  `CORR-1`, extends the existing host-recursive structural-walker risk to new
  function metadata edges; it is an architecture stop pending user disposition.
  Three low findings are remediated here: C=1 preliminary-inference assertions,
  current-facing corpus/sprint lifecycle text, and an explicit counter-versus-timing
  control policy. No `CORR-1` implementation or backlog item is created in this sprint.
- **WU7 approval and design (2026-07-13).** The user chose immediate implementation
  over deferral. Three Terra design passes converged on separate local postorder
  task/value stacks modeled on the primary conditional evaluator. The disabled
  `sr_rewrite_hotpath_wu7/` syntax corpus pins both source routes; direct arena tests
  will carry the 10k+ depth proof without relying on the CLI's 256MiB worker stack.
- **WU7a/WU7b implementation and review (2026-07-13).** `db0a788` made
  `InferRewrite` iterative and `092ba3d` did the same for
  `InferenceConstraintEvaluator`. Both use private task/value stacks rather than a
  shared visitor: the rewrite retains fresh-binder/memo/SCC-taint semantics, while
  constraint evaluation retains pending-type delegation and conservative
  exhaustion rollback. The enabled shallow source corpus covers both routes;
  direct arena tests cover a 10,000-deep metadata chain and a 10,005-deep
  alternating signature-child spine. Independent adversarial reviews of WU7a and
  WU7b were PASS. At that point the final gates and official-suite ratchet were
  outstanding; the following verification completed them.
- **WU7 final verification (2026-07-13).** `cargo fmt --check`, the full debug suite,
  focused release infer-rewrite/inference-constraint suites, clippy, and release
  build all passed. The pinned official-suite ratchet at `050880ce5` reported
  0 regressions, 0 progress, and 0 missing entries. Two final independent Terra
  reviews over `0d8e7d0..092ba3d` were PASS with no findings; the exact 10k/100k
  WU4b metric probes also passed. No follow-up backlog work was identified.
- **WU8 architecture audit (2026-07-13).** `MappedRewrite` is the only remaining
  unbounded structural recursion under `src/check/checker/eval/`. It is source-
  reachable through concrete mapped aliases, but a huge source alias spine would
  also exercise host-recursive generic substitution before reaching it. WU8 therefore
  pairs a shallow source-route witness with a direct 10k+ arena evaluation witness.
  Its in-progress re-entry returns the original node and still permits an ancestor
  to memoize a partial clone; WU7's SCC-taint policy must not be reused.
- **WU8 metadata arbitration (2026-07-13).** Strict `tsc` and current typokat probes
  establish that `MappedValue` lowers into generic function constraints/defaults.
  WU8 therefore rewrites both metadata slots in addition to receiver, parameters,
  and return. The disabled metadata fixture is intentionally RED at current typokat:
  strict `tsc 6.0.3 --strict` reports only `TS2345` (bad argument) and `TS2322`
  (bad assignment), while typokat also reports a false-positive `TK2322` on the
  clean generic-signature assignment because its source constraint remains `T[K]`.
