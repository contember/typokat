# Sprint — generic methods and signatures (2026-07-12)

**Goal.** Close backlog `41` (and its narrow `23` symptom) by representing method-level generic
binders persistently across lowering, substitution, calls, overloads, and relation.

**Theme.** Generic free functions already have inference, constraints, explicit type arguments, and
overloads, but generic member/call/construct signatures are either dropped or replaced by a
conservative `never` surface. This sprint makes generic signatures a first-class part of the type
model and reuses the existing inference machine without weakening the relation-cache invariants. It
is the highest-leverage first Track A prerequisite of full `lib.d.ts`; it does not itself add array
members, ambient declarations, or close the blocked real-project preview.

## Refs re-verified at HEAD (2026-07-12)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ Backlog `41` owns class/interface/object generic methods, generic call signatures, generic
  construct signatures, inference, constraints, and relation; it subsumes the static-method
  `TK2304` symptom in backlog `23` — `docs/backlog/41-generic-methods.md`,
  `docs/backlog/23-static-method-type-params.md`.
- ✔ Generic methods are the dominant construct blocker in the pinned `lib.es5.d.ts` audit
  (`Array.map<U>`, `reduce<U>`, `Object.defineProperty<T>`, `freeze<T>`, `bind<T>`), while `43` and
  `70` remain separate mandatory blockers of `14` — `docs/backlog/lib-audit-6.0.3.md`,
  `docs/backlog/14-libdts-loading.md`.
- ⚠ Generic signature binders currently live only in a pass-local
  `TypeId → Vec<TypeParamId>` side map. They are not part of `FunctionType` or its structural hash —
  `src/check/checker/context.rs:224-227`, `src/types/repr.rs:457-471`,
  `src/types/hash.rs:168-183`.
- ⚠ Substituting an outer class/interface type parameter re-interns only a function's positional
  parameters and return type. A side-map-only generic-method implementation would therefore lose
  the method binder on `Box<T> → Box<number>` and silently make `Box<number>.map<U>` non-generic —
  `src/types/substitute/apply.rs:92-125`.
- ✔ The call candidate path already performs explicit type-argument lowering, constraint checking,
  inference, and substitution when it can recover persistent candidate-local parameters —
  `src/check/checker/calls.rs:314-387`.
- ✔ Free generic function publication is the only producer of the current side-map metadata;
  overload alignment also depends on exact template `TypeId` lookup —
  `src/check/checker/statements.rs:953-1018`, `src/check/checker/statements.rs:1216-1235`.
- ✔ Class method collection explicitly rejects every method with type parameters and replaces any
  overload set containing one with a `never` property; ordinary class method lowering has no
  method-scoped frame — `src/check/checker/classes/mod.rs:653-704`,
  `src/check/checker/classes/mod.rs:799-804`.
- ✔ Object/interface method, call, and construct signature lowering explicitly excludes generic
  declarations — `src/check/checker/annotations/signatures.rs:32-130`.
- ⚠ Function relation currently compares only positional parameters and returns. There is no
  binder-alignment rule, and the divergence ledger records generic-function-to-specific-signature
  assignment as a safe over-report — `src/types/repr.rs:457-471`,
  `docs/reference/divergences.md:514-539`.
- ✔ Array member access synthesizes only `.length`; generic methods do not by themselves make
  `array.map` available. That member projection and the full standard-library source remain owned
  by `14` — `src/check/checker/expr.rs:1112-1124`.
- ✔ The active real-project preview sprint is stopped at its zero-threshold witness gate. `41` is
  being run for Track A dependency leverage, not represented as a hidden relaxation or completion of
  `72` — `docs/sprints/sprint-2026-07-12-real-project-preview.md`.

## Work units

### WU0 — spec-only corpus and persistent-binder design gate (effort L)

- **Problem.** Backlog `41` has no acceptance corpus, and copying the existing pass-local side map
  into method lowering would lose binders after outer substitution and make relation results depend
  on ambient metadata not present in type identity.
- **Verify first.** Cross-check every proposed fixture with pinned `tsc 6.0.3 --strict`. Trace the
  old-HEAD result through class/interface/object signature lowering, substitution, member access,
  call candidate instantiation, overload selection, and function relation. Prove the sharp witness
  `class Box<T> { map<U>(f: (x: T) => U): U }` retains `U` after `Box<number>` substitution; include
  declaration-order/cache-order variants.
- **Scope.** Add disabled `tests/cases/b41_generic_methods/` fixtures and register them `false`.
  Cover instance/static class methods, generic methods inside generic classes, interface/object
  methods, generic object call signatures, generic construct signatures, method/class binder
  shadowing, explicit/inferred arguments, constraints and defaults, overload selection,
  alpha-equivalent generic-signature assignment, incompatible arity/constraints, inheritance, and
  recursive/cache-order controls. Write ADR-0005 selecting a persistent generic-signature
  representation that survives substitution and gives relation enough binder context without
  making cache verdicts depend on hidden mutable state. Explicitly reject a pass-only side map.
- **Stop gate.** If the representation requires a second inference engine, permissive
  instantiation, a relation verdict whose cache key omits binder context, a whole-project de Bruijn
  migration, or a change to the named-unique declaration parameter invariant, stop after the spec
  and ADR evidence and ask rather than improvise.
- **Acceptance / witness.** One behavior-neutral spec commit precedes implementation. Enabling the
  corpus at old HEAD fails only on the pinned generic-signature behavior; every marker matches tsc.
  The accepted ADR explains identity, structural hashing/equality, substitution, constraint access,
  alpha-renaming for relation, cache soundness, display, and migration of existing free functions.
- **Touch points.** `tests/cases/b41_generic_methods/`, `tests/cases/README.md`,
  `tests/conformance.rs`, `docs/decisions/0005-*.md`, `docs/decisions/README.md`.

### WU1 — persistent generic signature representation and lowering (effort L)

- **Problem.** Genericity is not stored with a function type, and all member/call/construct
  signature collectors discard method-level parameters before calls can see them.
- **Verify first.** Implement the WU0 representation proof against focused type-store tests first:
  structural identity includes the binder shape, equality agrees with hashing, substitution of an
  outer parameter preserves method binders and constraints, and unrelated declarations cannot
  capture one another's named `TypeParamId`s.
- **Scope.** Implement ADR-0005 in the type representation/interner/hash/equality/substitution/display
  path; migrate free generic functions off hidden pass-only identity. Add reusable signature lowering
  that allocates a fresh method-scoped parameter frame nested inside class/interface frames, lowers
  constraints/defaults, and produces persistent generic function types for class instance/static
  methods, interface/object methods, call signatures, and construct signatures. Preserve optional
  methods and explicit `this` parameters as their existing separate deferrals.
- **Acceptance / witness.** Focused representation tests pass; the `Box<number>.map<U>` template
  retains `U` after outer substitution; static type parameters no longer produce spurious `TK2304`;
  unsupported or malformed signatures never become permissive calls; old free-generic and M32/M33
  surfaces retain their diagnostic identities.
- **Touch points.** `src/types/{repr,hash,intern,substitute,store}.rs`, signature rendering,
  `src/check/checker/{context,calls,statements}.rs`,
  `src/check/checker/{classes,annotations,decls}/`, focused unit tests.

### WU2 — member-call inference, explicit arguments, and constraints (effort L)

- **Problem.** Member and object-call candidates can reuse the generic call machine only after their
  binders persist with the signature. Inference must not leak a class binder into a method binder or
  accept a failed constraint through fallback.
- **Verify first.** For each WU0 call fixture, inspect candidate construction before diagnostics.
  Compare implicit inference, explicit type arguments, missing/excess type arguments, defaults,
  contextual arguments, rest/optional parameters, and constraint failures with tsc.
- **Scope.** Route persistent generic member/call/construct signatures through the existing candidate
  instantiation and inference machinery. Apply explicit arguments and defaults by binder position,
  infer remaining method parameters from call arguments/context, check constraints as `TK2344`, and
  preserve overload trial isolation. Enable only the call/inference portion of the corpus once the
  old behavior is fully replaced.
- **Acceptance / witness.** Valid explicit/inferred calls return the substituted result; invalid
  arguments and constraints report the pinned `TK2345`/`TK2344`/overload identity; generic methods on
  generic outer types retain both binder levels; repeated and reordered calls yield identical
  diagnostics. No fallback to `any`, error-typed false clean, or lost incomplete record.
- **Touch points.** `src/check/checker/calls.rs`, `src/check/infer/`, persistent signature helpers,
  `tests/cases/b41_generic_methods/`, focused call/inference tests.

### WU3 — generic-signature relation, overloads, and inheritance (effort L)

- **Problem.** Calls alone are insufficient: generic methods participate in property assignability,
  interface/class compatibility, inheritance, and overload implementation checks. Comparing their
  unique parameter ids literally over-reports alpha-equivalent signatures; comparing under hidden
  binder state risks relation-cache poisoning.
- **Verify first.** Start with WU0's alpha-equivalent, incompatible-constraint, inheritance, and
  cache-order pairs. Run each order in fresh relation engines and against tsc. Inspect every proposed
  binder-alignment path for whether its verdict depends on an in-flight assumption or contextual
  mapping absent from the durable cache key.
- **Scope.** Implement ADR-0005's binder-aware generic-signature relation and constraint comparison;
  integrate it with object properties, class/interface conformance, overload candidate relation, and
  implementation compatibility. Preserve source contravariance/target covariance and
  `Relation::No(ReasonChain)`. Align generic overload parameters without mutating shared declarations
  or caching provisional/context-dependent results. Enable the full WU0 corpus.
- **Acceptance / witness.** Alpha-equivalent generic signatures relate; incompatible binder count,
  constraints, parameters, or returns reject with stable reasons; inherited/overloaded generic
  methods match tsc on the pinned corpus; order-reversed/cache-reuse probes produce identical
  diagnostics. Relation/type-store invariant tests remain green.
- **Touch points.** `src/relate/relation/`, generic signature representation/helpers,
  class/interface/overload compatibility paths, relation tests, WU0 fixtures.

### WU4 — independent adversarial soundness review (effort L)

- **Problem.** Nested binders, substitution, overload trials, and recursive relation form a high-risk
  cluster for dropped diagnostics and durable-cache poisoning.
- **Verify first.** A Terra reviewer independent of WU1-WU3 starts from the committed WU0 corpus and
  ADR, reads the implementation diff without relying on its rationale, and cross-checks fresh probes
  against `tsc 6.0.3 --strict`.
- **Scope.** Hunt outer/method binder capture, same-name shadowing, generic members after class
  substitution, defaults referencing earlier binders, failed-constraint fallback, explicit arity,
  contextual inference, rest parameters, generic overload trial leakage, static/instance separation,
  inheritance/override variance, construct/call signatures, alpha-equivalence, recursive methods,
  and cache/order dependence. Audit hashing/equality/substitution field parity. Any FAIL receives a
  focused fixture and returns to the implementation agent; the same reviewer rechecks remediation.
- **Acceptance / witness.** Explicit PASS with probes and commands; zero false clean, binder leak,
  permissive fallback, identity/hash mismatch, context-unsound cached verdict, or unexplained tsc
  divergence. No forbidden cast, `any`, suppression, `unsafe`, or new architecture outside ADR-0005.
- **Touch points.** Read-only WU0-WU3 diff, full generic/function/class corpora, fresh scratch probes;
  focused fixtures only for confirmed failures.

### WU5 — lib-admission audit, scoreboard ratchet, and closure (effort M)

- **Problem.** Closing `41` requires proving the construct family is modeled everywhere, not merely
  that selected calls work. Public/reference docs and the executable roadmap must keep `43`/`70`/`14`
  honest.
- **Verify first.** Re-run the pinned `lib.es5.d.ts` generic-method probes and lower representative
  declaration shapes through the checker without loading the full lib. Run the official suite before
  any save and audit all movement by diagnostic/incomplete identity. Re-screen the WU0 preview
  candidates only as information; do not relax or close `72` unless its original thresholds pass.
- **Scope.** Remove the generic-method and static-method deferrals, delete backlogs `41` and `23`,
  mark their completion-manifest owner complete, update architecture/scope/divergences and public
  limitations, and save only audited scoreboard progress. Record explicitly that array member
  projection, namespaces/merging, `this` parameters, and full lib loading remain `14`/`43`/`70`.
  Stamp the outcome, archive this sprint, and refresh docs indexes.
- **Acceptance / witness.** `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D
  warnings`; `cargo build --release`; official-suite unit tests; fresh official-suite fetch and
  `run --check` all pass. The generic-method audit has no silently dropped method/call/construct
  shape, and every scoreboard move is owned and identity-audited.
- **Touch points.** `README.md`, `docs/reference/{architecture,scope,divergences}.md`,
  `docs/backlog/{README,completion-1.0.toml,lib-audit-6.0.3.md}`, backlog `41`/`23`,
  `tooling/official-suite/scoreboard.txt`, `docs/INDEX.md`, `docs/sprints/README.md`, archive.

## Out of scope (explicit)

- Full `lib.d.ts`, array/primitive instance-member projection, or any new bounded-prelude member —
  backlog [`14`](../backlog/14-libdts-loading.md).
- Namespaces, qualified names, and declaration merging — backlog
  [`43`](../backlog/43-namespaces-declaration-merging.md).
- Explicit `this` parameters, receiver compatibility, and contextual `ThisType<T>` — backlog
  [`70`](../backlog/70-this-parameter-typing.md).
- Optional methods/possibly-undefined calls — backlog
  [`49`](../backlog/49-possibly-undefined-family.md).
- Packages, directory/re-export breadth, or other module resolution — backlog
  [`15`](../backlog/15-modules-imports.md).
- Relaxing or closing the real-project preview gate. `72` resumes only when a public witness meets
  its original zero thresholds without project-specific declarations.
- A whole-model migration of declaration parameters to de Bruijn indices, a second inference engine,
  permissive generic instantiation, or a relation-cache key that omits decision-relevant context.

## Decisions

- Run `41` now as the first Track A critical-path feature, not as a claim that it alone unblocks
  `72`. It has the highest `lib.es5.d.ts` dependency leverage; `43` and `70` remain independent later
  sprints before `14`.
- Do not build an Array/prelude bridge for the rejected preview candidate. That would cross the
  shipped minimal-prelude architecture boundary and still leave resolver/ambient gaps.
- Persistent generic binders are an architecture gate, not an implementation detail. WU0 must write
  ADR-0005 and prove substitution/relation/cache behavior before WU1 changes the type model.
- One sprint owns all generic method/call/construct signature sites because partial support would
  leave a silently permissive model family. Work stays split into independently reviewable commits.
- Keep the mandatory spec → implementation → independent-review loop. The leader commits each green
  unit; implementation and adversarial review use different Terra agents.
- Recommendation confidence is high for the Track A ordering and deliberately low as a direct
  preview unlock. This choice is wrong if WU0 proves a persistent representation cannot fit the
  current named-parameter/cache invariants without a model-wide migration; in that case stop `41`
  and re-rank `70`/`43` rather than forcing it.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Disabled corpus + ADR committed independently; no behavior change. |
| 2 | WU1 | Terra implementation agent; representation/lowering gates green and leader-inspected. |
| 3 | WU2 | Same implementation agent; call/inference fixtures pass before relation work. |
| 4 | WU3 | Same implementation agent; full corpus + cache-order controls pass. |
| 5 | WU4 | Different Terra reviewer; every FAIL remediated and independently re-reviewed. |
| 6 | WU5 | Full gates, lib-shape/scoreboard audit, backlog closure, then archive. |

WU0 fixture design and read-only representation analysis may run in parallel, but the ADR decision
and spec commit are one gate. WU1-WU3 are strictly ordered because persistent identity is the
substrate for inference and relation. WU4 starts only after all implementation is available.

Exact full gate: `cargo fmt --check`; `cargo test`; `cargo clippy --all-targets -- -D warnings`;
`cargo build --release`; official-suite unit tests; fresh official-suite fetch and `run --check`.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-12 — Full mandate received after `72` stopped at its explicit WU0 witness gate. Three
  independent Terra analyses agreed `41` is the highest-leverage next Track A feature and does not
  by itself make the rejected candidates preview-clean. Adversarial analysis identified the
  substitution-lost side-map binder as the design gate captured in WU0.
- 2026-07-12 — WU0 corpus and persistent-binder decision prepared; each fixture was checked against
  `tsc 6.0.3 --strict`. → ADR-0005
- 2026-07-12 — Re-audited every WU0 marker after materializing the defaulted generic return before
  its negative assignment: 31 inline markers, 31 matching `tsc 6.0.3 --strict` primary diagnostics.
- 2026-07-12 — WU1 persistent generic binders are now identity-bearing function fields. Outer
  substitution preserves inner binders while rewriting their constraints/defaults; generic class,
  object/interface, call, and construct surfaces lower under nested frames. The WU0 corpus remains
  disabled pending WU2/WU3 call and relation work.
- 2026-07-12 — Independent WU1 review: FAIL. Added disabled regressions for declaration-default
  constraints, outer-substituted method constraints/defaults, and generic function/constructor
  type annotations; remediation returns to WU1/WU2 before the corpus can be enabled.
- 2026-07-12 — WU1 review remediation: persistent call/construct candidates now consume
  descriptor constraints/defaults after outer substitution; signature defaults validate in order,
  generic type annotations lower, and explicit arity reports `TK2558`. All focused probes match;
  `b41` remains disabled pending binder-aware relation/overload work in WU3.
- 2026-07-12 — Re-review: FAIL. A forward generic default (`<T = U, U = string>`) remains
  false-clean instead of `TK2744`; the disabled declaration-default regression returns to WU1.
- 2026-07-12 — Forward-default remediation: signature default lowering now rejects later binder
  references as `TK2744` and discards the invalid default before persistent call instantiation.
- 2026-07-12 — WU3: relation now alpha-aligns persistent generic binders, compares aligned
  constraints, specializes generic sources only, and bypasses the durable cache below local
  binder contexts while retaining the recursive assume-true stack. Generic overload
  implementations and class overload checks use that relation directly; self-generic class
  method returns stay lazy through class fill, and focused call-site arrow contextual typing
  restores substituted callback inference. `b41` is enabled: every fixture matches `tsc 6.0.3
  --strict`; full Rust tests and clippy pass.
- 2026-07-12 — WU4 review: FAIL. Added disabled B41 regressions for illegal static references to
  class type parameters and tsc-clean overload implementations rejected as `TK2394`; remediation
  returns to implementation work after the spec commit.
- 2026-07-12 — WU4 remediation: a scoped static-class-binder barrier reports `TK2302` for class
  binders in static field and method signatures and static local annotations, while static-owned
  binders and outer names remain usable. Generic overload implementation checks now admit the
  tsc-clean fixed/generic and constrained-return shapes without admitting the invalid arity
  controls. The recursive assume-true stack is qualified by its active binder contexts, with a
  focused nested-context regression. `b41` is re-enabled; `cargo fmt --check`, `cargo test`, and
  `cargo clippy --all-targets -- -D warnings` pass.
- 2026-07-12 — WU4 P0 re-review: FAIL. Disabled recursive-binder corpus adds direct, structural,
  and nested-generic-callback terminating relation pairs in both query orders; TypeScript 6.0.3
  accepts the six alpha-equivalent assignments and rejects four number/string specialization
  controls as `TS2322`. The current checker false-cleans all four controls, so `b41` returns to
  disabled pending remediation.
- 2026-07-12 — WU4 P0 remediation: in-flight cycle keys now use a canonical flattened binder
  environment (alignment, explicit optional constraints, and source specializations), not frame
  allocation identity. Equivalent recursive frames collapse; distinct specializations and reverse
  alignments do not. Direct relation tests cover all three P0 terminating shapes, both query
  orders, and the specialization separation witness. The timeout-gated fixture now emits exactly
  four `TK2322` controls; `b41` is re-enabled.
