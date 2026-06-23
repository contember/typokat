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
│  │ (structural interp.)  │◄►│ engine       │  │ VM         │  │
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
  whole checker — larger than the type-level VM. Most time in real repos
  (React/Next/Nest/Prisma) goes into repeated `isAssignable(A, B)`, not into conditional
  arithmetic.
- The **statement checker ↔ type-level VM boundary** (§9) is unmapped territory — no
  existing rewrite built both layers at once.

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
  (`<T>(x:T)=>T` vs `<U>(x:U)=>U`) hash-cons to the same node.

### 3.2 Two identities (internal vs cross-run)

`TypeId(u32)` is an arena index — it is only valid within a single process run. The moment
you want disk-cached bytecode (§7.1), incrementality (§11 phase 4), or any serialization,
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
- Driving the relation engine (§6) for every assignability question and the type-level VM
  (§7) for every type-level computation.

### 5.1 Inference is generative, not just relational

Easy to conflate with the relation engine, but it's a different machine. `isAssignable` is
a *decision* procedure (returns a verdict); inference is *generative* — it produces types
from constraints (`infer` extraction, contextual typing of arguments from an expected
type, type-argument inference for generic calls). It needs its own candidate-collection +
constraint-solving path, and it is roughly as large as the relation engine. Calling it out
so it doesn't get silently folded into either neighbor.

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

## 7. Type-level VM — bytecode

Type-level TypeScript is a purely functional language (no loops, only recursion).
Tree-walking it is inherently slow; a VM gives order-of-magnitude speedup (confirmed in
spirit by `tyvm` and `TypeRunner`). This is for conditional/mapped/`infer`, template
literals, and arithmetic.

**Scope note:** the VM is the *bonus*, not the main win. On ordinary application code the
big levers are arena layout, hash-consing, and the relation cache (§6) — see §10. The VM
shines specifically on type-level-heavy code (deep conditional/mapped DSLs, tuple
arithmetic).

### 7.1 Pipeline

```
type-level AST  →  IR  →  bytecode (disk-cacheable)  →  stack VM
```

The first two stages are the expensive ones and run once per file; bytecode is cached
(cold/warm split). **Caveat:** a warm cache is only valid if transitive *type*
dependencies are unchanged — tracking that graph is as hard as Salsa, it just looks
easier. Cross-run validity relies on the stable structural hash (§3.2), not on `TypeId`.

### 7.2 Recursion and tail calls — the go/no-go of this layer

Iteration = recursion, so without this the VM never leaves toy examples. Four required
minima:

1. **Tail-position analysis over IR** → a recursive self-call rewrites into a loop with
   frame reuse (`goto` to subroutine start with rewritten args). Fragility warning: an
   enclosing conditional whose result is further processed pushes the call out of tail
   position (exactly the bug tsc is finicky about). You can be *more correct* than tsc if
   you build this as clean data-flow over IR.
2. **Tuple / accumulator reuse** (tail-rest): grow `[...Acc, X]` in place instead of
   copying each iteration. Turns O(n²) accumulator building into O(n). Often a bigger win
   than frame elimination because allocations dominate.
3. **Memoization for non-tail (tree) recursion**: `(subroutine, args) → result`. Tree
   recursion (`Flatten<L> & Flatten<R>`) can't be tail-called; memoize + depth limit +
   cycle detection.
4. **Explicit heap frame-stack**, not host (Rust) recursion on `Call`. Otherwise you
   overflow the native stack before the logical type-level limit.

### 7.3 Specialized instructions

Arithmetic, which type-level TS "hacks" via tuple lengths and template strings (extremely
expensive), gets native VM instructions (`Add`, `Sub`, `Lte`…) — the `tyvm` lesson: the
cheapest recursion is the one you replace with a constant-time op.

### 7.4 Instantiation limits

Type-level code is shared over npm, so divergence here hurts more than in statement
checking. **Stay as close to tsc's limits and fragilities as possible** (~50 ordinary
recursion, ~1000 tail), even if your VM could do more — paradoxically the layer where you
are technically strongest is the one where deviating pays least. Code that passes tsc
should pass you.

---

## 8. Parallelism

- Per-file parsing and binding are mostly independent → rayon, scales well (the tsgo
  lesson: much of its 10× is shared-memory parallelism, not just native code).
- The per-unit scope-graph model enables concurrent unit type-checking.
- Main tension: the shared interner (§3.4). Resolve with sharding or per-thread arenas.

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
| Type-level heavy (conditional/mapped DSL, arithmetic) | **order of magnitude** | VM vs tree-walking (different algorithmic class) |
| Ordinary app code (subtyping + narrowing dominate) | **~1.5–3×** | layout: SoA, pointer-free arenas, no GC pauses, relation cache |
| Whole large repo | weighted average, pulled down by the type-level share | — |

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
2. **The VM ↔ interpreter boundary is unmapped.** When to hand a computation to the VM, how
   to return the result into the structural world, sharing `TypeId` across the boundary.
   No attempt has both layers — `tyvm` has only the top, the others only the bottom.
3. **Relation cache lifetime** (§6.2) is subtle: get it wrong and you either leak memory on
   narrowed ephemerals or lose the hit rate that makes the engine fast.
4. **Bytecode cache invalidation** (§7.1) is not free incrementality; tracking transitive
   type dependencies is as hard as Salsa.

---

## 12. Phased plan (against the "years in, still early stage" trap)

1. **Phase 0 — Type store + binder.** Arena, hash-consing, `TypeId` + stable hash,
   multi-slot scope graph, `.d.ts` parsing, load `lib.d.ts`. Nothing runs without this.
2. **Phase 1 — Structural interpreter + relation engine, narrow scope.** Subtyping +
   generics + **full narrowing** + the **relation cache and cycle handling** (§6). Inference
   engine (§5.1). Type-level eval still in the interpreter for now (slow but correct). Goal:
   a usable checker on a real repo as early as possible. Completability is decided here.
3. **Phase 2 — Scope hardening.** Variance, declaration merging (multi-slot), contextual
   typing, overloads, reporting mode (§6.4). Catching up on model coverage (the §11.1 wall).
4. **Phase 3 — Type-level VM.** Once the core stands, carve type-level eval out of the
   interpreter into the bytecode VM. This is where the order-of-magnitude type-level
   speedup arrives.
5. **Phase 4 — Incrementality (IDE).** Salsa-style layer over the binder with durability
   (lib/deps = HIGH, workspace = LOW). Per-file VM cache as a complement.

Plan rule: **the relation engine and narrowing come before the VM.** The VM is the
sexiest piece but the smallest share of real-world cost; the relation engine is the
largest. If time runs out, cut the VM before you widen scope.
