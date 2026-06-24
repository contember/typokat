# typokat

A from-scratch **TypeScript type checker in Rust**, built to a state-of-the-art architecture.
`typokat check <file.ts>` parses, binds, and type-checks TypeScript and reports `tsc`-style
diagnostics — structural **and** nominal typing, control-flow narrowing, generics with inference,
full classes, and the common "real-world" type constructs. It is a **checker, not a compiler**:
emit, JS runtime semantics, and module resolution are out of scope by design — the goal is to
preserve the **type model** (see [`ts-checker-architecture.md`](./ts-checker-architecture.md)).

> Status: **M0–M20** implemented. ~16k lines of Rust, 169 unit tests + a 55-file conformance
> corpus (132 expected diagnostics), `clippy -D warnings` clean. Every milestone was
> cross-checked against real `tsc 6.0.3 --strict`.

## Quick start

```sh
cargo run -- check path/to/file.ts
```

Exit code is `0` when clean, `1` when diagnostics are reported. Example output:

```
error[TK2322]: Type '{ a: { b: string } }' is not assignable to type '{ a: { b: number } }'
   ┌─ demo.ts:28:38
   │
28 │ const nested: { a: { b: number } } = { a: { b: "oops" } };
   │                                      ^^^^^^^^^^^^^^^^^^^^
   │
   =   Types of property 'a' are incompatible.
         Types of property 'b' are incompatible.
           Type 'string' is not assignable to type 'number'.
```

## What it checks

| Area | Coverage |
|---|---|
| **Foundation** | primitives & intrinsics (`any`/`unknown`/`never`/`void`, strict null), objects (structural, excess/missing/depth), functions (arity, contravariant params, void-return rule), unions (canonicalized), recursive & mutually-recursive named types, literal types |
| **Narrowing** | `typeof`, truthiness, `null`/`undefined` equality, **discriminated unions**, `in`, `switch` (flow-sensitive, scoped) |
| **Generics** | type parameters, instantiation, **type-argument inference** from call arguments |
| **Classes** | fields, constructor, methods, `this`, `new`, structural instances; inheritance (`extends`/`super`); access modifiers (`private`/`protected` — access control **+ nominal typing**); `static`; member-assignment checking; `readonly`; getters/setters; `abstract`; **generic classes** |
| **Real-world types** | arrays (`T[]`/`Array<T>`, element access, covariance), tuples (positional, contextual typing), index signatures (`{ [k: string]: T }`), `keyof T`, indexed-access types (`T[K]`) |
| **Reporting** | nested reason chains (`Types of property 'x' are incompatible …`) |

### Diagnostics

`tsc`-compatible numeric codes with a `TK` prefix:
`TK2304` (cannot find name), `TK2322` (not assignable), `TK2339` (no such property),
`TK2341`/`TK2445` (private/protected), `TK2345` (argument), `TK2353` (excess property),
`TK2511` (instantiate abstract), `TK2540` (assign read-only), `TK2554` (arity),
`TK2741` (missing property).

## Architecture

The full design is in [`ts-checker-architecture.md`](./ts-checker-architecture.md); the build plan
in [`mvp-plan.md`](./mvp-plan.md). The pieces that make it Rust-shaped and fast:

- **Type store** — every type is a `TypeId(u32)` into an arena (no `Rc<RefCell>`), **hash-consed**
  so structural equality is an integer compare, SoA cold side-tables, substitution-aware.
- **Relation engine** (`is_assignable`) — the biggest CPU piece — with a 3×`u32` cache, an
  **assume-true-until-disproven cycle stack** for recursive types, and **reason chains** (not a
  bare `bool`) so reporting runs through the same path. Carries a soundness fix for cache poisoning
  by provisional cycle assumptions (architecture §6.3).
- **Binder** — a scope graph with **multi-slot symbols** (value / type / namespace spaces), which is
  what lets a class be both a type (instance) and a value (constructor), and what nominal classes
  key on.
- **Statement checker** — a flow-sensitive interpreter: a narrowing environment that forks at
  `if`/`else`/`switch` and restores after, plus the generic **inference engine** (a separate,
  generative machine from the relation engine).

## How it was built

Milestone by milestone (M0 = a literal-to-primitive "walking skeleton" → M20), each as a **vertical
slice** that runs end-to-end. The conformance corpus in [`tests/cases/`](./tests/cases/) is the
spec: each `.ts` fixture carries inline `// error[TK…]` markers, and the harness diffs the checker's
diagnostics against them (see [`tests/cases/README.md`](./tests/cases/README.md)). Process:

1. write the fixture corpus for a milestone (the acceptance spec),
2. implement to the spec,
3. **independent adversarial review** (hunting false negatives, cross-checked vs `tsc`),
4. fix, then commit.

That independent review caught real soundness/correctness bugs the implementation missed —
relation-cache poisoning (order-dependent dropped errors), unchecked `static` bodies, a get-only
accessor writable in its constructor, literal-tuple over-strictness, nested excess in index-sig
values — none of which the implementation's own tests surfaced.

## Project layout

```
src/
  driver.rs, main.rs, span.rs, diagnostics.rs   pipeline, CLI, spans, diagnostics + rendering
  types/    store · intern (hash-consing) · repr · hash · substitute   the type store
  binder/   scope · symbol (multi-slot) · bind                          scope graph
  check/    checker · infer (inference engine) · flow (narrowing ops)   the checkers
  relate/   relation (is_assignable, cycle stack, reasons) · cache      the relation engine
tests/
  conformance.rs        marker-driven harness (MILESTONE_DIRS enables m0..m20)
  cases/mN_*/           the conformance corpus (the spec)
```

## Known limitations (deferred, documented)

By design `typokat` keeps types and drops emit/runtime; beyond that, these are conscious deferrals:

- **Narrowing through unstructured flow** — `if (x.kind === "a") return; …` does **not** narrow the
  code after the early `return` (only structured `if`/`else`/`switch` narrows). This needs the
  flow-node CFG and is the most likely to surprise on idiomatic code.
- **The type-level VM phase** — conditional types (`T extends U ? X : Y`), mapped types, and utility
  types (`Partial`, `Record`, …) are not implemented; they want the bytecode VM (architecture §7).
  Generic `keyof`/`T[K]` over a bare type parameter likewise defers to that phase.
- **Optional properties** (`a?: T`) are currently dropped at lowering (a safe-direction
  under-approximation).
- **No `lib.d.ts`** (so `console`, array methods, `Promise`, … are absent), **no modules/imports**,
  and an **unresolved type name** is silently the error type (no `TK2304` in type position yet).
- Minor `tsc` divergences, all in the safe (over-report) direction, are logged in
  [`tests/cases/README.md`](./tests/cases/README.md).

## Testing

```sh
cargo test                              # 169 unit tests + the conformance corpus
cargo clippy --all-targets -- -D warnings
```
