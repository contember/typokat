# Sprint — source-preserving `satisfies` checking (2026-08-17)

**Goal.** Ship the `satisfies` half of backlog `44`: contextually type and check the source
expression against the target while preserving the source's inferred type for downstream use.

**Why this is a separate sprint.** TypeScript added `satisfies` independently in
[`microsoft/TypeScript#46827`](https://github.com/microsoft/TypeScript/pull/46827). Its model change
is contextual typing plus one-way assignability. It has no semantic dependency on const contexts.
Keeping it separate makes the implementation-complexity comparison meaningful and avoids combining
two unrelated risks in one review.

Success means direct failures use `TK1360`, contextually owned property/element failures retain
`TK2322`, excess properties retain `TK2353`, the expression keeps its narrower/contextually shaped
source type, and the official `syntax:satisfies` gate is opened without hiding any newly exposed
false negative. Backlog `44` and completion criterion `A-satisfies-as-const` remain open until the
separate `as const` sprint also ships.

## Upstream comparison baseline

The upstream PR reports 102 changed files and `+3,172/-449`, but those totals are not a useful model
complexity measure:

- 82 test and generated-baseline files account for `+3,037/-443`.
- 20 `src/` files account for `+135/-6`; most integrate the new AST node into the parser, scanner,
  factories, emitter, transformer, language service, and public visitor APIs.
- The semantic checker change in `src/compiler/checker.ts` is only `+18/-0`: expose the target as
  contextual type, check source-to-target assignability with the existing elaboration path, return
  the source type, and add the checker dispatch.

Typokat delegates parsing and AST ownership to OXC and has no emitter, so only the last item is the
fair model comparison. Typokat adds its own cost in deferred obligation ownership, deterministic
diagnostic replay, incomplete-surface accounting, and the class/query invariants. The sprint outcome
must report production code, fixtures, generated/scoreboard changes, and assurance work separately;
one aggregate diffstat is explicitly insufficient.

## Refs re-verified at HEAD (2026-08-17, `857ef6f`)

- `TSSatisfiesExpression` currently walks both children, records
  `expr-infer/satisfies-expression/self`, and returns no type in ordinary, chain, and
  assignment-target-wrapper paths — `crates/typokat-check/src/check/checker/expr.rs:394-402`,
  `crates/typokat-check/src/check/checker/expr.rs:526-532`, and
  `crates/typokat-check/src/check/checker/expr.rs:605-607`.
- M30 already owns contextual fresh-object/array/tuple shaping, duplicate-safe contextual rewalk,
  recursive excess checks, and optional-style target peeling —
  `crates/typokat-check/src/check/checker/expr.rs:1124-1331` and
  `crates/typokat-check/src/check/checker/assignment.rs:640-775`.
- Assignability obligations distinguish assignment, argument, and fresh-argument diagnostics but
  have no `satisfies` mapping — `crates/typokat-check/src/check/checker/context.rs:56-82` and
  `crates/typokat-check/src/check/checker/statements.rs:3769-3847`.
- Pinned tsc 6.0.3 emits `TS1360` for direct scalar, missing-property, tuple-length, and whole-union
  failure; contextually typed property/element mismatch uses `TS2322`; excess properties use
  `TS2353`.
- `TK1360` is absent. Shipping it requires a documented narrow semantic exception beside `TK1192`
  — `crates/typokat-diagnostics/src/diagnostics/mod.rs:46-180` and
  `docs/reference/scope.md:29-48`.
- The surface inventory assigns the incomplete identity to backlog `44`; B73 contains current
  witnesses — `tests/surface/inventory.toml:732-740`,
  `tests/cases/b73_expression_shape_tail/satisfies.ts:5`, and
  `tests/cases/b73_expression_shape_tail/update_wrappers.ts:6-7`.
- Class-surface initializer inference rejects `TSSatisfiesExpression`, so an unannotated field would
  publish no source-preserving type unless the second publication path is updated atomically —
  `crates/typokat-check/src/check/checker/classes/initializer.rs:247-253`.

## Work units

### WU0 — commit the disabled RED `satisfies` corpus (effort M)

- **Verify first.** Cross-check every row with pinned `tsc 6.0.3 --strict --noEmit --pretty false`.
  Use declaration emit for inferred-type witnesses. Force-run the disabled directory on current
  typokat and prove every intended acceptance row is RED for the expected cause.
- **Scope.** Add disabled `tests/cases/b44_satisfies/` coverage for source retention; contextual
  scalar, object, tuple, nested literal, function/callback, readonly-target, and parenthesized
  shaping; exact `TK1360`/`TK2322`/`TK2353` ownership and spans; direct/nested excess; whole-union
  failure; downstream source use; unannotated class-field publication; unresolved/error recovery;
  diagnostic counts; and query/order permutations. Keep ordinary `as T`, no-context widening, and
  mutable results as controls.
- **Acceptance.** The spec commit is behavior-neutral. The disabled corpus is tsc-verified and
  forced RED without parser failure or unowned expectations.
- **Touch points.** `tests/cases/b44_satisfies/**`, `tests/conformance.rs`,
  `tests/cases/README.md`, and this sprint plan. Commit separately from implementation.

### WU1 — implement contextual source-preserving assignability (effort L)

- **Verify first.** Trace one clean tuple, property mismatch, missing property, excess property,
  and whole-union mismatch through raw inference, contextual rewalk, excess checking, deferred
  relation replay, span selection, and downstream use. Confirm which raw effect batch is retained
  or superseded before adding an obligation kind.
- **Scope.** Lower/demand the target through the existing annotation/query boundary. Infer the
  expression through the shared contextual fresh-literal machinery, run recursive excess checks,
  schedule one deferred semantic assignability obligation, and return the final source type. Add a
  `Satisfies` diagnostic mapping and `TK1360`. Route ordinary, chain, and assignment-target-wrapper
  syntax through one helper. Preserve reason chains, candidate-local effects, source order, and the
  coordinator-only relation boundary. Do not reuse bidirectional assertion compatibility.
  Publish the same source-preserving type through class-surface initializer inference; if the target
  or source is unavailable, withhold the whole surface rather than publishing a partial fallback.
- **Acceptance.** Enable the WU0 corpus. Clean rows retain the source type; failures match pinned
  tsc code/line; excess runs once; error recovery does not cascade; and query/order permutations
  cannot drop an obligation. Mark only `expr-infer/satisfies-expression/self` supported and remove
  its stale B73 markers.
- **Touch points.** `crates/typokat-check/src/check/checker/expr.rs`, `context.rs`,
  `statements.rs`, `classes/initializer.rs`, `crates/typokat-diagnostics/src/diagnostics/mod.rs`,
  `tests/surface/inventory.toml`, and the B73 witnesses.

### WU1b — preserve contextual literal provenance and diagnostic source sites (effort M)

- **Problem.** The first WU1 review proved that target-only widening loses the distinction between
  raw string/number literals, which contextual typing widens, and boolean or explicitly typed/
  asserted literal sources, whose exact types survive `satisfies`. It also proved that one-level
  source-member spans cannot identify a nested property or contextual arrow return.
- **Verify first.** Use the committed raw string/number controls, then pair them with otherwise
  identical raw booleans, literal-typed identifiers, literal assertions, tuple elements, and array
  elements. Put nested property keys and arrow returns on separate lines so the ordinary corpus
  observes their source sites; keep the direct operator's exact column in a focused span unit.
- **Scope.** Add a syntax-directed contextual provenance policy only for `satisfies`: widen raw
  string/number literals under base primitive targets; preserve booleans and exact types obtained
  from identifiers/assertions; recurse through parentheses and admitted object/array/tuple syntax.
  Carry nested source-member/return paths far enough to choose the tsc-owned source line. This does
  not implement backlog `111`'s call-site candidate provenance model or alter generic inference.
- **Acceptance.** Every WU0/WU1b row matches tsc code, count, type retention, and primary source
  line; direct non-fresh structures remain `TK1360`; ordinary assignment/argument contextual
  behavior is unchanged; and a second independent review returns PASS.
- **Touch points.** The WU1 expression/provenance/span helpers and their focused unit tests only;
  backlog `111`, call-site inference, fixtures, and expectations are read-only after this spec
  amendment.

### WU2 — independent adversarial review and inference gates (effort M)

- **Verify first.** Freeze the WU1 diff. Dispatch an independent review subagent that did not
  implement it, with WU0, pinned tsc, the semantic-query/type-store invariants, and the pre-change
  binary.
- **Scope.** Hunt source-type replacement; wrong top-level versus nested codes; duplicated or
  discarded contextual effects; generic/deferred target leakage; provisional relation-cache
  writes; parenthesis/chain/wrapper bypasses; excess duplication; and error/unknown/any recovery
  that suppresses an independent diagnostic; and class publication that widens, replaces, or
  partially publishes the source type. Cross-check every repro with tsc.
- **Acceptance.** No unresolved HIGH or MEDIUM finding. Run focused B44/B73/surface/diagnostic
  tests, then CPU-leased full tests, ordinary exact conformance, clippy, format, and diff checks.
  Because contextual typing changes, run the required differential repro gate and seeds 1/2/3 ×
  400 against an isolated pre-change binary; triage every delta.

### WU3 — open the official `satisfies` gate and record the comparison (effort M)

- **Verify first.** Re-fetch the pinned official corpus and enumerate every row whose first bucket
  changes when only `syntax:satisfies` is removed. Record expected diagnostics, typokat output,
  remaining incomplete identities, and the first unsupported owner.
- **Scope.** Remove only the `satisfies` regex. Fix in-scope failures; keep independently owned
  unsupported rows explicit. Update the syntax-bucket unit test to expect no gate and remove the
  stale gate from the harness README. Run `run --check`, then `run --save` only after identity-by-
  identity review. Update `TK1360` scope/diagnostic documentation and current coverage. Do not
  delete backlog `44` or complete `A-satisfies-as-const`.
- **Comparison receipt.** In this file's run log and archived outcome, record: semantic production
  files and changed lines, model touch points, fixture lines, generated/scoreboard lines, review
  findings, and wall/CPU gate cost. Compare the semantic production slice to upstream checker
  `+18/-0`; explain architectural causes for every material difference.
- **Acceptance.** Full official-suite check has zero regressions, every moved row has an explicit
  disposition, living docs state that only the `as const` half of backlog `44` remains, and docs
  validation passes.
- **Touch points.** `tooling/official-suite/tsofficial.py`, `test_tsofficial.py`, `README.md`, and
  `scoreboard.txt`; `docs/reference/scope.md`; current coverage/fixture docs; this sprint and docs
  indexes.

## Out of scope

- `as const`; it has its own sprint and model comparison.
- General `as T` / angle-assertion compatibility (backlog `75`).
- New relation kinds, direct `Relater` construction, parser/emitter work, project/module breadth,
  and performance refactors.
- Backlog `76` lazy declaration/value-type resolution.

## Sequencing

1. Commit this plan and WU0 as the standalone acceptance spec.
2. Route WU1 to one implementation subagent.
3. Freeze the diff and route WU2 to a different independent reviewer.
4. Complete WU3, archive this sprint, and leave backlog `44` open for the ordered `as const` sprint.

The `satisfies` sprint runs first because it is the smaller upstream semantic change and establishes
a clean architecture-overhead baseline. The `as const` sprint follows; both touch `expr.rs` and the
official harness, so they are not parallel implementation units.

## Run log

<!-- Append as work proceeds. Graduate durable decisions/findings to ADR/backlog and trim this log
     to pointers. -->

- 2026-08-17: WU1 first review FAIL exposed contextual-provenance, direct-vs-contextual diagnostic,
  union-headline, and source-span defects. The implementation agent fixed the direct/contextual
  obligation provenance and headline issues in one fix round.
- 2026-08-17: independent re-review remained FAIL: target-only widening destroyed boolean and
  typed/asserted literal provenance, while nested member/arrow/operator spans remained shallow.
  Per orchestration policy, work stopped before a production commit. The user approved this WU1b
  reslice; it is explicitly narrower than backlog `111`'s call-site inference model.
