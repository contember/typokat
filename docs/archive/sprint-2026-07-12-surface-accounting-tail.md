> **OUTCOME — shipped 2026-07-12.** The final expression-shape accounting tail now reports
> incomplete explicitly while preserving independently reachable diagnostics, and every remaining
> unsupported surface has a durable semantic owner. Three independent review/audit rounds found
> failures before PASS: update/direct optional access, nested optional chains, and private-field
> false positives. Commit map: plan `967fc56`; spec `45943bb`; review witnesses
> `7b11620`/`e1ecf87`/`5830b04`; implementation `0858508`; private-boundary fix `9ecd164`;
> official ratchet `45ac8d9`. Verification: `cargo test` (286 unit + 14 conformance + 4
> divergences + 7 incomplete + 10 manifest + 5 surface) · `cargo fmt --check` ·
> `cargo clippy --all-targets -- -D warnings` · `cargo build --release` · official-suite 874
> `run --check` (0 regressions). Ratchet: in-scope 422→358, unsupported 151→223; 64
> IN→unsupported + 7 unresolved→unsupported + 1 parse-error→unsupported; matched identities
> preserved, no new FP after remediation; clean-kept 126/159→100/132, diag-recall
> 379/1868→385/1880. Backlog `73` closed; `72` unblocked.

# Sprint — surface-accounting expression tail (2026-07-12)

**Goal.** Close backlog `73`: no inventoried in-scope expression shape may leave
`infer_expr` silently, and every remaining unsupported surface must have a durable semantic owner.

**Theme.** The accounting infrastructure already exists. This sprint finishes its last
false-clean boundary with explicit expression-shape handling, preserves nested diagnostics where
existing walkers can reach them, avoids turning routine `for` updates into noisy incomplete
results, and then hands semantic gaps to their long-lived backlog owners. It does not implement the
deferred expression semantics themselves.

## Refs re-verified at HEAD (2026-07-12)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `infer_expr` still ends in one `_ => None` wildcard after the already-accounted template,
  spread, elision, and computed-key paths — `src/check/checker/expr.rs:27-175`.
- ✔ All remaining OXC `0.137.0` expression variants are classified in the executable surface
  inventory, but fourteen shapes still lack an explicit `infer_expr` arm —
  `tests/surface/inventory.toml:437-826`, `src/surface.rs`.
- ⚠ `for_stmt.update` always enters `infer_expr`; emitting `update-expression/self` for every
  update would demote ordinary numeric `for` loops even when their operand is fully traversable —
  `src/check/checker/statements.rs:441`,
  `../archive/sprint-2026-07-12-surface-accounting-tail.md` (archived outcome).
- ✔ OXC exposes existing child expressions for the unsupported wrappers; update expressions are
  the exception because their operand is a `SimpleAssignmentTarget`, so they need a narrow target
  walker rather than a second expression traversal.
- ⚠ Thirty-six inventory records still name backlog `73` as their live owner. Most already emit;
  they need semantic ownership migration before `73` can be deleted without breaking the surface
  validator — `tests/surface/inventory.toml`.
- ✔ The current `b73_surface_accounting` corpus is enabled, so spec-first history requires a new
  disabled `b73_expression_shape_tail` directory — `tests/conformance.rs:129-135`.
- ✔ `73` is the sole declared blocker of the real-project preview, and the roadmap names
  `73` closure → `72` as the next executable chain —
  `docs/backlog/72-real-project-preview-readiness.md:1-4`, `docs/backlog/README.md`.

## Work units

### WU0 — spec-only expression-tail corpus (effort M)

- **Problem.** The inventory names the remaining shapes, but no disabled acceptance corpus pins
  exact incomplete identities, retained child diagnostics, or the `for`-update noise boundary.
- **Verify first.** Parse each proposed fixture with OXC and cross-check its verdict with
  `tsc 6.0.3 --strict`. Confirm at old HEAD that each target shape exits without its prescribed
  incomplete record or nested diagnostic, while supported controls retain their current verdict.
- **Scope.** Add disabled `tests/cases/b73_expression_shape_tail/`, register it as `false`, and
  document it. Cover: update operands; non-null and optional-chain wrappers; await/yield; tagged
  templates; `satisfies`; explicit instantiation; dynamic import; bigint and regexp literals;
  class expressions; private-field access; and private-in expressions. Child-bearing fixtures pin
  both the wrapper's incomplete identity and standalone child diagnostics reachable without
  implementing the wrapper's deferred semantics. Optional-chain fixtures explicitly do not require
  member/call compatibility after the optional boundary; they require structural child traversal
  plus the incomplete identity. Private-field access is record-only because its receiver can depend
  on unsupported binding patterns; private-in still traverses its ordinary right operand. Update
  fixtures pin a missing-name operand and a clean ordinary numeric `for` loop.
- **Acceptance / witness.** One behavior-neutral spec commit. Enabling the directory at old HEAD
  fails only because the new expectations are absent. Every expected incomplete identity matches
  `tests/surface/inventory.toml`; every ordinary diagnostic is confirmed against tsc.
- **Touch points.** `tests/cases/b73_expression_shape_tail/`, `tests/cases/README.md`,
  `tests/conformance.rs`.

### WU1 — explicit shape traversal and incomplete emissions (effort L)

- **Problem.** `infer_expr` silently returns `None` for the remaining shapes, so unsupported
  wrappers can still hide child errors and produce an untrustworthy clean result.
- **Verify first.** Inspect each OXC node's direct children and reuse an existing checker/lowering
  entry point where one exists. Confirm the update target variants can be covered without casts,
  `unsafe`, or a new traversal boundary.
- **Scope.** Replace the wildcard behavior with explicit arms. Child-bearing unsupported wrappers
  structurally traverse their existing child expressions/annotations, record exactly one stable
  self identity, and return `None`; leaf literals record and return `None`; class expressions record
  without partially invoking top-level class lowering. Once a `ChainExpression` crosses an optional
  member/call boundary, traverse the rest of that chain structurally without ordinary member/call
  compatibility, which belongs to backlog `49`. Treat private-field access as an opaque record-only
  boundary rather than exposing binder deferrals as user diagnostics. Add a narrow
  `SimpleAssignmentTarget` walker for
  update operands, return the existing coarse error type, and change the inventory identity from
  `update-expression/self` to supported `update-expression/operand`. Update-operator type
  validation remains backlog `45`. Update inventory witnesses and the historical census slot name.
  Enable the WU0 corpus.
- **Acceptance / witness.** The new corpus passes; routine numeric `for` loops remain complete;
  each unsupported wrapper records one deterministic identity; standalone nested diagnostics are
  retained without optional-chain false positives; all existing conformance and surface-validator
  tests pass.
- **Touch points.** `src/check/checker/expr.rs`, `tests/surface/inventory.toml`,
  `tests/surface/census.md`, `tests/conformance.rs`.

### WU2 — independent adversarial review (effort M)

- **Problem.** Wrapper recursion can duplicate records, returning `None` can still discard child
  diagnostics, and optional/update paths can accidentally acquire unsupported semantics.
- **Verify first.** A reviewer independent of WU1 starts from the committed WU0 spec, reads the
  uncommitted implementation diff, and creates fresh probes against `tsc 6.0.3 --strict`.
- **Scope.** Hunt duplicate/inconsistent records, child-diagnostic loss, all `ChainElement` and
  `SimpleAssignmentTarget` variants, nested tagged/import/instantiation/satisfies shapes,
  private-expression bases, class-expression boundary leaks, span/id nondeterminism, and ordinary
  `for`-loop regressions. Any FAIL receives a focused regression witness before remediation and is
  re-reviewed by the same independent reviewer.
- **Acceptance / witness.** Explicit PASS with probes and commands; no unexplained false clean,
  false positive, duplicate identity, architecture expansion, or invariant regression.
- **Touch points.** Read-only WU0/WU1 diff and scratch probes; focused fixtures only for confirmed
  failures.

### WU3 — ownership migration, ratchet audit, and closure (effort L)

- **Problem.** Deleting backlog `73` would leave thirty-six `unsupported-in` inventory records and
  the preview dependency pointing at a dead owner.
- **Verify first.** Enumerate every `owner = ...73...` record and every repository reference to the
  backlog path; confirm each replacement owner already covers the semantic family. Run manifest,
  surface, and divergence validators before deleting anything.
- **Scope.** Move binding-pattern ownership to `48`; loop flow to `51`; optional/non-null to `49`;
  predicates to `50`; type-query resolution to `52`; traversal/spread/iteration positions to `71`;
  regexp to `14`; and the remaining ownerless async/generator/private/literal/class/exception
  semantic tail to `75`, expanding those backlog descriptions concisely. Remove the redundant
  never-emitting `annotation-lower/type-query/self` record. Mark
  `C-unsupported-surface-audit` shipped, remove `73` from `72`'s dependencies, delete backlog `73`,
  audit all links, and re-baseline the official suite only for explained identity movement.
- **Acceptance / witness.** No live reference to backlog `73`; validators pass; `72` is unblocked;
  official-suite changes are aggregated by incomplete identity with no lost matched diagnostic,
  new false positive, or unexplained IN→OOS move. Stamp the outcome and archive this sprint.
- **Touch points.** `tests/surface/inventory.toml`, `tests/surface.rs`, `tests/surface/`,
  `docs/backlog/{14,45,48,49,50,51,52,71,72,75}-*.md`,
  `docs/backlog/completion-1.0.toml`, `docs/backlog/README.md`, `docs/INDEX.md`, public/tooling docs,
  `tooling/official-suite/scoreboard.txt`, this sprint file.

## Out of scope (explicit)

- Semantic typing for `await`, `yield`, optional chains, non-null assertions, `satisfies`, explicit
  instantiation, dynamic imports, bigint/regexp values, private expressions, tagged templates, or
  class expressions; their existing/new semantic backlog owners remain live.
- Optional member/call compatibility after an optional-chain boundary, including argument checking
  against the non-null callable branch — backlog [`49`](../backlog/49-possibly-undefined-family.md).
  The chain records incomplete and only preserves diagnostics arising independently in its children.
- Binding object/array/rest patterns — backlog [`48`](../backlog/48-no-implicit-any.md). Private-field
  access must not emit `TK2304` merely because its receiver was introduced through an unsupported
  binding pattern.
- Update-operator operand compatibility and result fidelity (`TK2362`/`TK2365` family) — backlog
  [`45`](../backlog/45-operator-comparison-typing.md). WU1 only prevents operand traversal loss.
- Template/spread/iteration value semantics — backlog
  [`71`](../backlog/71-expression-inference-fn-tail.md).
- A binder incomplete channel, cross-role `requires_slots` validator architecture, second AST
  traversal, second flow model, or changes to relation/type-store/CFG invariants.
- Backlog `72` implementation. This sprint only removes its final declared blocker.

## Decisions

- Use explicit arms, never a catch-all incomplete emission. This keeps inventory/code drift visible
  and makes official-suite changes attributable by shape.
- Treat update expressions as a supported operand-traversal wrapper, matching the existing coarse
  treatment of arithmetic expressions; keep actual operator validation in `45`. This is the narrow
  path that preserves clean routine `for` loops without claiming `++/--` parity.
- Unsupported child-bearing wrappers retain standalone child diagnostics before recording their own
  incomplete identity. Optional chains are walked structurally after the optional boundary; applying
  member/call compatibility there would implement backlog `49`, so the incomplete result deliberately
  carries no such claim. No wrapper is promoted to semantically supported by traversal alone.
- Private-field access is record-only until binding-pattern and private-member semantics are modeled;
  traversing its receiver made unsupported object-rest declarations look like missing user names.
- Reuse existing backlog owners; no new semantic backlog is needed. Backlog `75` is the explicit
  owner for the remaining census-discovered surface families without a narrower owner.
- Stop rather than improvise if any shape needs a new traversal boundary, forbidden type workaround,
  or semantic implementation to expose its children soundly.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Disabled corpus committed independently and tsc-cross-checked. |
| 2 | WU1 | Terra implementation agent; no commit until focused tests and leader inspection pass. |
| 3 | WU2 | Different Terra reviewer; every FAIL remediated and re-reviewed before implementation commit. |
| 4 | WU3 | Full gates, ownership/link audit, official-suite identity audit, then closure/archive commit. |

Exact full gate: `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D warnings`;
`cargo build --release`; official-suite unit tests; fresh official-suite run and `run --check`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-12 — Plan grounded at `143a19f`; working tree clean. Terra read-only grounding confirmed
  existing OXC child accessors, the update-target granularity boundary, and that all current `73`
  inventory owners can migrate to existing backlogs.
- 2026-07-12 — WU0 spec shipped as `45943bb`; enabling it at old behavior produced exactly 24
  missing expectations across the fourteen planned shapes. Independent WU2 review then FAILED twice:
  update-target wrappers and direct optional access (`7b11620`), followed by nested optional chains
  (`e1ecf87`). The optional-call finding clarified the accounting boundary above rather than
  expanding this sprint into backlog `49` semantics.
- 2026-07-12 — WU1 + WU2 shipped as `0858508` after the third independent review PASS. The first
  full official-suite audit preserved every matched identity but FAILED on two new `TK2304` false
  positives from object-rest receivers under private-field access. Binder expansion was rejected as
  out of scope; regression witness `5830b04` pins the record-only boundary before remediation.
- 2026-07-12 — Private-field boundary remediation shipped as `9ecd164`; the official-suite ratchet
  `45ac8d9` verified the stated identity movement with no new false positive after remediation.
  Backlog `73` is closed, its ownership migrated to live items, and `72` is unblocked.
