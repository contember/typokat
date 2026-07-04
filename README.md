# typokat

A from-scratch **TypeScript type checker in Rust**, built to a state-of-the-art architecture.
`typokat check <file.ts>` parses, binds, and type-checks TypeScript and reports `tsc`-style
diagnostics — structural **and** nominal typing, control-flow narrowing, generics with inference,
full classes, and the common "real-world" type constructs. It is a **checker, not a compiler**:
emit, JS runtime semantics, and module resolution are out of scope by design — the goal is to
preserve the **type model** (see [`docs/reference/architecture.md`](./docs/reference/architecture.md)).

> Status: **M0–M24** implemented (M24 = generic constraints: `TK2344` on explicit type
> arguments, the constraint as the parameter's apparent type across every structural
> consumer, clamp-to-constraint inference, `TK2313` circular constraints), on top of the
> M23 flow-node CFG, class completeness, and constructor accessibility. ~19.5k lines of
> Rust, 172 unit tests + a 101-file conformance corpus (293 expected diagnostics),
> `clippy -D warnings` clean. Every milestone was cross-checked against real
> `tsc 6.0.3 --strict`.

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
| **Foundation** | primitives & intrinsics (`any`/`unknown`/`never`/`void`, strict null), objects (structural, excess/missing/depth, **optional members `a?: T`**), functions (arity, contravariant params, void-return rule), unions (canonicalized), recursive & mutually-recursive named types, literal types |
| **Narrowing** | `typeof`, truthiness, `null`/`undefined` equality, **discriminated unions**, `in`, `switch`; **unstructured flow** via the flow-node CFG — early `return`/`throw`, `&&`/`\|\|`/ternary, assignment narrowing, `while` loop edges (back edge / exit / `break` / `continue`) |
| **Generics** | type parameters, instantiation, **type-argument inference** from call arguments |
| **Classes** | fields, constructor, methods, `this`, `new`, structural instances; inheritance (`extends`/`super`); access modifiers (`private`/`protected` — access control **+ nominal typing**); `static`; member-assignment checking; `readonly`; getters/setters; `abstract` (incl. **abstract-member completeness**); **generic classes**; **override compatibility** (tsc's base-keyed method bivariance); **constructor accessibility** on `new` |
| **Real-world types** | arrays (`T[]`/`Array<T>`, element access, covariance), tuples (positional, contextual typing), index signatures (`{ [k: string]: T }`), `keyof T`, indexed-access types (`T[K]`) |
| **Reporting** | nested reason chains (`Types of property 'x' are incompatible …`) |

### Diagnostics

`tsc`-compatible numeric codes with a `TK` prefix:
`TK2304` (cannot find name), `TK2313` (circular constraint), `TK2322` (not assignable),
`TK2339` (no such property), `TK2341`/`TK2445` (private/protected), `TK2344` (constraint
not satisfied), `TK2345` (argument), `TK2353` (excess property), `TK2416` (incompatible
override), `TK2511` (instantiate abstract), `TK2515`/`TK2654` (abstract member not
implemented), `TK2540` (assign read-only), `TK2554` (arity), `TK2673`/`TK2674`
(private/protected constructor), `TK2741` (missing property).

## Architecture

The full design is in [`docs/reference/architecture.md`](./docs/reference/architecture.md); the build
plan in [`docs/archive/mvp-plan.md`](./docs/archive/mvp-plan.md). The pieces that make it Rust-shaped
and fast:

- **Type store** — every type is a `TypeId(u32)` into an arena (no `Rc<RefCell>`), **hash-consed**
  so structural equality is an integer compare, SoA cold side-tables, substitution-aware.
- **Relation engine** (`is_assignable`) — the biggest CPU piece — with a 3×`u32` cache, an
  **assume-true-until-disproven cycle stack** for recursive types, and **reason chains** (not a
  bare `bool`) so reporting runs through the same path. Carries a soundness fix for cache poisoning
  by provisional cycle assumptions (architecture §6.3).
- **Binder** — a scope graph with **multi-slot symbols** (value / type / namespace spaces), which is
  what lets a class be both a type (instance) and a value (constructor), and what nominal classes
  key on.
- **Statement checker** — a flow-sensitive interpreter over a **flow-node CFG** (M23): a pre-pass
  builds each body's flow graph (conditions, assignments, joins, loop labels, unreachable), and
  references resolve by a memoized backward walk — with the same provisional-state cache
  discipline as the relation engine. Plus the generic **inference engine** (a separate,
  generative machine from the relation engine).

## How it was built

Milestone by milestone (M0 = a literal-to-primitive "walking skeleton" → M22), each as a **vertical
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
  check/    checker (incl. flowgraph) · infer (inference engine) · flow (nodes + narrowing ops)   the checkers
  relate/   relation (is_assignable, cycle stack, reasons) · cache      the relation engine
tests/
  conformance.rs        marker-driven harness (MILESTONE_DIRS enables m0..m22 + bug-fix corpora)
  cases/mN_*/           the conformance corpus (the spec)
```

## Known limitations (deferred, documented)

By design `typokat` keeps types and drops emit/runtime; beyond that, these are conscious deferrals:

- **Narrowing deferrals past M23** — assertion functions / type predicates (`x is T`),
  `for`/`for-of`/`do-while` loop forms (fall back to declared types — safe), closure narrowing of
  never-reassigned bindings, and member-path narrowing (`x.a` — narrowing is symbol-keyed).
  Declaration initializers deliberately don't narrow.
- **The type-level evaluation phase** — conditional types (`T extends U ? X : Y`), mapped types, and
  utility types (`Partial`, `Record`, …) are not implemented; they will be tree-walked with the
  algorithmic wins folded in (architecture §7). Generic `keyof`/`T[K]` over a bare type parameter
  likewise defers to that phase. (A bytecode VM is a deferred, profiling-gated refactor, not a
  planned step — see `docs/decisions/0001-…`.)
- **Optional properties** (`a?: T`) on objects/interfaces/class fields are implemented (M21): a
  member may be absent, reads yield `T | undefined`, `keyof`/indexed-access include it. Still
  deferred: optional **methods**/accessors (`go?(): T`), the dedicated *possibly-undefined*
  diagnostics (tsc `TS2532`/`TS18048`/`TS2722`), and narrowing an optional through a member-access
  guard (over-reports `T | undefined`, the safe direction).
- **No `lib.d.ts`** (so `console`, array methods, `Promise`, … are absent) and **no modules/imports**.
  An **unresolved type name** in type position is `TK2304` (M22); still deferred there (distinct tsc
  codes): a value used as a type (`TS2749`), type args on a type parameter (`TS2315`), a wrong
  type-argument count such as bare `Array` (`TS2314`), and qualified names `A.B` (`TS2503`).
- Minor `tsc` divergences, all in the safe (over-report) direction, are logged in
  [`tests/cases/README.md`](./tests/cases/README.md).

## Testing

```sh
cargo test                              # 172 unit tests + the conformance corpus
cargo clippy --all-targets -- -D warnings
```
