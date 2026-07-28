# Invariants — the soundness/architecture core (binding)

These are the rules you **must not break**. They were hard-won; regressing them silently drops errors
— the worst outcome for a checker. This file is **binding** (top of the source-of-truth precedence in
[`../CLAUDE.md`](../CLAUDE.md)). The design behind them is in [`architecture.md`](./architecture.md);
the build method that protects them is in [`dev-method.md`](./dev-method.md).

## 1. Invariants you must NOT break

- **Semantic query and relation boundary** (`crates/typokat-check/src/check/query/`, `src/relate/relation/mod.rs`):
  production assignability enters only through `SemanticQueryCoordinator`; raw `Relater`
  construction/`is_assignable` is test-only. Each query uses fresh projection/evaluation overlays,
  and `Relater` applies present overlay mappings **before** identity, the 3×`u32`
  `(TypeId,TypeId,RelationKind)` cache, or the **assume-true-until-disproven cycle stack**.
  An absent mapping leaves a deferred node under its conservative identical-only rule; it must not
  be demand-evaluated merely to precede identity. Publication/poison preflight runs before same-id
  success for assignability and overload compatibility.
  Relation returns `Yes | No(ReasonChain) | Exhausted`, never a bare `bool`; demand returns
  `Ready | Exhausted`. Whether a `No` carries its explanation is conditional on `want_reason`
  (architecture §6.4, [ADR-0016](../decisions/0016-reason-free-relation-probes.md)): reason-free
  mode is granted only where the caller already discards its child's reason, and there a cached
  `false` answers from the memo with a shared leaf. This is **stricter, not neutral** — the
  re-derivation it replaces ran under a stack push and could return the assume-true `Yes` of §6.3
  against the key's own cached `false`, so a reason-free frame both answers `No` where the old
  engine could answer `Yes` and creates none of the cache entries a recompute would. It is safe
  because no rule consuming a relation result is antitone, so it can only add or preserve
  diagnostics. A reason-carrying recompute of a cached `false` is **clamped** to that `false`, so
  the two modes never disagree and the engine never contradicts its own cache. Helpers **inherit**
  `self.want_reason`; opting out is per-call-site and must be justified by the caller discarding the
  reason, never by convenience. A verdict that depended on an in-flight *ancestor* assumption is
  provisional and must not be committed. Poison, an exhausted planner/evaluator budget or cycle, or any other
  taint promotes **none** of the pending evaluator, projection, or relation-cache writes. A
  regression here causes order-dependent dropped errors — the sharpest bug class in the project.
- **Type store** (`src/types/`): every type is a hash-consed `TypeId`; structural equality is `==`.
  Property metadata that is part of *identity* — `visibility`, `declaring_class`, `readonly`,
  `is_accessor`, index signatures — is folded into the structural hash + `object_props_eq`, but the
  **relation engine ignores `readonly`/`is_accessor`** for assignability (they only gate access /
  assignment targets). `substitute` must carry **all** `PropertyType` fields through.
- **Immutable class publication and lexical effects** (`src/class_semantics.rs`,
  `crates/typokat-check/src/check/checker/classes/`, `crates/typokat-check/src/check/checker/events.rs`): every class application is an
  immutable `ClassInstance` with a syntactically valid, semantically available, complete ordered
  vector of real argument `TypeId`s; never fabricate error/`unknown`/`never` or a partial vector.
  Class declaration SCCs publish complete instance/constructor/static/binder/callable surfaces
  atomically behind the construction capability; heritage, initializer, and surface poison expose
  typed exhaustion before identity/cache/member/`new`/call/inference, never a partial surface.
  Retained callable rows—including binders, constraints/defaults, receiver, parameters, return,
  overload state, and parameter-property ownership—are lowered once and reused by body checking.
  Every diagnostic/incomplete record has one checker-wide `EventStore` owner and replays by
  `(original_module_ordinal, source_start, event_ordinal, record_ordinal)`, independent of build,
  completion, query, and cache order. Candidate effects remain local until exactly one selected set
  commits; there is no diagnostic deduplication, span deletion, truncation, or post-hoc suppression.
  An argument walked twice — once raw for candidate selection, once under the instantiated
  contextual type — is that same rule, not an exception to it: the raw batch is *held* and released
  only if the contextual re-walk supersedes it, and every batch not superseded commits when the
  call's frame closes. The line is *when*, not *whether* — holding a batch before it reaches an
  owner is candidate locality; removing records after they have reached one is the suppression this
  forbids.
- **Private iterative evaluator walkers** (`crates/typokat-check/src/check/checker/eval/`): `InferRewrite` and
  `MappedRewrite` use explicit heap task/value stacks for
  every child they traverse, including function type-parameter constraints/defaults. Keep their
  policies distinct: infer rewriting preserves per-run fresh-binder scope, completed-memo rules,
  and SCC-suffix identity taint; mapped-value replacement keeps a
  per-property local memo/in-progress set where re-entry returns the original `TypeId` and a
  completed ancestor may memoize a partial clone. It has no SCC taint, evaluator memo write, or
  budget. Inference constraints cross the semantic query coordinator and preserve typed
  exhaustion instead of using a private recovery evaluator. A shared generic walker must not
  obscure these cleanup and identity boundaries. This
  names only the hardened walkers: the bounded `keyof`-intersection and template-union helper
  recursion remains safe because interner flattening removes same-family nesting before traversal.
- **Nominal classes**: a `private`/`protected` member makes a type nominal — the relation requires
  same-name + same `declaring_class` + same visibility. Generic class *instances* are structural.
- **Type-parameter constraints**: circular chains through bare type parameters, unions, or
  intersections must report `TK2313` and clear the recorded constraint before any relation query can
  treat the cycle as assignable. Structural indirection such as `{ self: T }` is legal recursion and
  must terminate through the relation engine instead.
- **Narrowing** (`crates/typokat-check/src/check/flow.rs` ops + the **flow-node CFG** in `crates/typokat-check/src/check/checker/flowgraph.rs`):
  keyed on `SymbolId`, never escapes its branch, unknown guard narrows nothing, resets at function
  boundaries. The flow-node CFG (M23) is the **single** narrowing model — `if`/`else`/`switch` *and*
  unstructured flow (early `return`/`throw`, `&&`/`||`/ternary, `while` back/exit/`break`/`continue`
  edges) all resolve through the same backward walk. An assignment narrows the variable to the
  **assigned value's type** in the straight-line flow that follows (a compound/complex RHS resets to
  the declared type — over-report, sound); joins widen (union of branch states). **The sharpest CFG
  trap:** resolving through a **loop label** must never durably cache a pre-loop narrow state across a
  not-yet-finalized back edge — that is a dropped-error false negative (same shape as the relation
  cache's provisional-cycle bug; the fixpoint seeds the declared type and never memoizes the
  provisional seed).
- **No forbidden hacks**: no `unsafe`, no reachable `unwrap`/`panic`/`todo!`, no `as` casts except
  numeric arena-index/discriminant conversions, no `#![allow(warnings)]`. `clippy -D warnings` is
  clean — keep it that way.
- **Soundness > completeness**: when in doubt, **over-report** (a false positive in the safe
  direction). Document any deliberate `tsc` divergence in [`divergences.md`](divergences.md).

## 2. Deliberate deferrals already taken (don't be surprised; plan for them)

- **Type parameters are NAMED unique ids; `infer` binders are per-node de Bruijn** — resolved by
  [ADR-0002](../decisions/0002-de-bruijn-scoped-to-infer-binders.md) when conditional types landed
  (M25): de Bruijn indices apply to `infer` binders within conditional nodes only; declaration type
  params stay named unique ids (context-free open types keep the relation cache sound).
  Alpha-equivalent hash-consing of generic *declarations* remains a measured, deferred optimization.
  M24's **type-parameter constraint column** on the `Store` (keyed by `TypeParamId`; a side column,
  NOT part of the interned type's identity) is unaffected by this split.
- **Stable structural hash (blake3) is reserved but uncomputed** — needed for incrementality (Phase 5)
  and for parallelism Stage 2 (cross-file export identity, architecture §8.2). The interner is already
  shaped for it (`src/types/hash.rs`).
- **Per-argument call errors over-report** vs `tsc` (which stops at the first bad arg) — safe
  direction, documented.
