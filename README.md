# typokat

**A TypeScript type checker written from scratch in Rust.**

No `tsc` fork, no transpiled JS — a fresh implementation of the TypeScript *type model* on a
hash-consed type arena and a purpose-built relation engine. Point it at a `.ts` file and it
parses, binds, and type-checks it, then reports `tsc`-style diagnostics: structural **and** nominal
typing, control-flow narrowing, generics with inference, full classes, conditional/mapped/template
types, and the everyday "real-world" constructs.

It is a **checker, not a compiler** — emit and JS runtime semantics are out of scope by design.
Current module coverage is deliberately narrow: local relative `.ts` modules plus a bounded
files-only Bundler project route. Physical resolution is delegated to `oxc_resolver`; general
packages and alternate host profiles remain deferred. The one goal is to preserve the **type
model** faithfully; when in doubt it *over-reports* (the safe direction). Full design:
[`docs/reference/architecture.md`](./docs/reference/architecture.md).

<p>
  <img alt="Rust" src="https://img.shields.io/badge/built%20with-Rust-000?logo=rust">
  <img alt="Milestones" src="https://img.shields.io/badge/milestones-M0--M33-2ea44f">
  <img alt="Tests" src="https://img.shields.io/badge/tests-702%20unit%20%2B%20403%20conformance-blue">
  <img alt="Clippy" src="https://img.shields.io/badge/clippy-clean-brightgreen">
</p>

By the numbers: **83,195 lines of Rust** (78,888 nonblank), **702 unit tests**
(696 passing plus 6 ignored release measurements), and **403 enabled conformance source files** with
1,910 expected diagnostic markers plus 234 explicit incomplete-surface markers. `clippy -D warnings` is
clean, and **every milestone is cross-checked against real `tsc 6.0.3 --strict`**.

## Quick start

```sh
cargo run -- check path/to/file.ts
cargo run -- check --project-summary json path/to/project
```

The project form accepts a directory containing `tsconfig.json` or the config path itself. Its
exact admitted config is `files` plus `strict: true`, `noEmit: true`, `module: "ESNext"`, and
`moduleResolution: "Bundler"`. Every configured root must be a local `.ts` file. Local named
imports may be extensionless or use `.js`→`.ts` substitution; `oxc_resolver 11.24.2` performs the
physical lookup. The JSON summary deterministically accounts for roots, checked/skipped files,
resolutions, project notices, parse errors, incomplete surfaces, and diagnostics. Any unsupported
config, specifier, or module form is an explicit non-clean project notice.

Exit code is `0` when the check is complete and clean, `1` when diagnostics are reported, `2` on
usage/IO errors, and `3` when the checker hit an **unsupported in-scope construct** — the file was
not fully checked, so a clean diagnostic list is not a clean verdict. Incomplete surfaces render as
`incomplete[<surface-id>]` records alongside any ordinary diagnostics (exit `3` takes precedence
over `1`; there is deliberately no flag to downgrade it). Real output:

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
| **Foundation** | primitives & intrinsics (`any`/`unknown`/`never`/`void`, strict null), objects (structural, excess/missing/depth, **optional members `a?: T`**), functions (arity, optional/default/rest params, ordered overload resolution, contravariant params, void-return rule), unions (canonicalized), **intersections (`A & B`)** (canonicalized, merged relation + member access + excess), recursive & mutually-recursive named types, literal types |
| **Narrowing** | `typeof`, truthiness, `null`/`undefined` equality, **discriminated unions**, `in`, `switch`; **unstructured flow** via the flow-node CFG — early `return`/`throw`, `&&`/`\|\|`/ternary, assignment narrowing, `while` loop edges (back edge / exit / `break` / `continue`) |
| **Generics** | type parameters, instantiation, **type-argument inference** from call arguments, **constraints** (`extends` — apparent types, declaration + call-site `TK2344`/`TK2345`, circularity `TK2313`), persistent generic free/member/call/construct signatures |
| **Type-level evaluation** | conditional types (**distribution**, `infer` incl. tuple/function rest capture and anchored template extraction, recursion guards `TK2456`/`TK2589`), mapped types (modifier arithmetic, homomorphic union distribution), template literal types (construction + anchored pattern matching), deferred `keyof`, the pinned TypeScript 6.0.3 ES2025 full-host default library, and the `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` intrinsics |
| **Classes** | fields, constructor, methods, `this`, `new`, structural instances; inheritance (`extends`/`super`); access modifiers (`private`/`protected` — access control **+ nominal typing**); `static`; member-assignment checking; `readonly`; getters/setters; `abstract` (incl. **abstract-member completeness**); **generic classes**; **override compatibility** (tsc's base-keyed method bivariance); **constructor accessibility** on `new`; immutable complete class applications, dependency-first SCC publication, poison propagation, and bounded demand-driven projection |
| **Real-world types** | arrays (`T[]`/`Array<T>`, element access, covariance), tuples (positional, rest elements, contextual typing), contextual fresh object/array/tuple literals, index signatures (`{ [k: string]: T }`), `keyof T`, indexed-access types (`T[K]`), type-side namespaces/reopenings/qualified lookup and declaration merging, local relative modules with named imports/exports and acyclic named source re-exports, and the bounded files-only Bundler project route |
| **Reporting** | nested reason chains (`Types of property 'x' are incompatible …`) |

### Diagnostics

`tsc`-compatible numeric codes with a `TK` prefix:
`TK2302` (static member references a class type parameter), `TK2304` (cannot find name),
`TK2305` (no exported member), `TK2307` (cannot find module), `TK2313` (circular constraint),
`TK2314` (wrong generic type-argument count), `TK2322` (not assignable),
`TK2339` (no such property), `TK2341`/`TK2445` (private/protected), `TK2344` (constraint
not satisfied), `TK2345` (argument), `TK2353` (excess property), `TK2391` (missing or
non-contiguous function implementation), `TK2394` (overload incompatible with implementation),
`TK2416` (incompatible override), `TK2456` (circular type alias), `TK2511` (instantiate abstract),
`TK2515`/`TK2654` (abstract member not implemented), `TK2540` (assign read-only),
`TK2554`/`TK2555` (arity), `TK2558` (wrong explicit type-argument count), `TK2589`
(instantiation excessively deep), `TK2673`/`TK2674` (private/protected constructor), `TK2684`
(invalid explicit receiver), `TK2693` (type used as value), `TK2707` (generic arity range),
`TK2741` (missing property), `TK2744`
(invalid type-parameter default reference), and `TK2769` (no overload matches).

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
- **Class publication + semantic queries** — complete immutable `ClassInstance` applications are
  published by declaration SCC behind a capability boundary. A sole `SemanticQueryCoordinator`
  owns demand-driven projection/evaluation and relation transactions with query-local overlays,
  typed `Ready`/`Yes`/`No`/`Exhausted` outcomes, poison propagation, and all-or-nothing durable
  memo/cache promotion.
- **Statement checker** — a flow-sensitive interpreter over a **flow-node CFG** (M23): a pre-pass
  builds each body's flow graph (conditions, assignments, joins, loop labels, unreachable), and
  references resolve by a memoized backward walk — with the same provisional-state cache
  discipline as the relation engine. Plus the generic **inference engine** (a separate,
  generative machine from the relation engine).

## How it was built

Milestone by milestone (M0 = a literal-to-primitive "walking skeleton" → M33), each as a **vertical
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
  lib.rs, main.rs                               root facade and CLI
crates/
  typokat-core/                                 spans and shared source identities
  typokat-types/                                type store, interning, representation, substitution
  typokat-binder/                               scope graph and multi-slot symbols
  typokat-relate/                               assignability, cycle stack, reasons, and cache
  typokat-diagnostics/                          diagnostics and rendering
  typokat-surface/                              exhaustive Oxc AST surface classification
  typokat-frontend/                             parser and project dependency ordering
  typokat-check/
    src/check/                                  checker, inference, flow, and test support
    src/check/test_support_prelude.ts           test-only bounded ambient unit
  typokat-library/
    src/typescript-6.0.3/                       pinned default-library sources and notices
    src/{base,compiler,profile,provider}.rs      frozen library base and source compiler
  typokat-driver/src/driver.rs                  frontend/check orchestration and reports
tests/
  conformance.rs        marker-driven harness (MILESTONE_DIRS enables m0..m33 + bug-fix corpora)
  cases/mN_*/           the conformance corpus (the spec)
```

## Known limitations (deferred, documented)

By design `typokat` keeps types and drops emit/runtime; beyond that, these are conscious deferrals:

- **Narrowing deferrals past M23** — assertion functions / type predicates (`x is T`); closure
  narrowing of never-reassigned bindings; and member-path narrowing (`x.a` — narrowing is
  symbol-keyed). `for`/`for-in`/`for-of`/`do` bodies **are** structurally checked (against
  declared types) since the 2026-07-10 soundness pass; only flow-*narrowing* inside those loop
  forms still falls back to declared types (safe). Declaration initializers deliberately don't
  narrow. (Backlog `50`/`51`.)
- **Type-level evaluation edges** — the phase itself shipped (M24–M28); a conservative
  (over-report) tail remains in the backlog: cross-binder nested `infer` stays a poisoned
  deferral, conditionals buried in named alias/interface/class bodies stay deferred, and
  `keyof` over unions/`never`/template-literal key sources plus two tsc-parity conditional
  edges are documented divergences (backlog `26`, `27`, `35`–`37`). (A bytecode VM is a
  deferred, profiling-gated refactor — see `docs/decisions/0001-…`.)
- **Remaining type-model gaps** — enums and `satisfies`/`as const` are not modeled yet. Type-side
  namespaces, reopenings, qualified lookup, declaration merging, standalone instantiated namespace
  values, and local `Array` heritage are modeled. Generic methods, explicit `this`
  parameters, contextual `ThisType<T>`, and object call/construct signatures are modeled
  persistently, but generic/deferred indexed access (`T[K]`), optional methods, and member-path
  narrowing remain separate gaps. The remaining model-completeness track is in
  [`docs/backlog/`](./docs/backlog/README.md). The pinned TypeScript 6.0.3 ES2025 full-host default
  library is loaded from source on every production route. (Intersections `A & B` landed in M31;
  signature shape landed in M32;
  overloads landed in M33; `&` distribution over unions,
  `keyof`/indexed-access over an intersection, and overload-signature intersection remain
  deferred — see [`docs/reference/divergences.md`](./docs/reference/divergences.md).)
- **Optional properties** (`a?: T`) on objects/interfaces/class fields are implemented (M21): a
  member may be absent, reads yield `T | undefined`, `keyof`/indexed-access include it. Still
  deferred: optional **methods**/accessors (`go?(): T`), the dedicated *possibly-undefined*
  diagnostics (tsc `TS2532`/`TS18048`/`TS2722`), and narrowing an optional through a member-access
  guard (over-reports `T | undefined`, the safe direction). (Backlog `49`.)
- **Default-library and module profile.** Production checks use the exact pinned TypeScript 6.0.3
  ES2025 full-host library, compiled from its 82 vendored source files in each fresh process.
  The public `check --project-summary json <directory|tsconfig.json>` route accepts only the exact
  files-only strict/noEmit/ESNext/Bundler config and configured local `.ts` roots. It resolves local
  named imports, including extensionless and `.js`→`.ts` forms, through `oxc_resolver 11.24.2` and
  admits acyclic local named source re-exports over namespace-free value/type slots, including
  aliases, type-only forms, and chains. It emits deterministic complete accounting. Every
  unsupported form is an explicit non-clean notice. Still deferred: packages / `node_modules`,
  broader config/root selection, `.d.ts`, default/namespace/star imports, star/namespace
  re-exports, namespace-bearing source targets, CommonJS, ambient modules, cyclic module graphs,
  and parallel cross-file type identity. An **unresolved type name** in type position is
  `TK2304` (M22); still deferred there (distinct tsc codes): a value used as a type (`TS2749`), type
  args on a type parameter (`TS2315`) and a wrong type-argument count such as bare `Array`
  (`TS2314`). Qualified namespace types `A.B` and standalone instantiated namespace values are
  modeled. Ambient external modules remain with `15`; diagnosed ambient export-alias cascade parity
  remains with `63`. (Backlogs `15`, `52`, `63`.)
- **Incomplete checking is a first-class outcome (2026-07-10 accounting sprint).** The consumed
  OXC AST surface is classified in a machine-validated inventory (`tests/surface/`), and an
  unsupported in-scope construct now reports `incomplete[<surface-id>]` with exit `3` instead of
  silently exiting clean — for annotations, signatures, class members, statements/declarations,
  and every audited expression shape. The remaining expression-shape semantics are explicitly
  incomplete, never silently clean: `x!`/`a?.b` remain backlog `49`; traversal/iteration forms
  such as elisions, spreads, and tagged templates remain `71`; the other deferred surface tail is
  `75`. The pinned real-project preview gate (`72`) remains required. The bounded Bundler
  resolver/project substrate is shipped, but its replacement sprint also terminated incomplete:
  six screened public projects failed the unchanged zero threshold before mutations. There is no
  pinned public witness, mutation ratchet, or preview CI gate. The full default-library production
  cutover is shipped and archived after WU7
  independent **PASS** with zero unresolved HIGH/MEDIUM findings and exact-`d1aa6d4` remote CI.
  Acyclic local named source re-exports are shipped; default exports/imports are the next
  module-breadth target in `15`, followed by package breadth. A clean
  result on an arbitrary npm/Bun/Node project is not yet a completeness claim.
- Remaining `tsc` divergences are logged in
  [`docs/reference/divergences.md`](./docs/reference/divergences.md): known under-report families
  block 1.0 through manifest Track C; documented over-report/cosmetic tails are non-blocking only
  when they have an explicit owner and witness.

## Testing

```sh
cargo test                              # unit tests + the conformance corpus
cargo clippy --all-targets -- -D warnings
```

## Continuous integration

`.github/workflows/ci.yml` runs the same commands a contributor runs — one
separately-diagnosable job each: `cargo fmt --check`, `cargo test`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release`, the
official-suite harness unit tests (`python3 -m unittest test_tsofficial`), the
official-suite regression ratchet (`python3 tsofficial.py fetch`, then
`python3 tsofficial.py run --check`, which rejects regressions against the committed
`scoreboard.txt` while permitting progress), and the randomized differential harness
(`tooling/differential/` — its unit tests, the committed minimal-repro ratchet
`differential.py repros --check`, and a time-capped self-consistency fuzz sweep). A
weekly scheduled job re-verifies the differential harness's frozen `tsc 6.0.3` baselines
and prints its truth-mode dashboard. Formatting is enforced against the toolchain
pinned in `rust-toolchain.toml`, so run `cargo fmt` before pushing.
