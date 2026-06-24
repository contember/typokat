# typokat — handoff for the next agent

You are taking over `typokat`, a from-scratch TypeScript type checker in Rust. **M0–M22** are done
(see [`README.md`](./README.md)). This document is how to continue: the **method** to follow, the
**invariants** you must not break, and a **roadmap** of the next phases. For each phase you pick up,
**prepare a per-milestone plan the same way it was built so far** (spec first, then implement, then
independently review) — described below.

Read first: [`README.md`](./README.md) (what exists), [`ts-checker-architecture.md`](./ts-checker-architecture.md)
(the design + §12 phased plan), [`mvp-plan.md`](./mvp-plan.md) (how M0–M6 were scoped),
[`tests/cases/README.md`](./tests/cases/README.md) (the conformance/marker conventions + every
documented `tsc` divergence).

---

## 1. The method (follow this exactly — it is what made the project sound)

The build loop, per milestone `Mn`:

1. **Leader writes the spec first** = the fixture corpus. Create `tests/cases/mN_<topic>/*.ts` with
   inline `// error[TK…]: <substring>` markers (conventions in `tests/cases/README.md`). The
   fixtures are the acceptance spec and must be written **independently of** the implementation, so
   the review stays honest. Pick fixtures robust to subtle `tsc` choices (e.g. avoid depending on
   literal-widening; keep at most one mismatched argument per call). Update `tests/cases/README.md`
   (milestone index, any new `TK` code, deferred-list changes). **Commit the spec on its own**
   (`"Add Mn … corpus (spec)"`) — it does not change behavior because the dir is not yet enabled in
   `MILESTONE_DIRS`.
2. **Dispatch an implementation subagent** (`Agent`, general-purpose). Give it: the fixtures, the
   relevant architecture sections, which existing code to REUSE, the exact scope, the explicit
   **deferred** list, and the invariants below. It must leave `cargo build`/`clippy`/`cargo test`
   green and enable `"mN_<topic>"` in `MILESTONE_DIRS`. It must NOT commit.
3. **Dispatch an independent adversarial review subagent** (a *different* agent — independence is the
   point). Its job is to **hunt false negatives** (dropped errors — the worst outcome for a checker),
   verify no regression, confirm faithfulness, and **cross-check probes against real `tsc --strict`**
   (`tsc 6.0.x` is available). It returns **PASS/FAIL** with concrete repros.
4. **On FAIL**, route the fix back to the implementation agent via `SendMessage` (it keeps its
   context). For a soundness fix that touches the relation engine, re-review (via subagent) before
   committing; for a small prescribed fix, a gate-check is enough.
5. **Leader commits** once green. Verify yourself (`cargo test` + `clippy` + spot-run the fixtures)
   before committing — committing unverified code is the one thing the leader owns.

You are the **leader/orchestrator**: you write specs, dispatch agents, review their results, and
commit. Delegate implementation **and** review to subagents; do not write implementation code
yourself. Run agents in the background (`run_in_background`/`SendMessage`) when waiting.

### Commit conventions (from the repo's CLAUDE.md)
- Atomic: `git add <explicit paths> && git commit -m …` in one call. **Never** `git add -A`/`.`;
  list files (a whole `src/` or `tests/cases/<dir>` path is fine when it's all intended).
- Spec commit and implementation commit are **separate**.
- End every commit message with the two trailers (`Co-Authored-By:` + `Claude-Session:`) — copy the
  format from `git log`.
- Never `git stash`/revert work that isn't yours.

---

## 2. Invariants you must NOT break (the soundness/architecture core)

These were hard-won; regressing them silently drops errors.

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
  boundaries. All current narrowing is **structured** (`if`/`else`/`switch`) — see the roadmap for
  the unstructured-flow CFG.
- **No forbidden hacks**: no `unsafe`, no reachable `unwrap`/`panic`/`todo!`, no `as` casts except
  numeric arena-index/discriminant conversions, no `#![allow(warnings)]`. `clippy -D warnings` is
  clean — keep it that way.
- **Soundness > completeness**: when in doubt, **over-report** (a false positive in the safe
  direction). Document any deliberate `tsc` divergence in `tests/cases/README.md`.

### Deliberate deferrals already taken (don't be surprised; plan for them)
- **Type parameters are NAMED unique ids, not de Bruijn** (architecture §3.1 wants de Bruijn). This
  is the one foundational deferral that the **VM phase forces**: `infer` + alpha-equivalent
  hash-consing want de Bruijn. Plan the migration when you start conditional types (it is localized
  to the type-param repr + `substitute`).
- **Stable structural hash (blake3) is reserved but uncomputed** — needed for incrementality (Phase
  4). The interner is already shaped for it (`src/types/hash.rs`).
- **Per-argument call errors over-report** vs `tsc` (which stops at the first bad arg) — safe
  direction, documented.

---

## 3. Roadmap (recommended order; architecture §12 says: relation engine + narrowing before the VM)

Continue the `M`-numbering from M20. For each, **write the fixture corpus first**, then run the
loop. Sketches below are scope hints, not the full spec — you author that.

### Near-term: close documented gaps (low risk, high value — good to warm up)
- **M21 — Optional properties (`a?: T`). DONE.** Optional data members on objects/interfaces/class
  fields. Key design choice (the relation engine borrows `&Store` read-only and **cannot intern**):
  an optional member's `| undefined` is baked into its type at **lowering** (`union(T, undefined)`,
  where `&mut Interner` is live) and the member carries `optional: true`. With that, member-read /
  indexed-access return the (already-union) type, `keyof` uses names, excess sees a real member, and
  the relate "present" case reuses the union-target logic — the `optional` flag itself governs only
  **presence**: an absent optional *target* is allowed, and an optional *source* never satisfies a
  *required* target (presence is independent of value type — caught by review, see §4). Modeled with
  `exactOptionalPropertyTypes` OFF; no new diagnostic code. Deferred: optional **methods**/accessors
  (`go?(): T`, tsc `TS2722`), the dedicated possibly-undefined diagnostics (`TS2532`/`TS18048`), and
  member-access narrowing of optionals (over-reports, safe). One known cosmetic gap: the renderer
  omits the `?` marker, so an optional-vs-required object message can show identical-looking sides
  (verdict/code correct; object-target messages are asserted code-only).
- **M22 — `TK2304` in type position. DONE.** An unresolved simple-identifier type reference now
  reports `TK2304` at the single resolution chokepoint (`resolve_type_reference`), covering every
  type position via the existing `lower_annotation` recursion; it degrades to the error type, so
  cascade is suppressed (`const a: Foo = 5` is only `TK2304`). Fires only when the name resolves to
  *no* space — deferred-silent (distinct tsc codes, see `tests/cases/README.md`): value-as-type
  (`TS2749`), type-args-on-a-type-parameter (`TS2315`), wrong type-arg count incl. bare `Array`
  (`TS2314`), qualified names `A.B` (`TS2503`). The review caught an `Array`-builtin regression
  (bare/wrong-arity wrongly `TK2304` instead of degrading silently), now fixed. **Function
  overloads** (the optional half) remain deferred — a separate medium milestone (multiple call
  signatures, resolve to first match).
- **(later) method-override compatibility `TK2416`, abstract-not-implemented `TK2515`** — small class
  completeness checks deferred during the class phase. Good lighter warm-up before/instead of M23.

### Mid-term: narrowing completion + generics depth (both precede the VM)
- **M23 — Unstructured-flow narrowing (the flow-node CFG).** The biggest narrowing gap and the most
  likely to surprise on idiomatic code: `if (x === null) return; …` does not narrow after the early
  `return`; nor do `throw`, loops, `&&`/`||`/ternary, or assignment-in-flow. Build the flow-node CFG
  in `src/check/flow.rs` (the `FlowNode` stub anticipates it) and resolve a reference's type by
  walking the CFG backward applying guards. **Reuse the existing narrowing operations** (they were
  written flow-model-agnostic for exactly this). Architecture §5 — this is the "native interpreter"
  the design wants.
- **M24 — Generic constraints (`<T extends U>`).** Constraint checking on type arguments (`TK2344`),
  the constraint as a type parameter's *apparent type* (so `T extends {x:number}` permits `t.x`), and
  constraint-based inference. Prerequisite for conditional types and many real patterns.

### The marquee: type-level VM phase (architecture §7, do tree-walked FIRST, VM last)
- **M25 — Conditional types `T extends U ? X : Y`.** Evaluate the `extends` check via the relation
  engine; support `infer` extraction; distribute over unions. This is where you likely need the **de
  Bruijn migration** for `infer`.
- **M26 — Mapped types `{ [K in keyof T]: … }`.** Homomorphic, with `+?/-?`, `+readonly/-readonly`,
  and key remapping (`as`).
- **M27 — Template literal types** (`` `${A}-${B}` ``).
- **M28 — Utility types** (`Partial`, `Required`, `Readonly`, `Pick`, `Record`, `Exclude`, `Extract`,
  `Omit`, `NonNullable`, `ReturnType`, `Parameters`). Most fall out of mapped/conditional; a few are
  built-in.
- **M29 — The bytecode VM (§7).** Once conditional/mapped exist tree-walked, carve type-level
  evaluation into the IR → bytecode → stack VM with tail-call/accumulator-reuse/memoization and
  specialized arithmetic instructions. This is the order-of-magnitude marquee — and per §12 it comes
  **after** the relation engine + narrowing, never before.

### Long-term: real-world scale + IDE
- **`lib.d.ts` loading** — "mandatory core" (§4); needs generics + conditional/mapped (lib leans on
  them). Unlocks `console`, array methods, `Promise`, etc. — i.e. checking real code. Big.
- **Modules / imports / module resolution** — whole-repo checking; also where I/O cost dominates.
- **Phase 4 — incrementality** (Salsa-style; finally compute the blake3 stable hash) **+ parallelism**
  (rayon for parse/bind; resolve the shared-interner contention — sharded or per-thread arenas).

---

## 4. Lessons / what the reviews kept catching (so your reviews hunt for it)

Independent review caught these classes of bug that implementation-side tests missed — look for them:

- **Cache poisoning under recursion** (M5): provisional cycle assumptions cached durably → order-dependent dropped errors. The deepest soundness trap.
- **A path that was sound when a feature was secondary but became a hole when it was promoted** (M13:
  `static` bodies were skipped while statics weren't members; M15: get-only accessors inherited the
  `readonly`-field constructor carve-out). When you promote something to a real member/type, re-check
  every place that special-cased it.
- **Excess/freshness not recursing** through a new container (M19: index-sig values).
- **Contextual typing too aggressive or not applied** (M18: literal-tuple widening; array literals
  hijacked vs not).

General: every review should write throwaway `.ts` in the scratchpad, run `cargo run -- check`, and
diff against `tsc --strict`. Classify each divergence as false-negative (must fix), false-positive
(usually acceptable, document it), or regression (must fix).

---

## 5. Your first action

Pick the next milestone. Recommended: **M23 — unstructured-flow narrowing** (the flow-node CFG) —
the biggest remaining narrowing gap and the most likely to surprise on idiomatic code (`if (x === null)
return; …` does not narrow after the early `return`; nor do `throw`, loops, `&&`/`||`/ternary). Reuse
the existing flow-model-agnostic narrowing operations (architecture §5). For a lighter warm-up first,
the small class-completeness checks **`TK2416`** (method-override compatibility) and **`TK2515`**
(abstract-member-not-implemented) are well-isolated. Then: write its fixture corpus, update
`tests/cases/README.md`, commit the spec, dispatch the implementation subagent, dispatch the
independent review, fix, and commit. Same loop, all the way up.
