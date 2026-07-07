# Invariants — the soundness/architecture core (binding)

These are the rules you **must not break**. They were hard-won; regressing them silently drops errors
— the worst outcome for a checker. This file is **binding** (top of the source-of-truth precedence in
[`../CLAUDE.md`](../CLAUDE.md)). The design behind them is in [`architecture.md`](./architecture.md);
the build method that protects them is in [`dev-method.md`](./dev-method.md).

## 1. Invariants you must NOT break

- **Relation engine** (`src/relate/relation.rs`): the 3×`u32` `(TypeId,TypeId,RelationKind)` bool
  cache, the **assume-true-until-disproven cycle stack** for recursive types, and `Relation::No(ReasonChain)`
  (never a bare `bool`). **Cache-soundness fix (architecture §6.3):** a verdict that depended on an
  in-flight *ancestor* assumption must NOT be committed to the durable cache (only a `false`, or a
  non-provisional `true`). A regression here causes order-dependent dropped errors — the sharpest
  bug found in the whole project.
- **Type store** (`src/types/`): every type is a hash-consed `TypeId`; structural equality is `==`.
  Property metadata that is part of *identity* — `visibility`, `declaring_class`, `readonly`,
  `is_accessor`, index signatures — is folded into the structural hash + `object_props_eq`, but the
  **relation engine ignores `readonly`/`is_accessor`** for assignability (they only gate access /
  assignment targets). `substitute` must carry **all** `PropertyType` fields through.
- **Nominal classes**: a `private`/`protected` member makes a type nominal — the relation requires
  same-name + same `declaring_class` + same visibility. Generic class *instances* are structural.
- **Narrowing** (`src/check/flow.rs` ops + the **flow-node CFG** in `src/check/checker/flowgraph.rs`):
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
  direction). Document any deliberate `tsc` divergence in `tests/cases/README.md`.

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
