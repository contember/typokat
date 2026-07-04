<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — generic constraints `<T extends U>` / M24 (2026-07-04)

**Goal.** Ship backlog [`08`](../backlog/08-generic-constraints.md): constraints are checked on
explicit type arguments (`TK2344`), serve as the type parameter's **apparent type** (member
access + `T → constraint` assignability), and participate in inference (candidate clamped by the
constraint, argument check reports `TK2345`).

**Theme.** The last prerequisite before the type-level evaluation phase (`09` needs constraints
for the `extends` check). Closes both false negatives (no `TK2344`/`TK2345` today) and false
positives (member access through a constraint reports `TK2339` today; `return t` against the
constraint reports `TK2322`).

## Refs re-verified at HEAD (2026-07-04, 5651152)

- ✔ **Type params are named unique ids** — `src/types/repr.rs:362-383`: `TypeParamId(u32)`,
  `TypeParamType` identity = the id alone. Constraints are stored NOWHERE today. The de Bruijn
  migration is scheduled at the start of `09` (invariants §2) — the constraint slot must be a
  **side column keyed by `TypeParamId`** (not folded into the interned type's identity) so the
  migration only re-keys it.
- ✔ **Frames map name → `TypeParamId`** (`with_type_params`, `src/check/checker/context.rs`);
  generic fns/classes/aliases/interfaces carry `Vec<TypeParamId>` (context.rs:118-173,315).
- ✔ **Inference is `infer_type_arguments(&mut interner, &[TypeParamId], targets, sources)`**
  (`src/check/infer.rs:144`); un-inferrable params fall back to `unknown` today.
- ✔ **Explicit instantiation sites**: `instantiate_type_reference` (type refs, `decls.rs`),
  `new_class_substitution` (`calls.rs`), generic-call explicit args. These are where `TK2344`
  fires.
- ✔ **Member access on a bare `T` reports `TK2339` today** (probe: `t.x` with
  `T extends {x:number}` → `Property 'x' does not exist on type 'T'`) and **`T → constraint`
  assignability fails today** (`return t` as the constraint type → `TK2322`) — both are the
  over-report side the apparent type fixes. `constraint → T` correctly stays an error.
- ✔ **tsc 6.0.3 probes** (scratchpad `probe_m24.ts`): explicit bad arg → `TS2344`
  (`Type 'X' does not satisfy the constraint 'Y'`, on the type argument, and the bad argument
  still instantiates — no double-report); `t.y` → `TS2339` on type `'T'`; inference candidate
  violating the constraint → the argument check reports **`TS2345` against the constraint**
  (`g(5)` with `T extends string` → `'number' not assignable to 'string'`); interface/alias
  type references check `TS2344` at the argument position; object-literal-to-constraint
  contextual typing (tsc reports `TS2353` there) is OUT of scope — fixtures avoid it.

## Work units

### WU1 — Constraint storage + the three behaviors (effort M/L)

- **Scope.** (1) Lower each type parameter's `extends` annotation (in the scope where the
  parameter list is declared, with the frame active so `<T, U extends T>` resolves) into a
  **store-side constraint column** keyed by `TypeParamId`, readable by the relation engine.
  (2) **Apparent type**: member access on a `TypeParam` with a constraint resolves through the
  constraint's members; the relation engine treats `TypeParam(T) → X` as satisfiable via
  `constraint(T) → X` (one direction only — `X → TypeParam(T)` stays failing). Cache stays
  sound: TypeParam types are interned per id, so the 3×`u32` key is unchanged.
  (3) **`TK2344`** on every explicit instantiation site (calls, `new`, type references), message
  `Type '{A}' does not satisfy the constraint '{C}'.` with the reason chain, on the offending
  type argument; the bad argument still instantiates. (4) **Inference**: a candidate violating
  its constraint clamps to the constraint (so the argument check reports `TK2345`); the
  `unknown` fallback for un-inferrable params becomes the constraint when one exists.
- **Acceptance / witness.** `m24_generic_constraints/` (3 files, committed spec) enabled and
  green; no regression in m9/m10/m16 generic corpora; official-suite `run --check` zero NEW
  regressions.
- **Touch points.** `src/types/` (constraint column), `src/binder`/`decls.rs`/`classes.rs`
  (lowering at each param-list site), `src/relate/relation.rs` (apparent-type rule),
  `src/check/checker/expr.rs` (member access), `calls.rs`/`decls.rs` (TK2344 sites),
  `src/check/infer.rs` (clamping), `src/diagnostics.rs` (TK2344).

### WU2 — Independent adversarial review + ratchet (effort M)

- **Scope.** Hunt false negatives: constraint referencing an earlier param (`<T, U extends T>`),
  self-referential shapes (`<T extends { self: T }>` — must terminate via the cycle stack),
  constraint chains (`U extends T`, `T extends HasX` — transitive apparent type), generic class
  instantiation via both explicit args and inference, interfaces/aliases, order-independence,
  relation-cache soundness (a `T → X` verdict must not leak to a different `T'`). Cross-check
  vs tsc. Then ratchet.

## Out of scope (explicit)

- `keyof T` constraints (`K extends keyof T`) — the generic `keyof` deferral (type-level phase).
- Contextual typing of arguments against the constraint (tsc's `TS2353` shape) — fixtures avoid.
- Type-parameter **defaults** (`<T = string>`); `const` type params; variance annotations.
- The de Bruijn migration itself — `09`'s opening move (invariants §2); the constraint column
  must merely survive it.

## Decisions

- **Constraint = store-side column keyed by `TypeParamId`**, not part of the interned
  `TypeParamType` identity (ids are already unique per declaration; folding the constraint in
  would only churn identity and complicate the de Bruijn re-key).
- **Clamp-to-constraint inference** (matches probed tsc behavior: the failure surfaces as
  `TS2345` against the constraint, not `TS2344`).

## Run log

<!-- Append as you work. -->
