# Sprint — declaration hoisting parity (2026-07-11)

**Goal.** Close backlog `74`: forward local-function calls are checked against the
hoisted callable surface, and `var` bindings resolve from their containing
function/module scope without changing initializer or flow timing.

**Theme.** Remove declaration-order-dependent checking before the minimal prelude and
real-project preview. The two symptoms share one TypeScript visibility rule but land in
different existing layers at HEAD: function names are already bound, while their types
are filled too late; `var` names are bound into the wrong lexical scope.

## Refs re-verified at HEAD (2026-07-11)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The binder completes before checking and walks every statement list, so a local
  function name is already present in the scope graph regardless of source order —
  `src/binder/bind.rs:274`, `src/binder/bind.rs:290`,
  `src/binder/bind.rs:514`.
- ⚠ Forward calls fail later than backlog `74`'s “one binder predeclaration problem”
  wording suggests: `DeclTypes` starts empty, the checker walks statements in source
  order, and a function's callable type is written only when its declaration is reached —
  `src/check/checker/context.rs:62`, `src/check/checker/statements.rs:42`,
  `src/check/checker/statements.rs:573`.
- ✔ M33 overload groups are recognized only as consecutive same-name declarations;
  their implementation signature is replaced by an object carrying the visible overload
  signatures — `src/check/checker/statements.rs:599`,
  `src/check/checker/statements.rs:630`, `src/check/checker/statements.rs:779`.
- ⚠ `infer_function` currently lowers the signature *and* checks the body in one call.
  Predeclaring callable types must therefore separate reserve/fill from body checking or
  otherwise prove that bodies, diagnostics, generic ids, and obligations are produced
  exactly once — `src/check/checker/calls.rs:986`, `src/check/checker/calls.rs:1020`.
- ✔ Every block and loop head gets a lexical `Block` scope, and `bind_declarator`
  declares the name directly in the scope it receives. The binder does not receive the
  declaration kind there, so `var`, `let`, and `const` currently take the same path —
  `src/binder/bind.rs:413`, `src/binder/bind.rs:447`,
  `src/binder/bind.rs:478`, `src/binder/bind.rs:502`.
- ✔ Function bodies already have an explicit `Function` scope, so nearest
  function/module placement can reuse the existing scope graph rather than add another
  ownership model — `src/binder/bind.rs:560`, `src/binder/bind.rs:618`,
  `src/binder/scope.rs:17`.
- ✔ Declaration initializers are visited at their source position and do not create a
  narrowing assignment in the flow graph. Hoisting a `var` binding must preserve that
  behavior — `src/check/checker/flowgraph/mod.rs:39`,
  `src/check/checker/flowgraph/mod.rs:47`.
- ✔ The two divergences are live and release-owned by backlog `74`: forward function
  calls are an under-report; block-scoped `var` is an over-report —
  `docs/reference/divergences.md:85`, `docs/reference/divergences.md:91`.

## Work units

### WU0 — spec-only `b74_declaration_hoisting` corpus (effort M)

- **Problem.** The current witnesses live indirectly under the project-scope corpus and
  do not pin the full declaration-order contract: ordinary/generic/overloaded forward
  calls, implementation-signature hiding, function/block boundaries, or `var` placement
  across all supported containers.
- **Verify first.** Run focused scratch probes through current typokat and
  `tsc 6.0.3 --strict`. Record exact pre-fix misses/over-reports and avoid fixtures that
  depend on backlog `47` definite-assignment/TDZ diagnostics. Confirm which exported
  declaration wrappers and loop/switch shapes reach the same paths at HEAD.
- **Scope.** Add a disabled `tests/cases/b74_declaration_hoisting/` corpus and register it
  `false` in `MILESTONE_DIRS`. Cover: calls before and after ordinary declarations;
  generic calls with explicit/inferred arguments; ordered overload selection and
  `TK2769` before the implementation; recursive and mutually-referential functions;
  nested functions and blocks; `var` declared in a block, `if`, switch clause, and each
  supported loop head/body; module/function boundary isolation; and `let`/`const`
  controls that remain block scoped. Keep at most one mismatched argument per call.
- **Acceptance / witness.** Commit the corpus alone while disabled. Every expected
  diagnostic and clean control is cross-checked against `tsc 6.0.3 --strict`; enabling
  the directory at old HEAD fails for the intended forward-call and `var` cases, not for
  an unrelated unsupported surface.
- **Touch points.** `tests/cases/b74_declaration_hoisting/`, `tests/conformance.rs`,
  `tests/cases/README.md`, focused scratch probes only.

### WU1 — predeclare local function callable surfaces (effort L)

- **Problem.** Name resolution succeeds before a declaration, but its `DeclId` still has
  no type. The reference therefore resolves to the defensive error type, suppressing
  arity, argument, constraint, and overload diagnostics.
- **Verify first.** Trace ordinary, generic, ambient/declaration-only, exported, and M33
  overload declarations through `infer_function`, `generic_sig_params`, obligations,
  return inference, and `expose_overload_value`. Establish the smallest reserve/fill
  split that gives every declaration in one statement list a callable surface before any
  executable statement is checked. Prove with counters/tests that a function body and
  its diagnostics are not evaluated twice. If unannotated mutually-recursive return
  inference needs a new fixpoint architecture, stop and ask rather than adding one to
  this sprint.
- **Scope.** Reuse the existing statement-list and M33 grouping machinery to predeclare
  ordinary/generic/overloaded local functions before executable checking. Preserve
  source order within overload groups, hide implementation signatures from calls, keep
  type-parameter ids/constraints stable, and check each body exactly once. Any
  provisional callable state must be replaced before ordinary call checking and must
  not become a permissive final result. Apply the same rule to every statement-list
  context already handled by the shared walker; do not change module boundaries.
- **Acceptance / witness.** Run the forward-function fixtures directly while the
  combined `b74_declaration_hoisting` directory remains disabled until WU2:
  before/after calls produce identical call diagnostics;
  overload calls see only declared signatures; generic constraints and argument errors
  are preserved; recursion/reordering is deterministic; no duplicate body diagnostic or
  obligation appears. Existing M3/M9/M10/M24/M32/M33 and local-overload corpora remain
  verdict/order stable.
- **Touch points.** `src/check/checker/statements.rs`,
  `src/check/checker/calls.rs`, `src/check/checker/context.rs`, focused unit tests,
  `tests/conformance.rs`.

### WU2 — bind `var` in its containing function/module scope (effort M)

- **Problem.** The binder passes the current lexical block/loop-head scope to every
  variable declarator, so a `var` name cannot resolve outside that block even though its
  initializer must still be visited at the original source position and lexical scope.
- **Verify first.** Enumerate every `VariableDeclaration` entry path: ordinary/exported
  declarations, C-style loop init, `for-in`/`for-of` heads, blocks, switch clauses, and
  nested functions. Confirm the existing supported binding-pattern boundary before
  promising destructuring behavior. Probe same-name `var` declarations, parameter/var
  interactions, nested-function isolation, and block `let`/`const` shadowing against
  `tsc 6.0.3 --strict`.
- **Scope.** Thread declaration kind through existing binder paths. Declare a supported
  `var` binding in the nearest `Function` or `Module` scope, but bind/check its
  initializer from the original lexical scope and keep flow evaluation at the source
  statement. Leave `let`/`const` in the current block/loop scope. Reuse the existing
  symbol slots and parent links; do not add a parallel hoist table or definite-assignment
  state.
- **Acceptance / witness.** Enable the `var` subset of `b74_declaration_hoisting`:
  block/switch/loop `var` names resolve throughout only their containing function;
  initializers still resolve nearby lexical names correctly; nested functions do not
  leak bindings; `let`/`const` controls retain `TK2304`; declaration initializers do not
  introduce new narrowing. Existing binder, switch-scope, loop, CFG, and project-scope
  corpora remain green.
- **Touch points.** `src/binder/bind.rs`, `src/binder/scope.rs` only if a narrow
  nearest-scope helper is justified, checker lookup integration as needed, focused
  binder tests, `tests/conformance.rs`.

### WU3 — independent adversarial soundness review (effort M)

- **Problem.** Hoisting changes which declaration type a reference sees and when it is
  available; a locally green corpus can still hide order-dependent false negatives,
  expose an overload implementation signature, or leak a `var` across a function
  boundary.
- **Verify first.** A different review subagent starts from the WU0 contract and current
  diff, not from the implementation agent's rationale. Re-run all probes against
  `tsc 6.0.3 --strict`.
- **Scope.** Hunt forward/reordered calls, non-consecutive overloads, declaration-only
  signatures, generic constraints, inferred returns, recursion, duplicate declarations,
  parameter/var collisions, block shadowing, nested functions, exports, loop/switch
  containers, and diagnostic duplication/order. Audit that provisional function state
  cannot survive into a relation/call cache and that flow's declared-type base case sees
  the final declaration type. Classify every divergence as false negative, safe false
  positive, or documented out-of-scope behavior.
- **Acceptance / witness.** Reviewer returns PASS with concrete probes and zero
  unexplained typokat/tsc verdict mismatch. Any false negative returns to the original
  implementation subagent and is independently re-reviewed before the implementation
  commit.
- **Touch points.** Read-only whole diff, focused scratch probes, WU0 corpus, existing
  M33/scope/flow regression corpora.

### WU4 — full verification and sprint closure (effort M)

- **Problem.** Backlog `74` is not closed until both divergence families, the manifest
  owner, test registration, official-suite movement, and active planning docs agree with
  shipped behavior.
- **Verify first.** Review the spec/implementation/review commit map and compare official
  diagnostic identities against the committed scoreboard before accepting any re-save.
- **Scope.** Run formatting, focused conformance, the full test suite, clippy, release
  build, manifest/divergence validators, focused `tsc` probes, and the official-suite
  regression check. Remove or rewrite the two shipped divergence entries, delete backlog
  `74`, mark `C-declaration-hoisting` complete, update roadmap/index claims, prepend a
  factual OUTCOME, and archive this sprint. Save an official scoreboard only for audited
  intended progress; never refresh away a regression.
- **Acceptance / witness.** `b74_declaration_hoisting` is enabled; all required gates
  pass; official-suite identity movement is fully attributed; the executable manifest
  has no live owner link to deleted backlog `74`; docs consistently name the next honest
  preview prerequisite (`73` closure before `38` → `72`).
- **Touch points.** `docs/reference/divergences.md`,
  `docs/backlog/completion-1.0.toml`, `docs/backlog/README.md`,
  `docs/backlog/74-declaration-hoisting-parity.md`, `docs/INDEX.md`,
  `docs/sprints/README.md`, `docs/archive/`, official-suite scoreboard only if audited.

## Out of scope (explicit)

- Definite-assignment, use-before-assignment, and temporal-dead-zone diagnostics
  (`TK2448`/`TK2454`/`TK2564`) — backlog [`47`](../backlog/47-definite-assignment.md).
- Duplicate/redeclaration diagnostics (`TK2300`/`TK2451`) — backlog
  [`18`](../backlog/18-duplicate-identifier-detection.md); this sprint may preserve
  current merging behavior but must not claim those diagnostics.
- New return-path or recursive return-inference fixpoint semantics — backlog
  [`46`](../backlog/46-return-path-analysis.md) and existing inference policy.
- Destructuring binding completeness beyond the binder's supported binding-pattern
  boundary; verify and document the boundary rather than silently widening it.
- The surface-accounting emission tail (`73`), minimal ambient prelude (`38`), and
  real-project preview (`72`). They remain the following chain after this sprint.
- General module-resolution, namespace/declaration-merging, or `lib.d.ts` work.

## Decisions

- Treat this as two coordinated fixes, not one cross-layer hoisting subsystem:
  callable-type availability belongs to the existing checker reserve/fill path; `var`
  placement belongs to the existing scope graph.
- Preserve a single source-order execution/flow model. Hoisting changes visibility and
  declaration type availability only; it does not imply initialization or assignment.
- Reuse M33 overload grouping and existing symbol/`DeclId` storage. No parallel symbol
  table, generic second traversal, or new architecture boundary is permitted.
- Follow the mandatory dev loop: leader-written disabled corpus and spec commit;
  implementation through subagents; a different adversarial review subagent; fixes and
  re-review as required; leader-run verification and atomic explicit-path commits.

## Sequencing

| Order | Unit | Gate |
|---|---|---|
| 1 | WU0 | Disabled corpus committed independently and verified to fail at old HEAD |
| 2 | WU1 | Forward-function fixtures pass directly while the combined dir stays disabled; legacy function suites green; independent review |
| 3 | WU2 | Full `b74` corpus enabled; binder/scope/flow suites green; independent review |
| 4 | WU3 | Cross-cutting adversarial PASS; any false-negative fix re-reviewed |
| 5 | WU4 | Full gates, audited scoreboard, backlog/manifest/docs closure, sprint archived |

WU1 and WU2 are conceptually independent but run sequentially: both consume the same
corpus and final integration review, and the function under-report is the higher-priority
soundness fix.

## Run log

<!-- Append discoveries, deviations, and blockers here. Graduate durable findings to an
     ADR/backlog/reference document; leave only transient execution notes in this log. -->
