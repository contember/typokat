<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 → <sha>,
> WU2 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — object/interface signatures (2026-06-28)

**Goal.** Ship backlog [`05`](../backlog/05-object-interface-signatures.md): method, call, and
construct signatures inside object type literals and interfaces no longer collapse the type to `{}`.

**Theme.** This is the next highest-value step because it is the first remaining official-suite
finding (`F1`) after the current-impl bug sprint, it is narrower than the flow-node CFG, and it
removes a recurring false-negative source in `assignmentCompatWith*Signatures*` without starting the
type-level evaluation phase. Success means the hand-written corpus proves the feature, the official
suite shows progress on the targeted signature files with no regression, and existing object,
interface, function, class-method, generic, optional, readonly, index-signature, and relation tests
stay green.

## Refs re-verified at HEAD (2026-06-28)

Re-read the load-bearing facts in the actual code before planning on them.
`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ **Object type literals reject signature members today** —
  `src/check/checker/annotations.rs:181-183` says call/method/construct signatures abort lowering,
  and `annotations.rs:190-221` handles only property signatures and index signatures before
  `_ => return None`.
- ✔ **Interface bodies skip signature members today** —
  `src/check/checker/decls.rs:363-419` lowers property signatures and index signatures, while
  `_ => continue` drops call/method/construct signatures, leaving otherwise-signature-only
  interfaces as empty objects.
- ✔ **`ObjectType` has no callable/constructable slots yet** —
  `src/types/repr.rs:288-303` stores named properties plus string/number index signatures only;
  `FunctionType` is a separate row kind at `repr.rs:377-390`.
- ✔ **Relation currently compares objects and functions as separate shapes** —
  `src/relate/relation.rs:388-395` dispatches object-vs-object and function-vs-function separately,
  and `relation.rs:468-533` checks only target named properties for object assignability.
- ✔ **Ordinary calls only accept `TypeTag::Function`** —
  `src/check/checker/calls.rs:207-212` returns the error type for any non-function callee after
  still checking callee/argument side effects.
- ✔ **`new` currently means class construction only** —
  `src/check/checker/calls.rs:359-388` resolves `new C(...)` through `class_ctors`; a non-class
  callee is out of subset and gets no construct-signature checks.
- ✔ **Class method signature lowering already builds function-typed properties** —
  `src/check/checker/classes.rs:636-650` lowers a method's annotated params/return into an interned
  `FunctionType`, which is the model to reuse for object/interface method signatures.
- ✔ **Rendering needs an object update once callable/constructable slots exist** —
  `src/diagnostics.rs:563-585` renders object types from index signatures and named properties only;
  function rendering is separate at `diagnostics.rs:595-605`.
- ✔ **Official-suite baselines show the current target files** —
  `tooling/official-suite/scoreboard.txt:676-685` includes
  `assignmentCompatWithCallSignatures*.ts` and `assignmentCompatWithConstructSignatures*.ts`
  in-scope with known false negatives; `scoreboard.txt:591-592` tracks object type literal method
  signatures.

## Work units

Each WU follows [`../reference/dev-method.md`](../reference/dev-method.md) §1:
leader writes the fixture spec first (new dir, registered `false`, committed on its own) →
implementation flips it `true` and leaves gates green → independent adversarial review hunts false
negatives and cross-checks `tsc --strict` → fix → leader verifies and commits.

### WU1 — Method signatures become function-typed properties (effort M)

- **Problem.** `interface I { f(x: number): string }` and `{ f(x: number): string }` do not create a
  member `f`; interfaces silently drop it (`decls.rs:419`) and object type literals abort the whole
  annotation (`annotations.rs:221`). This produces empty-object rendering, missing-property errors
  on reads, and false negatives when assigning an object with an incompatible `f`.
- **Verify first.** Probe `tsc --strict` for the exact function type of method signatures:
  one required parameter, omitted return type (`void`), generic-free method, optional/rest params
  out of scope. Confirm whether method parameter names matter only for display, as existing
  `FunctionType` relation already ignores names.
- **Scope.** Lower non-overloaded, non-generic `TSMethodSignature` members with static names in both
  object type literals and interfaces. Reuse the class-method signature lowering shape: params from
  annotations, unannotated params as error type, return annotation or `void`, stored as
  `PropertyType::public(name, function_ty)`. Preserve optional/readonly/index behavior already in
  these lowerers.
- **Acceptance / witness.** New dir `tests/cases/f1_object_interface_signatures/` with fixtures for
  interface method access/call, object-type-literal method access/call, incompatible method parameter
  and return assignments (`TK2322`/`TK2345` as tsc does), and a property plus method coexisting.
  The spec commit registers the dir disabled; the implementation commit flips it enabled.
- **Touch points.** `src/check/checker/annotations.rs`, `src/check/checker/decls.rs`,
  `src/check/checker/classes.rs` or a shared helper for signature lowering, existing
  `FunctionType`/relation/call machinery.

### WU2 — Single call signatures make objects callable (effort L)

- **Problem.** `{ (x: number): string }` / `interface Fn { (x: number): string }` have no place in
  `ObjectType`, and `infer_call` accepts only bare `FunctionType`. Typokat therefore either drops the
  signature or treats the object as non-callable, missing argument/return diagnostics.
- **Verify first.** Cross-check tsc for the minimal subset: a single call signature, optional named
  properties next to it, assignment from a function value to a callable object with no required
  properties, and assignment from a callable object back to a function type. Check how many
  diagnostics the local official-suite target `assignmentCompatWithCallSignatures2.ts` currently
  misses before the edit.
- **Scope.** Extend `ObjectType` with a first-class call-signature slot/list and include it in
  hashing, interning, substitution, rendering, and relation. `infer_call` should select an object's
  single call signature and run the existing arity/argument checks, returning the signature return
  type. Support only non-overloaded, non-generic call signatures in this WU; multiple signatures are
  deferred unless they collapse trivially to the same shape.
- **Acceptance / witness.** Fixtures prove calls through annotated callable interfaces/object types,
  wrong arity (`TK2554`), wrong argument (`TK2345`), return assignment failures (`TK2322`), function
  value assignability to a callable object with no required properties, and rejection when required
  named properties are missing. Targeted official-suite rows for call signatures improve or stay
  non-regressing; full `run --check` has zero regressions.
- **Touch points.** `src/types/repr.rs`, `src/types/hash.rs`, `src/types/intern.rs`,
  `src/types/substitute.rs`, `src/diagnostics.rs`, `src/check/checker/annotations.rs`,
  `src/check/checker/decls.rs`, `src/check/checker/calls.rs`, `src/relate/relation.rs`.

### WU3 — Single construct signatures make object types constructable (effort M/L)

- **Problem.** `interface Ctor { new (x: number): Box }` and `{ new (x: number): Box }` are dropped
  during lowering, and `infer_new` only recognizes class identifiers from `class_ctors`. This misses
  construct-signature assignment and `new c(...)` argument/return diagnostics.
- **Verify first.** Cross-check tsc for a single construct signature with a declared object return
  type, `new c(bad)` argument failures, and construct signature assignability. Confirm whether an
  ordinary function value should be considered constructable in the current subset; if that depends
  on JS runtime semantics, keep it out of scope and document the divergence.
- **Scope.** Add construct signatures alongside call signatures in `ObjectType` and relation. Extend
  `infer_new` so a non-class callee whose inferred type is an object with a single construct
  signature runs the existing `check_call_arguments` path and returns the construct signature's
  declared instance type. Keep class construction behavior unchanged and higher precedence.
- **Acceptance / witness.** Fixtures cover construct signatures in interfaces and object type
  literals, `new ctor(ok)` result member access, wrong arity (`TK2554`), wrong argument (`TK2345`),
  result assignment (`TK2322`), and assignment compatibility for constructor-like object types.
  Targeted official-suite construct-signature rows improve or stay non-regressing.
- **Touch points.** Same representation/relation/rendering files as WU2 plus
  `src/check/checker/calls.rs::infer_new`.

### WU4 — Adversarial review + official-suite ratchet (effort M)

- **Problem.** Signature features touch type identity, relation caching, substitution, diagnostics,
  and call/new checking. The risk is not compile failure; it is a false negative from a missing
  relation obligation or a cache/identity mismatch.
- **Verify first.** Run the hand-written corpus and targeted official-suite rows before and after
  each implementation WU. Inspect `tooling/official-suite/report/latest.json` for newly in-scope
  signature files whose changed status is progress vs accidental scope drift.
- **Scope.** Independent reviewer replays the WU fixtures against `tsc --strict`, hunts false
  negatives in call/construct assignment both directions, checks recursive/generic interface
  substitution where signature parameter/return types mention `T`, and stress-tests object relation
  order/caching with signature-bearing interfaces. Save the official-suite scoreboard only after the
  implementation and review fixes are complete.
- **Acceptance / witness.** `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release`, and `cd tooling/official-suite && python3 tsofficial.py run --check` are
  green with zero regressions. If targeted rows improve, run `python3 tsofficial.py run --save` and
  include the scoreboard delta in the commit.
- **Touch points.** `tests/cases/README.md`, `tests/conformance.rs`,
  `tooling/official-suite/scoreboard.txt`, and any reference docs whose current behavior changes.

## Out of scope (explicit)

- **Overload sets and specialized signatures.** Multiple call/construct/method signatures are common
  in the official suite, but this sprint only guarantees a single non-generic signature. If overloads
  are cheap after the representation lands, they may be added only with their own fixtures and review.
- **Generic call/construct/method signatures.** Generic functions already exist, but generic
  signatures embedded in object/interface types interact with constraints and inference; defer unless
  the implementation naturally supports them without widening scope.
- **Optional/rest parameters.** The existing call checker assumes exact arity and no optional/rest
  params; do not change that contract in this sprint.
- **`this` parameters, type predicates, `abstract new`, and constructor runtime semantics.** These
  are outside the current checker subset and should not be smuggled in.
- **Class-completeness checks (`06`) and unstructured-flow narrowing (`07`).** They remain the next
  roadmap candidates after this sprint.

## Decisions

- **Choose backlog `05` before `06`/`07`.** Axes: official-suite value, scope size, dependency order,
  and architectural reversibility. `05` is the best next sprint because it is a known in-scope
  official-suite false-negative family and smaller than the CFG. `06` is cheaper but lower-value;
  `07` is more fundamental but a larger architecture sprint. This recommendation would be wrong if
  the representation spike shows call/construct signatures require overload/inference machinery now.
- **Treat this as F1 fix-style corpus, not milestone renumbering.** Use
  `tests/cases/f1_object_interface_signatures/` and register it disabled in the spec commit, then
  enabled in implementation. This preserves the existing roadmap label where unstructured-flow
  narrowing remains M23.
- **Represent call/construct signatures on `ObjectType`, not by pretending an object is a function.**
  Method signatures can be function-typed properties, but callable/constructable object types need to
  keep their named properties and index signatures. The representation must participate in hashing,
  substitution, rendering, and relation identity.

## Sequencing

| WU | Touches | Parallel? |
|---|---|---|
| WU1 | lowerers + existing function-typed properties | start first; mostly independent |
| WU2 | `ObjectType` representation + `infer_call` + relation | serialize before WU3 |
| WU3 | same representation + `infer_new` | after WU2 to avoid conflicting representation edits |
| WU4 | review + official-suite ratchet | after each WU review, final save at end |

WU1 can produce the first spec + implementation loop while WU2's representation is being designed,
but only one implementation should edit `ObjectType`/interner/hash/substitution/relation at a time.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
