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

**Decision threshold and reversibility.** A WU5 optimization needs both semantic acceptance and either (a) at least 20% fewer directly relevant counted operations/allocations at 10k and 100k scale, or (b) at least 10% median improvement in the isolated microcorpus across five clean repetitions, with no more than 2% regression in the other measured microcorpora. A counter alone does not justify global caching. Keep each candidate in an isolated, one-purpose commit that can be reverted independently.

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

## Sequencing and ownership

One Terra writer owns the active worktree and makes one work unit's source changes at a time. The writer never self-approves an implementation WU: a distinct reviewer performs the adversarial review after each WU and before its commit. This prevents concurrent edits from obscuring cache/diagnostic causality.

| Order | Work | Gate |
|---:|---|---|
| 1 | WU0 | Acceptance corpus committed independently; architecture-stop probe must pass or pause the sprint. |
| 2 | WU1 | May proceed after WU0; bounded to call receiver propagation. |
| 3 | WU2, then WU3 | Sequential evaluator rewrite work to keep generic-metadata causality reviewable. |
| 4 | WU4 | May collect measurement after WU0, but no optimization decision until its reviewed baseline exists. |
| 5 | WU5 | One evidence-backed candidate at a time, each independently reversible. |
| 6 | WU6 | Independent final review, full verification, ratchet, outcome, archive. |

## Decisions / open questions

1. Correctness probes outrank performance work. A failed overload-isolation probe is an architecture stop, not a cue to add rollback machinery.
2. Per-run rewrite memoization is permitted only where its lexical/binder identity is explicit. Durable/global memoization is not part of this sprint.
3. The empty-context relation fast path is the only cache-adjacent change pre-authorized for measurement; contextual stack snapshots remain the reference implementation.
4. The ordered property change must preserve the target's canonical traversal and first failure. Faster lookup with altered reason ordering is rejected.
5. The owner will propose, rather than create, a minimal backlog item if measurement or the overload probe reveals a larger architectural requirement.

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
