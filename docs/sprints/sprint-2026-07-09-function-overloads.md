<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME - shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 -> <sha>,
> WU2 -> <sha>, ... Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint - function overloads (2026-07-09)

**Goal.** Ship backlog `40` as M33: ordered overload declarations and overload call
resolution, including `TK2769`, without exposing the implementation signature as the
externally callable signature.

**Theme.** This is the first remaining track-A blocker for full `lib.d.ts`. HEAD has
several explicit single-signature gates across binder, type identity, call checking,
and relation, so overloads should land as one coordinated type-model slice rather
than scattered special cases.

## Refs re-verified at HEAD (2026-07-09)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `ObjectType` already stores `Vec<TypeId>` for call and construct signatures,
  and the comments explicitly reserve longer lists for overload work; current
  lowerers still only create the singleton subset - `src/types/repr.rs:299`,
  `src/types/repr.rs:315`.
- ✔ Function declarations allocate one value `DeclId` per declaration, but a same-name
  declaration in the same scope overwrites the value slot, so overload declarations
  are not grouped at the binder boundary - `src/binder/bind.rs:386`,
  `src/binder/bind.rs:547`.
- ✔ The statement checker stores one `fn_ty` for the resolved function declaration
  and at most one generic template signature per value `DeclId` -
  `src/check/checker/statements.rs:251`, `src/check/checker/context.rs:132`.
- ✔ Object and interface lowerers already detect overloaded methods and overloaded
  call/construct signatures, but they currently skip or abort those shapes rather
  than preserving the ordered signature list - `src/check/checker/decls/interface.rs:61`,
  `src/check/checker/annotations/composites.rs:108`,
  `src/check/checker/annotations/signatures.rs:145`.
- ✔ Ordinary call resolution extracts a single function or object call signature;
  non-callables still silently return the error type until the dedicated
  callability/overload diagnostics land - `src/check/checker/calls.rs:214`,
  `src/check/checker/calls.rs:306`.
- ✔ `new` through object construct signatures has the same singleton assumption -
  `src/check/checker/calls.rs:698`.
- ✔ Relation requires singleton call/construct signature slices when relating
  callable/constructable object types or a callable object to a plain function -
  `src/relate/relation/objects.rs:252`,
  `src/relate/relation/objects.rs:302`.
- ✔ `TK2769` is in the diagnostic scope map, but `DiagnosticCode` has no `TK2769`
  variant or helper yet - `docs/reference/scope.md:53`,
  `src/diagnostics/mod.rs:20`.
- ✔ The shared call checker already centralizes arity, rest/optional target mapping,
  contextual fresh-literal obligations, and argument relation diagnostics; overload
  resolution should reuse that machinery through side-effect-free candidate trials
  rather than duplicating relation rules - `src/check/checker/calls.rs:365`.

## Work units

### WU1 - Disabled M33 corpus and tsc overload matrix (effort M)

- **Problem.** Backlog `40` has probes but no acceptance corpus. Current fixtures
  avoid overload syntax, so changing overload resolution can regress ordinary calls
  without a focused witness.
- **Verify first.** Cross-check a scratch matrix with `tsc 6.0.3 --strict --noEmit`:
  free-function overload order, implementation-signature non-callability, no-match
  calls, implementation incompatibility, interface/object call signatures, construct
  signatures, class/interface methods, generic free-function overloads, and controls
  for existing single-signature behavior.
- **Scope.** Add disabled `tests/cases/m33_function_overloads/` fixtures and register
  the directory as `false`. Cover first-match selection, return-type narrowing from
  selected overloads, no-match `TK2769`, implementation signature compatibility
  (`TK2394` if implemented in this sprint), method/property overload forms, object and
  interface call/construct signatures, constructor overloads when syntax is available,
  and generic free-function overload smoke tests that reuse M9/M10 machinery.
- **Acceptance / witness.** `cargo test` is behavior-neutral while the directory is
  disabled. Enabling it at HEAD fails on the pinned FP/FN families, especially
  implementation-signature leakage and skipped object/interface overload signatures.
- **Touch points.** `tests/cases/m33_function_overloads/**`, `tests/conformance.rs`,
  `tests/cases/README.md`, scratch `tsc` probes.

### WU2 - Overload representation and declaration grouping (effort L)

- **Problem.** The binder/checker currently collapses same-name function declarations
  to the last value declaration and object/interface overloads to no represented
  callability. Later call resolution cannot recover declaration order or the
  implementation-vs-overload split.
- **Verify first.** Add small unit probes around binder/checker grouping before call
  semantics: same-name declarations must preserve source order; the implementation
  signature must be stored separately from the externally callable overload list; a
  non-overloaded function must keep its existing singleton representation.
- **Scope.** Group contiguous same-name function overload declarations in declaration
  order. Preserve an explicit implementation signature for body checking and
  compatibility checks, but expose only overload signatures to callers. Prefer using
  existing `ObjectType.call_signatures` / `construct_signatures` as the ordered
  overload carrier for overloaded callable values; introduce a new function-overload
  node only if the object carrier cannot preserve the required identity or generic
  metadata without contorting the model. Carry hash-consing, substitution, rendering,
  and declaration metadata through the chosen shape.
- **Acceptance / witness.** Unit tests prove overload-set identity includes ordered
  signature lists, singleton functions still intern/render as before, implementation
  signatures are not selected by external calls, and generic free-function overload
  metadata remains tied to the selected signature rather than a global callee template.
- **Touch points.** `src/binder/bind.rs`, `src/binder/symbol.rs`,
  `src/check/checker/statements.rs`, `src/check/checker/context.rs`,
  `src/types/repr.rs`, `src/types/hash.rs`, `src/types/intern/composites.rs`,
  `src/types/substitute/apply.rs`, `src/diagnostics/render_type.rs`,
  `src/types/intern/tests.rs`.

### WU3 - Overload call resolution and diagnostics (effort L)

- **Problem.** `infer_call` chooses one callable signature before checking arguments,
  so there is no place to try candidates in order, suppress failed-candidate side
  effects, or emit `TK2769` with the best failure when no overload matches.
- **Verify first.** For every WU1 no-match case, record which tsc diagnostic wins:
  arity-only failures, wrong argument type, generic inference failure, fresh literal
  excess, and mixed overload failures. Keep exact wording expectations narrow enough
  that `TK2769` parity is about the verdict and stable substrings, not the full tsc
  prose tree.
- **Scope.** Add a side-effect-free candidate trial path for call arguments: infer or
  instantiate a candidate signature, evaluate its parameters, compute arity and
  per-argument relation/excess failures, and commit obligations/diagnostics only for
  the selected candidate. Walk overloads in tsc order; first successful candidate
  wins. If none match, emit one `TK2769` diagnostic with a useful best-candidate
  elaboration and no stray `TK2345`/`TK2554` diagnostics from failed candidates.
  Explicit type arguments and M10 inference must run per candidate, not once for the
  overload set.
- **Acceptance / witness.** M33 free-function overload fixtures pass; existing M3,
  M9, M10, M24, M30, M32, and b65 call fixtures keep their intended diagnostics.
  No successful overload call reports candidate-trial obligations from rejected
  overloads.
- **Touch points.** `src/check/checker/calls.rs`, `src/check/infer/mod.rs`,
  `src/check/infer/context.rs`, `src/diagnostics/mod.rs`,
  `src/diagnostics/tests.rs`, `tests/cases/m3_functions/`,
  `tests/cases/m9_generics/`, `tests/cases/m10_inference/`,
  `tests/cases/m24_generic_constraints/`, `tests/cases/m32_signature_shape/`,
  `tests/cases/b65_inference_candidate_policy/`.

### WU4 - Object/interface/class overload surfaces and relation parity (effort L)

- **Problem.** Object/interface call signatures, construct signatures, and methods
  are deliberately singleton today. Even if free calls work, assignability and member
  call surfaces can still silently drop overloads or reject legal overload-bearing
  types for the wrong reason.
- **Verify first.** Build a tsc matrix for callable-object assignability: overloaded
  source to single target, single source to overloaded target, overload-set to
  overload-set, method overloads, construct signatures, implementation-incompatible
  declarations, and return-type covariance. Keep `strictFunctionTypes` on.
- **Scope.** Lower overloaded object/interface call and construct signatures into
  ordered lists instead of skipping them. Lower class/interface method overloads where
  the signatures are non-generic and representable; keep method-level type parameters
  deferred to backlog `41`. Define relation rules from the tsc matrix, using
  safe-direction over-reporting for any unresolved assignability corner rather than a
  permissive all-to-all shortcut. Check overload implementation compatibility against
  the implementation signature and emit the chosen diagnostic code.
- **Acceptance / witness.** M33 object/interface/class overload fixtures pass; F1
  callable-object fixtures and b06 class method override fixtures stay green; relation
  cache invariants remain unchanged.
- **Touch points.** `src/check/checker/annotations/composites.rs`,
  `src/check/checker/annotations/signatures.rs`,
  `src/check/checker/decls/interface.rs`, `src/check/checker/classes/mod.rs`,
  `src/relate/relation/objects.rs`, `src/relate/relation/tests.rs`,
  `tests/cases/f1_object_interface_call/`,
  `tests/cases/f1_object_interface_construct/`,
  `tests/cases/b06_class_completeness/`.

### WU5 - Independent review, docs, backlog close, and ratchet (effort M)

- **Problem.** Overload resolution crosses call checking, inference, relation, and
  identity. A local green run can still hide silent false negatives if failed
  overload candidates leak obligations or if implementation signatures remain
  callable.
- **Verify first.** Run the independent adversarial review required by the dev
  method, focused on false accepts from implementation-signature leakage, dropped
  `TK2769`, generic candidate reuse across overloads, overload-set assignability, and
  object/interface skipped-signature regressions.
- **Scope.** Enable `m33_function_overloads`, delete backlog `40`, update
  `README.md`, `docs/reference/divergences.md`, `docs/reference/scope.md` if the
  emitted surface changes, `tests/cases/README.md`, `docs/INDEX.md`, and the official
  suite scoreboard if audited changes occur.
- **Acceptance / witness.** `cargo test`, `cargo test conformance`,
  `cargo clippy --all-targets -- -D warnings`, focused typokat/tsc probes, and
  official-suite `run --check` are green or have an audited safe-direction ratchet.
- **Touch points.** `README.md`, `docs/reference/divergences.md`,
  `docs/reference/scope.md`, `docs/backlog/40-function-overloads.md`,
  `docs/INDEX.md`, `tests/cases/README.md`, `tooling/official-suite/`.

## Out of scope (explicit)

- Generic methods / method-level type parameters (`41`) except for proving that this
  sprint's method-overload lowering rejects or defers them soundly.
- Enums (`42`), namespaces + declaration merging (`43`), `satisfies` / `as const`
  (`44`), and full `lib.d.ts` loading (`14`).
- Dedicated callability diagnostics (`19`: `TK2349` / `TK2348` / `TK2351`) beyond
  what is required to report `TK2769` for overload no-match calls.
- Overload intersections (`A & B` callable as an overload set), `Function.prototype`
  `.call` / `.apply` / `.bind`, and spread call expressions.
- Remaining track-C silent-FN tail: `56`, `60`, `62`, `32`, `21`, `22`, `66`, `67`,
  plus parity tail `68`, `69`, `63`.
- Any permissive fallback that accepts a call because overload resolution could not
  decide. Unknown cases must over-report or stay out of subset.

## Decisions

- **Planning tier:** Tier 1. The plan is reversible, but the fork is real:
  continue the `lib.d.ts` critical path or spend another sprint on the remaining
  silent-FN tail. Default is backlog `40` because root/project docs and
  `docs/INDEX.md` both point to track A as the next completion path, and overloads
  are a direct blocker of `14`.
- Treat overload declarations as ordered external signatures plus one separate
  implementation signature. The implementation signature is for body checking and
  compatibility, not call resolution.
- Default representation is the existing object call/construct signature vectors
  for overload-bearing callable values; add a new function-overload node only if the
  object carrier cannot preserve identity or generic metadata cleanly.
- `TK2769` is the no-match overload verdict. Failed candidate diagnostics are trial
  data until the resolver decides no overload matched.
- **Falsifiability.** This plan is wrong if the WU1 tsc matrix proves generic
  overloads are inseparable from method-level type parameters (`41`) for the required
  acceptance surface. In that case, rescope M33 to representation plus non-generic
  overload resolution and file/adjust the follow-up instead of widening the sprint
  with a permissive shortcut.

## Sequencing

| Order | WU | Rationale |
|---|---|---|
| 1 | WU1 corpus | The tsc matrix is the acceptance spec and pins the no-match verdicts. |
| 2 | WU2 representation | Call resolution needs ordered signatures and implementation separation first. |
| 3 | WU3 resolver | Free-function calls are the core behavioral surface and exercise inference. |
| 4 | WU4 object/class relation | Once calls work, extend the same shape through object/interface/class relation surfaces. |
| 5 | WU5 review/docs | Close only after independent false-negative review and official-suite audit. |

Parallelism: WU1 fixture authoring can split by surface (free function, object,
class, generic). WU2-WU4 should stay in one implementation context because the
representation and trial-checking side effects are tightly coupled. The independent
review must be done by a different agent after WU3/WU4 and before deleting backlog
`40`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* -> ../decisions/NNNN ; new future work -> ../backlog/NN ;
     transient -> leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("-> ADR-0007"). -->
- **WU1 probes.** Cross-checked overload fixture candidates with `tsc 6.0.3
  --strict --noEmit`: same-arity type mismatches and implementation-signature leaks
  report `TS2769`; pure overload arity failures stay `TS2554`; incompatible
  overload declarations report `TS2394`; generic methods remain backlog `41`.
