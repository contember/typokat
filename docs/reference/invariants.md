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
- **Narrowing** (`src/check/flow.rs` ops + the checker's narrowing env): keyed on `SymbolId`, never
  escapes its branch, unknown guard narrows nothing, resets on (any) assignment and at function
  boundaries. All current narrowing is **structured** (`if`/`else`/`switch`) — see the backlog for
  the unstructured-flow CFG.
- **No forbidden hacks**: no `unsafe`, no reachable `unwrap`/`panic`/`todo!`, no `as` casts except
  numeric arena-index/discriminant conversions, no `#![allow(warnings)]`. `clippy -D warnings` is
  clean — keep it that way.
- **Soundness > completeness**: when in doubt, **over-report** (a false positive in the safe
  direction). Document any deliberate `tsc` divergence in `tests/cases/README.md`.

## 2. Deliberate deferrals already taken (don't be surprised; plan for them)

- **Type parameters are NAMED unique ids, not de Bruijn** (architecture §3.1 wants de Bruijn). This
  is the one foundational deferral that the **conditional-types milestone forces** (`09`): `infer` +
  alpha-equivalent hash-consing want de Bruijn. Plan the migration when you start conditional types
  (it is localized to the type-param repr + `substitute`). (Forced by the evaluation work, not by a
  VM — the bytecode VM itself is deferred, [ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md).)
- **Stable structural hash (blake3) is reserved but uncomputed** — needed for incrementality (Phase 4)
  and for parallelism Stage 2 (cross-file export identity, architecture §8.2). The interner is already
  shaped for it (`src/types/hash.rs`).
- **Per-argument call errors over-report** vs `tsc` (which stops at the first bad arg) — safe
  direction, documented.
