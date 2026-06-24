# MVP Plan: a runnable typokat checker

> Companion to [`architecture.md`](../reference/architecture.md). This plan turns
> the architecture into the **smallest thing that runs end-to-end** — `typokat check f.ts`
> that parses, binds, checks, and reports real type errors on a tiny TypeScript subset.
>
> The MVP's job is **not** to be fast. Its job is to nail the **layer boundaries** the
> architecture cares about so the SOTA pieces (type-level VM, incrementality, parallel
> interner) bolt on later **without rework**. Per §12: *completability is decided here.*

---

## 1. Guiding constraints for the MVP

1. **Vertical slice, always.** Every milestone runs end-to-end (parse → bind → check →
   diagnostic). We never build a "complete layer" before the pipeline runs. M0 already
   prints a real error.
2. **Get the shapes right, defer the contents.** The data structures and module boundaries
   match the architecture from day 1; the *coverage* is tiny and grows per milestone.
3. **Three things are built correct-from-day-1 because retrofitting them is a rewrite**
   (the architecture says so explicitly):
   - the **3×`u32` relation cache + assume-true cycle stack** (§6.1, §6.3),
   - the relation engine returning a **reason chain on failure**, not `bool` (§6.4),
   - the **multi-slot symbol** (value / type / namespace spaces) (§4.1).
4. **Every deviation from the architecture is conscious and logged** (§7 below), not an
   accidental omission.

---

## 2. MVP scope — the TypeScript subset we accept

**Language we check:**

- `const` / `let` with optional annotation + initializer; `let` reassignment.
- Primitive annotations: `number string boolean null undefined any unknown never void`.
- Literals: number, string, boolean, `null`, `undefined`.
- Object literal **types** `{ a: number; b: string }` and **expressions** `{ a: 1 }`.
- Member access `obj.prop`.
- Union types `A | B`.
- `function` declarations, function **type** annotations `(x: number) => string`, calls.
- `type` aliases and single-declaration `interface` (no merging yet).

**Errors we catch (the value proposition):**

| Code | Error | Milestone |
|---|---|---|
| `TK2322` | Type X not assignable to type Y (annotation vs initializer / reassignment) | M0/M1 |
| `TK2304` | Cannot find name (undefined identifier) | M1 |
| `TK2353`/`TK2741` | Excess / missing property on object literal | M2 |
| `TK2339` | Property does not exist on type | M2 |
| `TK2554` | Wrong number of call arguments (arity) | M3 |
| `TK2345` | Argument type not assignable to parameter | M3 |

(We reuse tsc's numeric codes for familiarity, `TK` prefix to be honest about source.)

**The single error M0 must nail:**

```
error[TK2322]: Type 'string' is not assignable to type 'number'
 --> f.ts:1:19
  |
1 | const x: number = "hello"
  |                   ^^^^^^^ 'string' is not assignable to 'number'
```

---

## 3. Crate / module layout (mirrors architecture §2)

Single binary crate for the MVP (fast to iterate). Modules map 1:1 to the architecture
layers, so promotion to a cargo **workspace** at Phase 2 is a mechanical split, not a
redesign.

```
typokat/
  Cargo.toml
  src/
    main.rs          // CLI: `typokat check <file.ts>`
    driver.rs        // pipeline orchestration, owns CheckContext
    span.rs          // span <-> line/col mapping
    diagnostics.rs   // Diagnostic, severity, rendering (codespan-reporting)

    types/           // §3 type store — the foundation
      store.rs       // SoA arena: tag/flags/payload parallel vecs, TypeId(u32)
      intern.rs      // Interner: hash-consing + canonicalization (§3.3)
      repr.rs        // TypeTag, TypeFlags, ObjectType, FunctionType, LiteralValue
      hash.rs        // pluggable structural hash (FxHash now, blake3 slot for §3.2)

    binder/          // §4 binder
      scope.rs       // ScopeGraph, Scope, ScopeId (parent-walk resolution)
      symbol.rs      // Symbol { value, ty, ns } — MULTI-SLOT from day 1 (§4.1)
      bind.rs        // AST walk -> scopes + symbols

    check/           // §5 statement-level checker (structural interpreter)
      checker.rs     // statements + expressions, drives relate + intern
      infer.rs       // inference entry points (trivial in MVP, real machine later §5.1)
      flow.rs        // flow-node stubs so narrowing slots in without rework (§5)

    relate/          // §6 relation engine — the biggest piece
      relation.rs    // is_assignable/is_subtype, structural rules, cycle stack (§6.3)
      cache.rs       // RelationCache: (u32,u32,RelationKind) -> Relation (§6.1)
  tests/
    cases/           // *.ts fixtures with inline `// ^ error: ...` expectations
    harness.rs       // runs checker, diffs diagnostics
```

---

## 4. Key data structures (concrete sketches)

### 4.1 Type store — SoA arena (§3.1)

Parallel arrays indexed by `TypeId`; payloads in typed side tables. This **is** the SoA the
architecture asks for (hot `tag`/`flags` packed, cold payload out of line).

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

#[repr(u8)]
pub enum TypeTag { Intrinsic, Literal, Object, Union, Function }

pub struct Store {
    tag:     Vec<TypeTag>,        // \
    flags:   Vec<TypeFlags>,      //  > parallel, indexed by TypeId — SoA hot path
    payload: Vec<u32>,            // /  index into the side table selected by `tag`
    // cold side tables:
    objects:   Vec<ObjectType>,
    unions:    Vec<Box<[TypeId]>>,
    functions: Vec<FunctionType>,
    literals:  Vec<LiteralValue>,
    // stable_hash: Vec<Hash>     // §3.2 slot — NOT populated in MVP (see §7)
}
```

**Well-known ids:** intern primitives first so `never`/`unknown`/`any`/`number`/… get
small fixed `TypeId`s usable as constants throughout.

### 4.2 Interner — hash-consing + canonicalization (§3.3)

```rust
pub struct Interner {
    store: Store,
    dedup: FxHashMap<u64, SmallVec<[TypeId; 2]>>,  // structural hash -> candidates
}
impl Interner {
    pub fn intern(&mut self, tag: TypeTag, payload: Payload) -> TypeId { /* canon, lookup-or-insert */ }
    pub fn union(&mut self, members: &mut Vec<TypeId>) -> TypeId {
        // §3.3: flatten nested unions, sort by TypeId, dedup, normalize
        //       (X|never -> X), collapse 1-member -> member, then intern.
    }
}
```

Structural equality becomes `id_a == id_b`. The hash function lives behind `hash.rs` so
swapping FxHash → blake3 (for §3.2 cross-run identity) is a one-module change.

### 4.3 Multi-slot symbol (§4.1) + scope graph

```rust
pub struct Symbol {
    name: Atom,
    value: Option<DeclId>,   // value space
    ty:    Option<DeclId>,   // type space
    ns:    Option<DeclId>,   // namespace space
}
pub struct Scope { parent: Option<ScopeId>, symbols: FxHashMap<Atom, SymbolId>, kind: ScopeKind }
```

MVP barely merges declarations, but the slots exist so §4.1 (`namespace`+`interface`+`class`
collapsing to one name) needs zero structural change later.

### 4.4 Relation engine — cache + cycles + reasons (§6.1/§6.3/§6.4)

```rust
pub enum RelationKind { Identity, Subtype, Assignable }
pub enum Relation { Yes, No(ReasonChain) }   // reason chain built only on the failing path

pub struct Relater<'a> {
    store: &'a Store,
    cache: FxHashMap<(u32, u32, RelationKind), bool>,   // §6.1
    stack: FxHashSet<(u32, u32, RelationKind)>,         // §6.3 assume-true-until-disproven
}
```

Structural rules in MVP: intrinsic lattice (`never <: T <: unknown`, `any` both ways),
literal → base widening, object property-wise (width + depth), function
contravariant-params/covariant-return (§6.5 — we pick soundness over tsc's bivariance),
union (`src` union ⇒ all members; `tgt` union ⇒ some member).

---

## 5. Milestones (each one is runnable)

| # | Adds | Acceptance (runs end-to-end) |
|---|---|---|
| **M0** | Walking skeleton: oxc parse → primitives interned → check `const x: prim = literal` → diagnostic with span | `const x: number = "a"` → 1 error; `= 1` → 0 errors |
| **M1** | Binder, scope resolution, infer `const` type from initializer, `let` widening + reassignment check, undefined-name error | `let x = 1; x = "a"` errors; `y` undefined errors |
| **M2** | Object literal types/expressions, member access, structural assignability (missing/excess/wrong prop), `TK2339` | object-shape fixtures pass |
| **M3** | Functions, function-type annotations, call checking (arity + arg assignability), contravariant params | call fixtures pass |
| **M4** | Unions + interner canonicalization | union fixtures pass; **unit test:** `A\|B` and `B\|A` intern to the same `TypeId` |
| **M5** | `type` aliases, single-decl `interface`, mutually recursive types | recursive-type fixture **terminates** and checks (exercises §6.3 fixpoint) |
| **M6** | Reporting mode: populate the reason chain into nested "because property X…" messages (hooks built in M0) | nested diagnostic snapshots |

**After the MVP**, in architecture order (§12's rule — relation engine + narrowing *before*
the VM): narrowing (`typeof`/truthiness/`in`, flow nodes from `flow.rs`) → real `lib.d.ts`
loading → generics → **then** the type-level VM (Phase 3) → incrementality (Phase 4).

---

## 6. Dependencies & testing

**Crates:** `oxc_parser` + `oxc_ast` + `oxc_allocator` (bumpalo AST arena) + `oxc_span` for
the frontend; `rustc-hash` (FxHashMap); `codespan-reporting` (diagnostics); `smallvec`.
Deferred: `blake3` (§3.2), `rayon` + `dashmap` (§8 parallelism).

**Testing is the product** (§11.1 — correctness is the whole game):

- **Fixture conformance** — `tests/cases/*.ts` with inline `// ^^^ error[TK2322]: ...`
  markers (rust-analyzer style). Harness runs the checker and diffs diagnostics. This corpus
  *is* the spec coverage and grows with every milestone.
- **Unit tests** on the two correctness-critical invariants: interner hash-consing/dedup &
  canonicalization, and relation-cycle **termination**.
- **Differential testing vs `tsc`/`tsgo`** on the supported subset (compare error presence)
  — added once the subset is large enough to be worth it.

---

## 7. Deliberate deviations from the architecture (logged, reversible)

The MVP follows the architecture; where it intentionally cuts a corner, here's what and why:

1. **Stable structural hash (§3.2) not computed.** The architecture wants blake3-at-intern
   because of incrementality — which the MVP doesn't have. We keep the interner *shaped*
   around "a hash computed at intern time" (pluggable in `hash.rs`, `stable_hash` field
   reserved) so adding blake3 in Phase 4 is a one-module change, not the rewrite the doc
   warns about. **Respects the concern, defers the cost.**
2. **No real `lib.d.ts`.** Biggest honesty caveat: without lib, code using `Array`,
   `console`, string methods, etc. can't be checked. The MVP corpus avoids the stdlib; a
   minimal *synthetic* set of globals stands in. `.d.ts` parsing/consumption is the first
   post-MVP item.
3. **No narrowing yet.** Flow-sensitive narrowing is core (§5) and must never be sacrificed
   — so `flow.rs` carries flow-node stubs and the checker is structured around them, but the
   MVP doesn't implement narrowing logic. It's the first capability added after the subset
   is solid.
4. **No type-level VM.** Per §12 this is correct: the VM is Phase 3. Trivial type-level
   needs in the MVP subset are handled directly by the interpreter.
5. **Single file, no module resolution.** Modules/`tsconfig`/project references are out of
   MVP scope (and are a known gap in the architecture doc itself for whole-repo perf).

None of these touch a layer boundary, so none force rework.

---

## 8. First step

`cargo new typokat` (bin) → add oxc + rustc-hash + codespan-reporting → implement
**M0 walking skeleton**: CLI, parse one file, intern primitives, check the
`const x: prim = literal` rule, render the `TK2322` diagnostic. That's the
"`muzeme to zkusit`" moment — a binary you can run on a `.ts` file the same day.
