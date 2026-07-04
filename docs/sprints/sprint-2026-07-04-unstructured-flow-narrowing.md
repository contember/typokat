<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 → <sha>,
> WU2 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — unstructured-flow narrowing, the flow-node CFG / M23 (2026-07-04)

**Goal.** Ship backlog [`07`](../backlog/07-unstructured-flow-narrowing.md): narrowing works
through unstructured flow — early `return`/`throw`, `&&`/`||`/ternary, assignment-in-flow, and
loop edges — via the flow-node CFG (architecture §5), replacing the structured fork-and-restore
as the single narrowing model.

**Theme.** The biggest remaining narrowing gap and the most likely to surprise on idiomatic code
(`if (x === null) return;` is everywhere). Architecture §12 orders this **before** the type-level
evaluation phase. Success: the `m23_unstructured_narrowing` corpus passes, the m7/m8 structured
corpora stay green **through the new CFG path**, official-suite `controlFlow`/`typeGuards` rows
improve with zero regressions.

## Refs re-verified at HEAD (2026-07-04)

- ✔ **The narrowing ops are already flow-model-agnostic** — `src/check/flow.rs:145-344`:
  `narrow(interner, ty, op, positive)`, `narrow_by_typeof`, `narrow_by_truthiness`,
  `narrow_by_discriminant`, `narrow_by_in_operator` are pure `TypeId → TypeId` functions with no
  knowledge of the flow model; they were written for exactly this reuse.
- ✔ **A `FlowNode` stub anticipates the CFG** — `src/check/flow.rs:33,53` ("the eventual general
  model for **unstructured** flow").
- ✔ **Structured narrowing is fork-and-restore in the checker** —
  `src/check/checker/narrowing.rs:33-113`: `check_if` forks the env for then/else and restores;
  `check_switch` forks per clause. This mechanism is what the CFG **replaces** (single model).
- ✔ **The narrowing env is keyed on `SymbolId`** and resets on assignment and at function
  boundaries (invariants §1 "Narrowing"). M23 **refines** one clause of that invariant:
  an assignment now narrows the variable **to the assigned value's type** in the flow that
  follows (tsc behavior; still sound — probed) instead of resetting to the declared type.
  `invariants.md` must be updated in the same change that ships this.
- ✔ **tsc 6.0.3 probes pin the semantics** (scratchpad `probe_m23*.ts`, 2026-07-04): early
  `return`/`throw` narrows the rest of the block (the source type renders **fully narrowed**,
  e.g. `Type 'null' is not assignable to type 'string'`); a guard **without** early exit
  re-joins and un-narrows; `&&`/`||`/ternary narrow RHS/arms; assignment narrows straight-line
  flow and **joins widen** (`assignJoin`); `while` conditions narrow the body, assignments flow
  **around the back edge** (`loopBackEdge` errors), the exit edge narrows after the loop
  (`afterLoop` is `null`), a `break` edge **un-narrows** after the loop (`breakTrap` errors),
  `continue` re-checks the condition. A closure over a never-reassigned parameter sees the
  narrowing in tsc — typokat keeps the function-boundary reset (over-report, safe, documented).

## Work units

### WU1 — Flow-node CFG + full migration of narrowing resolution (effort L)

- **Problem.** Narrowing exists only inside structured `if`/`else`/`switch` forks; everything in
  the m23 corpus is either a false positive (early-exit/assignment shapes) or resolved against
  the un-narrowed declared type.
- **Verify first.** `cargo run -- check` over `tests/cases/m23_unstructured_narrowing/*.ts` at
  HEAD: positives over-report, `noEarlyExit`/`assignJoin`/`loopBackEdge`/`breakTrap` traps show
  today's safe behavior. m7/m8 all green (the regression baseline).
- **Scope.** Build the flow graph in `src/check/flow.rs` on the `FlowNode` stub: nodes for
  start, condition (guard + polarity), assignment, branch label (join), loop label (back-edge
  join), and unreachable (after `return`/`throw`); the checker's statement/expression walk
  **constructs** the graph (if/else, switch, while, `&&`/`||`, ternary, assignment, early exit)
  and each identifier reference resolves its type by the **backward walk** from its flow node,
  applying the reused narrowing ops, memoized per `(FlowNode, SymbolId)`, with the declared type
  as the fixpoint seed at loop labels. Replace the fork-and-restore env so if/else/switch resolve
  through the same CFG (delete the env or reduce it to the CFG's cursor state). Function
  boundaries stay narrowing barriers (existing invariant; the closure divergence stays).
- **Acceptance / witness.** `m23_unstructured_narrowing` enabled and green; **m7 + m8 stay green
  through the CFG path**; all other corpora unregressed; no reachable panic on cyclic/degenerate
  flow (deep nesting, `while(true)`).
- **Touch points.** `src/check/flow.rs` (graph + resolver), `src/check/checker/narrowing.rs`
  (rewired), `src/check/checker/statements.rs` + `expr.rs` (graph construction during the walk),
  `docs/reference/invariants.md` (the assignment-narrowing refinement + dropping "all narrowing
  is structured"), `tests/conformance.rs`.
- **Out of scope.** Assertion functions / type predicates (`x is T`), `for`/`for-of`/`do-while`
  beyond what falls out naturally, definite assignment (`TK2454`), reachability diagnostics
  (`TK2355`), closure narrowing of never-reassigned bindings (documented over-report),
  narrowing member accesses (`x.a` paths — symbol-keyed only, existing invariant).

### WU2 — Independent adversarial review + official-suite ratchet (effort M)

- **Scope.** A *different* agent hunts false negatives: joins that keep a stale narrow type
  (the sharpest CFG bug — a missed back-edge/break edge = dropped error), order-dependence via
  the relation cache, memoization returning provisional loop states, `switch` fallthrough,
  nested loops, guards on shadowed names. Cross-checks every fixture and fresh probes against
  `tsc --strict`. Then the ratchet: `tsofficial.py run --check` (expect `controlFlow/`,
  `typeGuards/typeof*` improvements), `--save` + scoreboard commit.
- **Acceptance.** PASS verdict with concrete probes; all gates green; scoreboard delta committed.

## Decisions

- **One narrowing model, not two.** The CFG subsumes structured narrowing; keeping fork-and-restore
  alongside a CFG doubles every future guard's implementation. The migration cost is paid by the
  m7/m8 regression net.
- **Backward walk with memoization (the tsc model), not forward abstract interpretation** — per
  the backlog item and architecture §5; the ops are already shaped for it.
- **Loop soundness over precision:** the fixpoint seed at a loop label is the declared type; a
  variable assigned anywhere in the loop body re-derives from the back edge. Where precision is
  not reachable cheaply, fall back to the declared type (over-report, safe) — never keep a
  pre-loop narrow state across a back edge (false negative).

## Sequencing

Spec (done, this commit precedes implementation) → WU1 (single Opus implementation agent) →
WU2 review → fix loop → leader commits → ratchet.

## Run log

<!-- Append as you work. -->

- 2026-07-04 (WU1 impl): graph built in a dedicated **pre-pass** (`build_flow_graph` →
  `src/check/checker/flowgraph.rs`), not during the walk — a reference inside a loop body is
  checked before the back-edge assignment is seen, so back edges must be complete before any
  resolution (the `loopBackEdge` trap). References map to nodes by span (`reference_flow`);
  resolution is an iterative memoized backward walk; loop labels run a single-unroll fixpoint
  seeded from the declared type, provisional seeds never durably memoized and durable writes
  suppressed while any fixpoint is in flight (`flow_loop_depth`) — the relation-cache lesson
  applied. Fork-and-restore deleted; every m7/m8 test passed through the CFG unchanged.
- 2026-07-04 (WU1 impl): supporting changes — binder now binds `while` bodies; `never` member
  access yields `never` (tsc-aligned); throw arguments stay unwalked in the check pass (pre-M23
  baseline); dead-code references resolve to the declared type (safe); literal assignments
  narrow to the widened base, complex/compound RHS resets to declared (safe).
- 2026-07-04 (WU1 impl): official-suite `--check` is NOT zero — **8 residual regressions, all
  claimed sound over-reports** (lib-shaped `TK2339`/`TK2345` on newly-walked while bodies /
  ternary arms / logical RHS, zero dropped errors, matched counts equal-or-up) — inherent to
  walking more control flow without `lib.d.ts`. Sent to independent review for a per-file
  matched/fn audit before the WU2 ratchet accepts them.
- 2026-07-04 (WU1 review fixes, round 1 → FAIL → fixed): (1) a **destructuring assignment**
  target (`[x] = …`, `({ x } = …)`) kept a stale narrow past the reassignment (dropped error) —
  the flow builder now emits a reset-to-declared assignment node for every identifier the
  pattern binds (elements/properties/shorthand/renamed/defaults/rest, nested; member targets
  bind no symbol; TS-wrapper targets out of subset). Pinned by the amended
  `assignment_patterns.ts` (spec commit 460807f). (2) the `never` member-access suppression was
  **reverted** — tsc 6.0.3 reports TS2339 on `never` member access (the "tsc-aligned" claim in
  the entry above was wrong); the revert reintroduced **no** regression and *gained* a match
  (`typeGuardsInIfStatement` matched 0→1, suite 236→237: the baseline's TS2339-on-`never` at
  line 139 now matches). Final `--check`: the same audited 8, nothing new. README m23 note
  added: declaration initializers deliberately not narrowed.
