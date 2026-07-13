# Architecture: A SOTA TypeScript Type Checker in Rust

> A from-the-ground-up type checker design. Goal: architecturally state-of-the-art,
> performance beyond tsgo — order-of-magnitude on type-level code, constant-factor on
> ordinary code. Not 100% parity with tsc; the goal is to preserve the **type model**,
> not runtime and emit.

---

## 1. Guiding principle

One sentence the rest follows from: **sacrifice runtime and emit, keep types; give each
layer the computational model its problem wants.**

Two key consequences:

1. We are not a compiler, we are a *checker*. Emit, JS semantics, and runtime constructs
   go away.
2. Type-level computation and statement-level checking are two different problems and get
   two different machines over one shared type representation.

---

## 2. The layers

```
┌─────────────────────────────────────────────────────────────┐
│  Frontend:  oxc parser  →  AST (arena, bumpalo)             │
├─────────────────────────────────────────────────────────────┤
│  Binder:    scope graph, symbols (multi-slot), decl merging  │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │ Statement checker     │  │ Relation     │  │ Type-level │  │
│  │ (structural interp.)  │◄►│ engine       │  │ evaluator  │  │
│  │ flow analysis,        │  │ subtyping,   │  │ conditional│  │
│  │ narrowing, contextual │  │ assignability│  │ /mapped/   │  │
│  │ typing, inference     │  │ + caches     │  │ infer      │  │
│  └──────────────────────┘  └──────────────┘  └────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Type store: arena + hash-consing, TypeId(u32), SoA, stable  │
│              structural hash                                 │
└─────────────────────────────────────────────────────────────┘
```

Two pieces are easy to underestimate and are called out explicitly below:

- The **relation engine** (§6) is almost certainly the single largest CPU consumer in the
  whole checker — larger than type-level evaluation. Most time in real repos
  (React/Next/Nest/Prisma) goes into repeated `isAssignable(A, B)`, not into conditional
  arithmetic.
- The **statement checker ↔ type-level evaluator boundary** (§9) is unmapped territory — no
  existing rewrite built both layers at once. (It only sharpens into a *VM* boundary if the
  deferred bytecode refactor of §7 is ever undertaken — ADR-0001.)

---

## 3. Type store — the foundation, not an optimization

This is the entry toll for choosing Rust. A mutable cyclic graph of symbols and types is
exactly what drove the author of `stc` out of Rust into Go and back, and what led
Hejlsberg to pick Go for tsgo. In Rust you **must** design it as indices into arenas, not
`Rc<RefCell<…>>`. Punishment and reward are the same step: the very thing that is painful
in Rust is the biggest perf lever.

### 3.1 Representation

- Every type is a `TypeId(u32)` — an index into a central arena. No `Rc`, no `RefCell`,
  no ownership cycles.
- **Hash-consing**: structurally identical types share one node. Type construction goes
  through an interner that deduplicates. Consequence: structural equality is `id_a ==
  id_b`, i.e. an integer compare instead of a recursive walk.
- **SoA (struct-of-arrays) layout** for hot type attributes (flags, kind tag), so that
  comparison and filtering are cache-friendly and bit-packable.
- **de Bruijn indices** for type parameters and `infer`, so alpha-equivalent generics
  (`<T>(x:T)=>T` vs `<U>(x:U)=>U`) hash-cons to the same node. **Scoped by
  [ADR-0002](../decisions/0002-de-bruijn-scoped-to-infer-binders.md)**: indices apply to
  `infer` binders within conditional nodes; declaration type params stay named unique ids
  (context-free open types keep the relation cache sound); alpha-equivalence of generic
  *declarations* is a measured, deferred optimization.

### 3.2 Two identities (internal vs cross-run)

`TypeId(u32)` is an arena index — it is only valid within a single process run. The moment
you want disk-cached bytecode (§7.1), incrementality (Phase 5), or any serialization,
the in-memory index is useless because the next run assigns different numbers. So every
type needs **two identities**:

- `TypeId(u32)` — fast, run-local, used everywhere in-process (cache keys, VM operands).
- **Stable structural hash** — content-addressed (e.g. blake3 over the canonical
  structure), computed at intern time, used for cross-run identity: disk cache validation,
  incremental invalidation, bytecode serialization.

This is an architectural requirement, not a detail: computing the stable hash at intern
time changes the interner's design. (Same pattern as rustc's `DefId` vs stable hash.)

### 3.3 Canonicalization (a precondition of hash-consing, not a detail)

Canonicalize before interning, or sharing falls apart:

- Unions/intersections: sort members by `TypeId`, dedup, flatten nested
  (`A | (B | C)` → `A | B | C`).
- Normalize trivial cases (`X | never` → `X`, `X & unknown` → `X`).
- Literals carry a precomputed 64-bit hash (à la TypeRunner) so literal-type comparison is
  an integer compare and they can key hash maps.

**Cost caveat (benchmark early):** canonicalizing huge unions (sort + dedup over N
members, and TS routinely has unions of hundreds of members) is real work. It is a *trade*,
not a loss — paid once at construction, repaid on every later comparison (now an integer
compare), and it raises relation-cache hit rate (§6). The one case where it doesn't repay
is single-use giant unions. Mitigation: canonicalize lazily on first comparison, or only
above a size threshold. This is tuning, not structure — but measure it early.

### 3.4 Parallelism vs the interner (one node, two axes)

A shared growing interner is global mutable state — the enemy of parallelization. Options:
a sharded interner (`DashMap`-like) or per-thread arenas with a deterministic merge at
phase boundaries. This decision couples interning (§3) and parallelism (§8) into one knot;
solve it in the design, don't bolt it on.

---

## 4. Binder — scope graph with multi-slot symbols

- Model name resolution and types as a **scope graph** (Visser/Delft line). It gives a
  unified resolution model and a basis for later incrementality and per-unit parallel
  checking.
- `.d.ts` consumption is **mandatory core**, not optional: without it you can't load
  `lib.d.ts` or `@types`. Namespaces are therefore kept carefully on the type side
  (`lib.d.ts` uses them).

### 4.1 Declaration merging is a design requirement, not a special case

This is the part where a clean scope graph meets TypeScript's mutant side. The canonical
nasty case:

```ts
namespace A {}
interface A {}
class A {}
```

These collapse into one name `A` carrying three different meanings. Hiding this under
"degrade exotic combinations" (§9) is a dodge: merging is a *type-side* concern, and
`namespace`+`interface` merges occur in `lib.d.ts` itself (`Symbol`, `globalThis`), so it
can't all be sacrificed.

The fix is structural, not a pile of special cases: a symbol is **not** a single binding
but a set of slots over **separate declaration spaces** — *value space*, *type space*,
*namespace space*. One name can occupy several. This is exactly how tsc models it
(`SymbolFlags` bitmask), and a scope graph carries it cleanly **if** designed with this
multiplicity from the start. So: not an argument against scope graphs — an argument that
the scope graph's node must be multi-slot by design.

Which merges to keep correct vs degrade:

- **Keep:** `interface`+`interface`, `namespace`(type container)+`interface`,
  `function`+`namespace`, `class`+`namespace` (statics).
- **Degrade (unsound but harmless):** the rare three-way `enum`+`namespace`+`function`
  chimeras.

---

## 5. Statement-level checker — structural interpreter

Here we go close to `checker.ts`'s structure (the `stc` philosophy), **not** abstract
interpretation (the `ezno` philosophy). Reason: narrowing and flow analysis are
flow-sensitive and need a native interpreter model; they cram awkwardly into a stack VM.
Closeness to checker.ts also means new TS features are *ported*, not reverse-engineered.

Responsibilities:

- **Control-flow narrowing**: `typeof`, `in`, truthiness, equality, discriminated unions,
  assertion functions. This is *the* reason people love TS DX — keep it correct, never
  sacrifice it.
- Contextual typing and the **inference engine** (see §5.1).
- Overload resolution.
- Driving the relation engine (§6) for every assignability question and the type-level evaluator
  (§7) for every type-level computation.

### 5.1 Inference is generative, not just relational

Easy to conflate with the relation engine, but it's a different machine. `isAssignable` is
a *decision* procedure (returns a verdict); inference is *generative* — it produces types
from constraints (`infer` extraction, contextual typing of arguments from an expected
type, type-argument inference for generic calls). It needs its own candidate-collection +
constraint-solving path, and it is roughly as large as the relation engine. Calling it out
so it doesn't get silently folded into either neighbor.

### 5.2 Current checker shape

The implementation keeps type construction and relation queries separated because the interner
requires `&mut Interner` while the relation engine borrows the store immutably:

1. **Prelude unit.** `src/prelude.ts` is parsed, bound, reserved, and checked first in the same type
   universe as the user program. The user pass keeps lifetime-free resolved type placeholders and
   the prelude's `DeclId → TypeId` value entries; string intrinsic aliases are then seeded to
   well-known marker types. Ordinary slot-aware parent lookup keeps user value/type declarations
   able to shadow their corresponding prelude slots independently.
2. **Declaration fill.** Type declarations are reserve-then-fill. Interfaces, classes, conditional
   aliases, mapped aliases, and recursive object-literal aliases reserve stable ids before their
   bodies lower, so self and sibling references store ids rather than expanding inline. Generic
   declarations lower as templates with named type-parameter ids embedded; references with type
   arguments instantiate by substitution, except conditional/mapped/intrinsic templates which stay
   lazy until evaluation is demanded.
3. **Flow graph.** A pre-pass builds flow nodes for module and function bodies before checking, so
   loop back edges are complete when identifier reads ask for narrowed types.
4. **Statement walk.** The checker lowers annotations, infers expressions, checks bodies, records
   assignability and override obligations, and emits immediate name/arity/access diagnostics.
5. **Relation phase.** Obligations run through one relater instance, so assignability diagnostics and
   class override diagnostics share the same immutable store view and relation cache discipline.

Named type declarations are intentionally id-based. Interfaces and class instances reserve object ids;
transparent aliases memoize their resolved target; object-literal aliases seed a reserved object only
for the non-generic recursive shape. Type parameters are named unique ids, while `infer` binders use
the scoped de Bruijn representation described in ADR-0002.

Generic binders are persistent fields of every generic `FunctionType`: free functions and class,
interface/object method, call, and construct signatures share ordered descriptors carrying a unique
parameter id plus optional constraint/default. Outer substitution preserves those inner descriptors
while rewriting their free outer references; call instantiation consumes the selected signature's
descriptors through the existing inference/constraint path. Generic-to-generic relation aligns the
descriptors locally and bypasses the durable cache below that binder context, so only the completed
outer relation remains cacheable. This is the B41 representation recorded in ADR-0005.
It does not synthesize instance members or load library declarations; generic/deferred indexed
access (`T[K]`), optional methods, and explicit `this` parameters remain separately owned tails.

Classes bind both spaces: the type slot is the instance type, and the value slot is the static side /
constructor lookup key. Instance and static members are composed base-first with own declarations
overriding by name. Private/protected members carry `visibility` plus `declaring_class` identity for
access control and nominal relation checks; `readonly` and accessor metadata gate assignments but do
not affect assignability. Constructor signatures, abstract-member state, override-kind state, and
constructor accessibility live beside `ClassInfo` when they are not part of the copyable `new` path.

---

## 6. Relation engine — probably the biggest single piece

Historically the largest CPU consumer in a TS checker, bigger than evaluating conditional
types. In real repos, time disappears into repeated `isRelatedTo(A, B, relation)`, not
into type-level arithmetic. It deserves first-class architectural treatment, not a
footnote.

### 6.1 Relation caches

With stable `TypeId`s, the cache key is three `u32`s — extremely cheap:

```
(TypeIdA, TypeIdB, RelationKind)  →  Result
```

- `RelationKind`: identity, subtype, assignable, comparable, strict-subtype. They are
  *different* relations with different rules and must not share a cache.
- This cache may be the single largest perf element of the whole checker; good
  hash-consing makes it brutally effective because structurally equal types collapse to
  one key.

### 6.2 Cache lifetime vs narrowing (the trap)

Narrowing creates swarms of short-lived types (every narrowing = a new type). A naively
global relation cache gets polluted by these ephemerals and grows unbounded. So the cache
needs a **lifetime strategy**: a per-checking-session / per-flow scope for the volatile
part, with only durable relations (between named, interned, non-narrowed types) promoted
to the long-lived cache. This is why it's an architectural concern, not "just turn on a
HashMap."

### 6.3 Recursive subtyping cycles

Mutually recursive types (`interface A { x: B }`, `interface B { x: A }`) make
`isRelatedTo` inherently cyclic. You need an **assume-true-until-disproven** fixpoint: when
you re-enter a relation already on the stack, assume it holds and continue, resolving the
fixpoint at the end. Without this, recursive types loop forever. This is the sharpest edge
of the engine and must be in the core design.

### 6.4 Reporting mode (error messages run *through* here)

The real error-reporting pain lives in the relation engine, not the VM. "X is not
assignable to Y because property Z…" requires `isRelatedTo`, **on failure**, to return a
*chain of reasons* (which property, at which depth, why), not just `false`. So the engine
can't be a pure `bool` function — it must run in a "reporting mode" that builds a cause
tree on the failing path. Design this in from the start; retrofitting it onto a boolean
engine is a rewrite. (This is also the answer to the "VM error reporting is bad" worry —
most user-facing errors are relation failures, not VM faults.)

### 6.5 Variance

tsc has bivariant method parameters (deliberate unsoundness for the DOM). You may choose
consistently contravariant parameters — a deviation from tsc *toward* greater soundness,
defensible under "not 100% parity," saving a whole layer of heuristics. Variance
measurement for generics is itself cached (`(GenericId, ParamIndex) → Variance`).

---

## 7. Type-level evaluation — tree-walked, with the bytecode VM as a deferred refactor

Type-level TypeScript is a purely functional language (no loops, only recursion). It is
evaluated by a **tree-walked evaluator** (built with the conditional/mapped/template/utility
milestones, backlog `09`–`12`). Its performance comes from four *algorithmic* properties folded
into the tree-walker — **memoization, accumulator reuse, an explicit work-stack, and arithmetic
intrinsics (§7.2)** — which carry the order-of-magnitude wins and are *required*, not optional.

A **bytecode VM** (IR → bytecode → stack VM, §7.1) is a **potential later refactor**, not a planned
pillar — undertaken only if profiling on real type-level-heavy code shows the *interpreter dispatch
loop itself* (not the algorithm, not relation/instantiation) is the bottleneck. The rationale and
evidence for that demotion are **[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)**;
the rest of §7 is the design reference *if* that refactor is ever justified.

**Scope note:** even at its best the VM is a *bonus*, not the main win. On ordinary application code
the big levers are arena layout, hash-consing, and the relation cache (§6) — see §10. The order-of-
magnitude wheelhouse is a narrow slice — type-level *programs* (parsers, tuple arithmetic, deep
recursive transforms) — and on that slice the algorithmic wins (§7.2), not bytecode dispatch, are
what move the needle; bytecode adds a constant-factor on top.

### 7.1 Pipeline (the refactor's shape, if undertaken)

```
type-level AST  →  IR  →  bytecode (disk-cacheable)  →  stack VM
```

The first two stages are the expensive ones and run once per file; bytecode is cached
(cold/warm split). **Caveat:** a warm cache is only valid if transitive *type*
dependencies are unchanged — tracking that graph is as hard as Salsa, it just looks
easier. Cross-run validity relies on the stable structural hash (§3.2), not on `TypeId`.

### 7.2 The four algorithmic wins — these belong in the *tree-walker*, not a VM

Iteration = recursion, so a naive recursive evaluator never leaves toy examples. The four required
minima below carry the order-of-magnitude wins — and **three of the four are pure tree-walker work,
orthogonal to bytecode** (ADR-0001). Build them into the evaluator as items `09`–`12` land:

1. **Memoization for non-tail (tree) recursion**: `(subroutine, args) → result`, keyed on hash-
   consed argument `TypeId`s. Tree recursion (`Flatten<L> & Flatten<R>`) can't be tail-called;
   memoize + depth limit + cycle detection. The single biggest lever — *no VM required*.
2. **Tuple / accumulator reuse** (tail-rest): grow `[...Acc, X]` in place instead of copying each
   iteration. Turns O(n²) accumulator building into O(n). Often a bigger win than frame elimination
   because allocations dominate — *no VM required*.
3. **Explicit heap work-stack (trampoline)**, not host (Rust) recursion on `Call`. Otherwise you
   overflow the native stack before the logical type-level limit — *no VM required*.
4. **Tail-position analysis** → a recursive self-call rewrites into a loop with frame reuse. This is
   the one item that benefits from an IR: a clean data-flow pass over IR can be *more correct* than
   tsc (an enclosing conditional whose result is further processed pushes the call out of tail
   position — exactly the bug tsc is finicky about). Achievable over the tree with an explicit
   work-stack; cleanest if/when the bytecode refactor (§7.1) materialises.

Three private evaluator walkers use heap task/value stacks for every structural child
they traverse, including function type-parameter constraints and defaults. They stay
separate rather than becoming a generic visitor because their policies are distinct:

- `InferRewrite` owns lexical fresh binders, a per-run completed memo, and SCC-suffix
  identity taint.
- `InferenceConstraintEvaluator` delegates pending types and conservatively restores
  a structural input after global exhaustion.
- `MappedRewrite` is per assembled mapped-property value; its local memo and
  `in_progress` set return the original `TypeId` on re-entry, allowing a completed
  ancestor to memoize a partial clone. It has neither SCC taint nor evaluator budget
  semantics.

These are bounded hardenings of named structural walks, not a claim that all evaluator
or repository recursion has disappeared. The remaining local `keyof`-intersection and
template-union helper recursion is safe under its current contracts because the
interner flattens nested intersections and unions before those helpers traverse them.
Direct arena tests exercise 10k+ deep acyclic metadata and mapped-value paths without
depending on parser nesting or the driver's enlarged worker stack.

### 7.3 Specialized arithmetic (intrinsics — also tree-walker work)

Arithmetic, which type-level TS "hacks" via tuple lengths and template strings (extremely
expensive), is computed natively (`Add`, `Sub`, `Lte`…) — the `tyvm` lesson: the cheapest recursion
is the one you replace with a constant-time op. These are **intrinsic type functions** the evaluator
intercepts; they need no bytecode, only pattern recognition. As VM opcodes they're marginally
faster, but the order-of-magnitude is in *not recursing at all*, which the tree-walker captures.

### 7.4 Instantiation limits

Type-level code is shared over npm, so divergence here hurts more than in statement
checking. **Stay as close to tsc's limits and fragilities as possible** (~50 ordinary
recursion, ~1000 tail), even if your evaluator could do more — paradoxically the layer where you
are technically strongest is the one where deviating pays least. Code that passes tsc
should pass you.

---

## 8. Parallelism

The win is real (the tsgo lesson: much of its 10× is shared-memory parallelism, not
just native code), but the *shape* is dictated by a hard constraint the original
"rayon over parse+bind" sketch missed: **the oxc AST is thread-pinned.**

### 8.1 The thread-pinned-AST constraint (this decides the shape)

`oxc_ast::Program` is **neither `Send` nor `Sync`**: its arena `Vec`s are
deliberately `!Send` (two threads must never allocate into one arena), and its
nodes hold `Cell`s (so it is `!Sync`). A parsed AST can therefore be neither
**moved** to another thread nor **shared** by reference — it is pinned to the
thread that parsed it.

Consequence: you cannot parse on a worker and then type-check it on a *serial*
thread against a shared interner (the AST can't travel to the checker), nor share
`&AST` across a barrier. The only thing a worker can export is **owned, `Send`
data** it produced by consuming its AST in place. So the unit of parallelism is the
**whole per-file pipeline** (parse → bind → check), *not* parse+bind alone — each
file owns its `Allocator` and runs end-to-end on one thread, returning owned
diagnostics. (The binder helps: `bind_module` is interner-free and `Binder` is
fully owned, so the parse+bind sub-phase touches no shared state whatsoever.)

### 8.2 The type universe is the real axis — build it in stages

Per-file isolation is free only while files share no types. They eventually will.
Stage the shared substrate so each step keeps as much parallelism as possible:

- **Stage 0 — per-file own interner (the baseline, in place today).** Each file is a
  self-contained universe = intrinsics + its own declarations. Sound and *lossless*
  exactly while there is no cross-file resolution (no `lib.d.ts`; the M29 project
  checker deliberately leaves this API untouched). Maximal parallelism, zero sharing
  — the correct floor, not a stopgap.
  Implemented as `driver::check_files` (rayon over a `check_source`-per-file).
- **Stage 0.5 — correctness-first serial project checker (M29).** Local relative
  `.ts` modules, named imports/exports, and simple export lists are checked in a
  single serial `Interner`, so cross-file `TypeId` identity is ordinary run-local
  identity. This proves module semantics before the parallel type-universe problem
  is solved. It is implemented as `driver::check_project` and is the CLI path for
  `typokat check <files...>`.
- **Stage 1 — shared *read-only* prelude.** `lib.d.ts` + intrinsics form a large,
  immutable, universally-needed base; re-seeding them into N per-file interners is
  absurd. Freeze a base `Store` once and share it `&`-immutably across workers —
  immutable shared data is `Sync`-friendly; the §3.4 enemy is only the *growing*
  interner. Each file layers a private **delta** arena over the frozen base.
  Parallelism survives intact. This is the first real cross-unit type-sharing, and
  it arrives with `lib.d.ts` (mandatory core, §4).
- **Stage 2 — cross-file *mutable* exports.** `export interface Foo` in A consumed by
  B: A must emit its **public type surface** (`export name → type`), and B must give
  those types identity in *its* world. A run-local `TypeId` (§3.2) is meaningless
  across interners, so this needs the **stable structural hash** (§3.2): serialize an
  export by content-hash, re-intern in the consumer. The alternative is one shared
  *growing* interner — the full §3.4 knot (sharded, or per-thread arenas with a
  deterministic merge at phase boundaries). This is the genuinely hard step; the M29
  serial project checker exists specifically so this can remain a separate Stage 2
  decision.

### 8.3 Invariant across all stages

Parse + bind is **always** per-file-parallel and interner-free (`Binder` is owned; it
never touches types). Only the *type universe / check phase* climbs the Stage 0→1→2
ladder. The reserved `stable_hash` column in the type store (§3.2) exists precisely
for Stage 2; computing it is shared work with incrementality (Phase 5).

---

## 9. Scope — what to sacrifice

The cut runs between **type semantics** (keep, cheap) and **runtime/emit semantics**
(sacrifice, expensive). Enum and namespace lie on *both* sides of that line at once.

| Sacrifice hard | Degrade (unsound but harmless) | Keep correct (core) |
|---|---|---|
| **JS emit** (all of it) | typed JS / `checkJs` | structural subtyping |
| **enum runtime** (reverse map, IIFE, const-enum inlining) | three-way decl-merge chimeras | generics + constraints + `const` params |
| **namespace value/runtime** + `import =`/`export =` | polymorphic `this` | control-flow narrowing |
| JSDoc types in JS | variance exactly (→ more soundness) | conditional / mapped / `infer` |
| legacy compiler flags | — | template literal types |
| | | inference engine |
| | | `.d.ts` parsing + consumption |

**Common-mistake correction:** enum *on the type side* (= union of its members, for
narrowing in `switch`) and namespace *on the type side* (= named type container, `N.T`)
belong to the **core**, not to the sacrifice column. Only their runtime/value side is
sacrificed. Namespaces on the type side you don't even *get* to sacrifice — `lib.d.ts` and
`@types` require them.

---

## 10. Honest performance expectations

Existing rewrites' benchmarks (TypeRunner ~400×) **cannot be extrapolated** to a fight with
tsgo: they measure a toy (missing subtyping/narrowing/lib.d.ts), a warm cache (skipping
work), and *old tsc* (the JS tax tsgo already paid). The more of the type system you
implement, the more your profile converges to tsgo's, because the expensive work — the
relation engine — is the same work in Go or Rust.

Realistic estimate vs **tsgo**:

| Code class | Expected speedup over tsgo | Source of the win |
|---|---|---|
| Type-level heavy (conditional/mapped DSL, arithmetic) | **order of magnitude** | memoization + accumulator reuse + arith intrinsics (§7.2) — a different algorithmic class, *in the tree-walker*; a bytecode VM adds only a constant factor on top (ADR-0001) |
| Ordinary app code (subtyping + narrowing dominate) | **~1.5–3×** | layout: SoA, pointer-free arenas, no GC pauses, relation cache |
| Whole large repo | weighted average, pulled down by the type-level share | — |

Caveat on the first row: the order-of-magnitude applies to a *narrow* slice — type-level
*programs* (parsers, tuple arithmetic, deep recursive transforms). Instantiation-heavy code that
merely *looks* type-level (Zod-class validators: `.extend`/`.omit` chains) is dominated by
instantiation count + structural relation checks, which the relation engine (§6) and lean
instantiation address — **not** the evaluator. See ADR-0001.

That is a defensible **SOTA architecture** — measurably faster than tsgo, dramatically so
on type-level code. But "faster tsc" is SOTA in *engineering*; SOTA in *type systems*
(soundness, effect tracking — the `ezno` direction) is a different, more distant goal. This
design targets the former.

---

## 11. Risks (honest)

1. **The wall all five attempts hit: subtyping/variance/narrowing correctness.** It's
   undocumented (the spec *is* checker.ts). No architecture sidesteps it — only grinding
   does. It's 10× the work of infra and is the real reason existing rewrites are
   unfinished, not speed.
2. **The VM ↔ interpreter boundary is unmapped** — *now off the critical path* (ADR-0001 defers the
   VM): when to hand a computation to the VM, how to return the result into the structural world,
   sharing `TypeId` across the boundary. No attempt has both layers — `tyvm` has only the top, the
   others only the bottom. This risk only re-arms if the bytecode refactor is ever undertaken.
3. **Relation cache lifetime** (§6.2) is subtle: get it wrong and you either leak memory on
   narrowed ephemerals or lose the hit rate that makes the engine fast.
4. **Bytecode cache invalidation** (§7.1) is not free incrementality; tracking transitive
   type dependencies is as hard as Salsa.

---

## 12. Phased plan (against the "years in, still early stage" trap)

1. **Phase 0 — Type store + binder.** Arena, hash-consing, run-local `TypeId`, multi-slot
   scope graph, and enough `.d.ts`-shaped binding machinery to keep declaration spaces honest.
   The full stable structural hash and full `lib.d.ts` are staged later; the type store is already
   shaped for them, but they are not prerequisites for the single-file checker.
2. **Phase 1 — Structural interpreter + relation engine, narrow scope.** Subtyping +
   generics + **full narrowing** + the **relation cache and cycle handling** (§6). Inference
   engine (§5.1). Type-level eval still in the interpreter for now (slow but correct). Goal:
   a usable checker on a real repo as early as possible. Completability is decided here.
3. **Phase 2 — Scope hardening.** Variance, declaration merging (multi-slot), contextual
   typing, overload-bearing signatures, reporting mode (§6.4), and a minimal ambient/prelude loading slice when it
   buys real-world feedback before the full standard library is viable. Catching up on model
   coverage is the §11.1 wall.
4. **Phase 3 — Type-level evaluator (tree-walked).** Once the core stands, build conditional/
   mapped/template/utility evaluation with the four algorithmic wins folded in (§7.2: memoization,
   accumulator reuse, explicit work-stack, arith intrinsics). This is where the order-of-magnitude
   type-level speedup arrives — *in the tree-walker*. A bytecode VM (§7.1) is a **deferred,
   profiling-gated refactor**, not part of this phase (ADR-0001).
5. **Phase 4 — Real-project scale.** Full `lib.d.ts` as a shared read-only prelude (parallelism
   Stage 1), then modules/imports as an explicit staged rollout: first a correctness-first
   whole-repo slice, then the cross-file type-identity strategy (stable structural hash or a shared
   growing interner) needed for parallel Stage 2.
6. **Phase 5 — Incrementality (IDE).** Salsa-style layer over the binder with durability
   (lib/deps = HIGH, workspace = LOW). This is where the stable structural hash becomes mandatory
   if it has not already landed for cross-file exports. A per-file bytecode cache is a complement
   *if* the VM refactor ever happens.

Plan rule: **the relation engine and narrowing come before the type-level evaluator, and the
evaluator's speed lives in its algorithms, not in a VM.** The bytecode VM is the sexiest piece but
the smallest share of real-world cost and the least-mapped risk (§11.2); it stays a
profiling-gated option. If time runs out, it is the first thing cut. Full `lib.d.ts`, modules,
parallel cross-file identity, and incrementality are staged separately so real-project feedback can
arrive before every scale feature is solved at once.
