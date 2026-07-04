# OUTCOME (closed 2026-07-04) — SHIPPED

**M24 shipped.** Constraints are checked on explicit type args (`TK2344`), act as the
apparent type through EVERY structural consumer (reads, writes, element access,
call/construct signatures, union members, destructuring — via the shared
`Pass::apparent_type`), participate in inference (clamp-to-constraint → `TK2345`,
per-argument freshness exemption), and circular constraints are `TK2313` (bare-param
chains + union members, DFS at lowering). `TK2304` surfaces for unresolvable
constraint annotations.

**Commit map.** Spec `2fb720f` · plan `bb75673` · spec amendments `162a78b` (TK2304
not suppressed), `02f58a5` (TK2313 circularity), `1aa2a4f` (non-fresh clamp),
`d118a60` (composite circularity), `c54bd47` (intersection case dropped → backlog
`25`), `cb9bae8` (review F1–F4) · implementation `803e1ca` · ratchet `1a2b958`.

**Verification.** 172 unit + conformance green (m24: 4 files / 31 markers), clippy
clean. Official suite: +2 progress (`typeParameterUsedAsTypeParameterConstraint{,2}`
fp 12→0), 4 audited `IN → OOS:unresolved` (lib-global TK2304 in constraint position,
all matched=0 — coverage-only cost). Scoreboard: in-scope 495→491, clean-kept
164/209, error-exact 21/282, diag-recall 250/1651. Five fix rounds total: four from
leader verification, one from the independent adversarial review (FAIL → all four
findings fixed → re-review PASS).

**Deferred.** Contextual typing of args against the constraint (tsc TS2353 shape);
intersection types (backlog `25`, found here); rest elements (backlog `24`); `keyof`
constraints, type-param defaults (unchanged deferrals); generic-base heritage clause
as a TK2344 site (pre-existing generic-base composition deferral).

---

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

### WU1 — implemented (constraint storage + the three behaviours)

**Where the constraint lives.** A store-side column `Store::type_param_constraints:
FxHashMap<TypeParamId, TypeId>` (`src/types/store.rs`), read via
`Store::type_param_constraint(id)` and written via `Interner::set_type_param_constraint`
(`src/types/intern.rs`). NOT folded into the interned `TypeParamType` identity, so it
merely re-keys under the de Bruijn migration (invariants §2, updated).

**Lowering.** `Pass::lower_type_param_constraints` (`decls.rs`) lowers each param's
`extends` annotation **inside** the active `with_type_params` frame (so `<T, U extends T>`
resolves `T`), wired into all five param-list sites: interface fill + alias resolve
(`decls.rs`), class fill (`classes.rs`), `infer_function` + `infer_arrow` (`calls.rs`).

**Apparent type.** Member access: `Pass::apparent_type` (`expr.rs`) chases the constraint
chain (visited-set cycle guard) and `infer_member_access` looks the property up on it while
rendering `base_ty` for `TK2339` (so a missing member still names `'T'`). Relation:
`relate_uncached` (`relation.rs`) gains a `TypeParam(T) → tgt` rule that recurses on
`constraint(T)` through the ordinary `relate` (cycle stack + cache-soundness intact).

**TK2344.** `Pass::check_type_argument_constraints` (`calls.rs`) substitutes the full
arg map into each constraint (so `<T, U extends T>` checks against the actual `T` arg) then
relates arg→constraint under a transient `Relater`; wired into `instantiate_type_reference`
(`decls.rs`), `instantiate_generic_callee`, and the explicit-args branch of
`new_class_substitution` (`calls.rs`). New code `TK2344` (`diagnostics.rs`).

**Clamp.** `infer::fix_params` (`infer.rs`) now processes params in declared order, clamps a
violating candidate to the (substituted) constraint, and falls back to the constraint for an
un-inferred param.

### Deviations / nuances found during the official-suite ratchet

Three real bugs surfaced by `run --check` (all fixed):

1. **`fix_params` must preserve non-declared candidates.** The first rewrite rebuilt the map
   from only the declared `type_params`, dropping inference bindings for a param that belongs to
   a *base* class's list (a derived generic class inheriting the base constructor →
   `classWithBaseClassButNoConstructor` fp). Fixed by seeding the map from the full candidate set.
2. **~~Constraint lowering must not leak `TK2304`~~ — REVERSED by leader verification
   (spec amended, 162a78b).** The first fix snapshot+rolled-back diagnostics so an unresolvable
   constraint (`T extends Bogus`, or a lib global with no `lib.d.ts`) stayed silent. That was a
   **false negative** — the unsafe direction: tsc 6.0.3 reports TS2304 in constraint position
   exactly as in value position, and typokat's own M22 discipline already reports TK2304 for
   `const x: Bogus = 1`; constraint position being uniquely silent was inconsistent with both.
   Final behaviour: the annotation's diagnostics **surface normally** (`h<T extends Bogus>` →
   `TK2304` at the annotation, pinned in `constraint_check_explicit.ts`), and only the *recording*
   is gated — an error-type/unlowerable constraint records **no constraint** (no `TK2344` cascade,
   inference proceeds). The lib-global fp cost on the official suite is accepted and audited
   (soundness > scoreboard) — see the ratchet audit below.
3. **Apparent-type rule must precede union-target decomposition.** `K extends "x" | "y"`,
   `K → "x" | "y"` was decomposed target-first, relating `constraint(K) = "x" | "y"` to a single
   member (`"x"`) and failing → spurious `TK2344` (`circularIndexedAccessErrors`). Moved the
   `TypeParam` source rule ABOVE the union rules so the constraint relates to the whole union.

### Official-suite ratchet audit (after the TK2304 reversal; not `--save`d)

`run --check` vs the committed scoreboard: **4 accepted state changes + 2 progress, 0 other
regressions**. The 4 are all `IN → OOS:unresolved` — typokat now reports `TK2304` on a lib-global
constraint (`extends Date/String/Number`; typokat has no `lib.d.ts`), and the harness gates a file
with an unresolved name out of scope. None had any *matched* diagnostic to lose (all matched=0);
the cost is coverage (their expected diagnostics leave the in-scope denominator). Scoreboard
columns: (matched, fn, fp, expected).

| file | before | after | why |
|---|---|---|---|
| `classes/propertyMemberDeclarations/memberFunctionDeclarations/typeOfThisInMemberFunctions.ts` | IN (0, 2, 0, 2) | OOS:unresolved | `TK2304 'Date'` (constraint) |
| `types/objectTypeLiteral/callSignatures/specializedSignatureIsNotSubtypeOfNonSpecializedSignature.ts` | IN (0, 1, 0, 1) | OOS:unresolved | `TK2304 'String'` ×2 (constraints) |
| `types/objectTypeLiteral/callSignatures/specializedSignatureIsSubtypeOfNonSpecializedSignature.ts` | IN (0, 0, 0, 0) | OOS:unresolved | `TK2304 'String'` ×2 (constraints) |
| `types/typeRelationships/assignmentCompatibility/genericCallWithObjectTypeArgsAndInitializers.ts` | IN (0, 5, 0, 5) | OOS:unresolved | `TK2304 'Number'` (constraint) |
| `types/objectTypeLiteral/callSignatures/typeParameterUsedAsTypeParameterConstraint.ts` | IN (0, 0, 12, 0) | IN (0, 0, **0**, 0) | ✓ progress (constraint work) |
| `types/objectTypeLiteral/callSignatures/typeParameterUsedAsTypeParameterConstraint2.ts` | IN (0, 0, 12, 0) | IN (0, 0, **0**, 0) | ✓ progress (constraint work) |

**Clamp exemption keys on argument FRESHNESS, not the candidate's type family** (second
leader amendment, spec at `constraint_inference.ts:17-31`). The first cut exempted every
*structural* candidate — too wide: a **typed value** (`pick(ny)` with `ny: { y: number }`,
`nums(strs)` with `strs: string[]`) cannot be contextually reshaped, so tsc clamps it and the
argument check reports `TS2345`; only a **fresh object/array literal** argument gets tsc's
constraint-guided contextual retyping (the out-of-scope pass — `pick({ y: 1 })` is tsc `TS2353`,
typokat silent, documented divergence; `foo([undefined])` against `T extends [any]` →
`wideningTuples1` stays exempt). Implementation: the call sites (`infer_call`, `infer_new`)
record a per-argument syntactic freshness flag (ObjectExpression/ArrayExpression, parens
transparent — mirroring the excess-property check) in the same loop that infers the arguments
(index-aligned past skipped out-of-subset args); `infer_type_arguments` tracks which params
received a candidate from a fresh argument and `fix_params` skips the clamp for those. A param
with BOTH fresh and non-fresh candidates is exempt (the safe no-new-error direction for the
deferral) — an edge the fixtures don't pin, noted for WU2.

### TK2313 — circular constraints (third leader finding; spec amendment 02f58a5)

**The dropped-error bug.** `<T extends T>` (and the mutual `<T extends U, U extends T>`) made
the constraint of `T` a bare `TypeParam` cycle, so the relation rule's `relate(constraint, tgt)`
re-entered its own in-flight key — the assume-true stack answered Yes and `T` became assignable
to **everything** (`return t` against `number` passed silently; tsc reports TS2313 + TS2322).

**The fix (at lowering, relation engine untouched).** `lower_type_param_constraints` gained a
circularity pass: after the **whole** parameter list is lowered (a later param's constraint is
not recorded yet mid-list — this is the "second pass" option the spec offered), each param's
constraint chain is followed through **bare type-parameter constraints only**
(`constraint_chain_revisits`, `decls.rs`); structural indirection (`<T extends { self: T }>`)
ends the chain and stays legal (terminates via the relation cycle stack). A chain revisiting the
param itself reports **`TK2313`** (`Type parameter '{T}' has a circular constraint.`, new code in
`diagnostics.rs`) at the constraint annotation and the param records **no constraint**
(`Interner::remove_type_param_constraint`) — the normal paths then produce the TK2322 with no
relation-engine special-casing. Detection collects ALL circular params **before clearing any**,
so the mutual cycle flags both (clearing `T` first would hide the cycle from `U`'s walk). A chain
that dead-ends into a cycle *not through* the checked param (e.g. `<T extends U, U extends V,
V extends U>`) terminates via a visited set without flagging it — the params *on* the loop flag
themselves (probed: `U`+`V` get TK2313, `T` keeps constraint `U`, `return t` errors — sound).

**Composite residual — CLOSED (spec amendments d118a60 + c54bd47).** The self-probed gap
(`<T extends T | number>` passing silently through the union-source assume-true rule) is fixed:
`constraint_chain_revisits` is now a **DFS over one-step bare-parameter successors** — a bare
`TypeParam` constraint continues to that parameter, and a **union** constraint continues through
each of its bare-`TypeParam` members (a union branches, hence DFS + visited set instead of the
original single-successor walk; canonical unions are flat, so one member level is exhaustive).
Everything else unchanged: detect after the whole list is lowered, collect all circular params
before clearing any, TK2313 at the annotation, no constraint recorded. Pinned by the amended
`circular_constraints.ts` (`ua`/`ud` circular; `uc` — `<T extends U | number, U extends string>`
— stays legal, the over-flag regression guard). Because the successors are read off the
**lowered** column, transparent-alias cycles (`type Id<X> = X; <T extends Id<T>>`, also as a
union member) are caught for free (probed). **Intersection** composites (`<T extends T & X>`,
tsc TS2313) were briefly specced (d118a60) then dropped (c54bd47): `&` has no repr in the type
model at all, so the case is unimplementable here — → backlog `25`; the README generics bullet's
composite false-negative note was removed (no longer true) and now points at the
intersection-model gap instead.

**Ratchet after TK2313 (incl. union composites) + freshness clamp:** `run --check` state is
EXACTLY the previously audited 4 `IN → OOS:unresolved` + 2 progress (table above) — the
amendments changed nothing else on the scoreboard.

### WU2 — independent adversarial review: FAIL (4 findings), all fixed (spec cb9bae8)

The review verdict was FAIL with four reproducible false negatives vs tsc 6.0.3 (probes under
the session scratchpad `review_m24/`), now pinned in the amended `apparent_type.ts` (F1–F3) and
`constraint_inference.ts` (F4). Root cause of F1–F3 was **systemic**: `apparent_type` was a
private read-side helper consumed only by `infer_member_access`; every other structural-operand
consumer branched on the raw `TypeParam` and silently yielded the error type.

**Fix: `apparent_type` promoted to the shared Pass-level resolver** (`expr.rs`, now
`pub(in crate::check::checker)`; semantics unchanged — transitive chain, cycle-terminating,
identity for unconstrained params/non-params) and routed through every structural-operand
consumer:

- **F1 — member writes**: `check_member_assignment` (`assignment.rs`) resolves the base's
  apparent type before the property lookup (`t.x = "s"` with `T extends { x: number }` →
  `TK2322`).
- **F2 — element access**: `infer_element_access` (`expr.rs`) resolves the base after the
  any/error skip (`t["x"]`, `t[0]` with `T extends number[]` read through the constraint).
- **F3 — calls (+ `new`)**: `callable_signature` and `construct_signature` (`calls.rs`) resolve
  the callee first (`t("s")` with `T extends (a: number) => number` → `TK2345`; the return type
  flows → `TK2322`).
- **F4 — per-argument freshness**: `infer_type_arguments` now tracks fresh AND non-fresh
  candidate provenance per parameter; the clamp exemption holds only when **every** candidate
  came from a fresh literal (`one({x: 1}, ny)` / `one(ny, {x: 1})` → `TK2345` on the violating
  non-fresh argument; all-fresh `pick({y: 1})` stays silent).

**Consumer audit** (everything that branches on an operand type's `TypeTag` / side-table):
- Routed in addition to F1–F3 (same finding class, found by the audit):
  `union_member_access` (`expr.rs` — union **members** that are constrained params; was a
  false-positive `TK2339`), `union_member_assignment_target` (`assignment.rs` — the write-side
  union fallback), and `pattern_member_access` (`classes.rs` — destructuring access control,
  source + union constituents).
- Audited, NOT routed (each is a documented pre-existing deferral, not an M24 gap):
  element-access **writes** (`t["x"] = …`) and **compound assignment** (`t.x += …`) — assignment
  targets of those shapes are unchecked baseline-wide since M14, for every base type;
  `check_excess_properties` + `infer_initializer`'s tuple-context check — contextual typing
  against a constraint is the explicit out-of-scope deferral; `keyof_type` /
  `indexed_access_type` (`annotations.rs`) — type-position operators with the documented
  generic/VM-phase deferral; flow-narrowing member lookups (`flow.rs`) — a non-object member is
  conservatively kept (sound; narrowing must not act on apparent types without its own probe);
  `override_failure_reason` (`statements.rs`) — a bare-`T` member falls through to the plain
  relation query, which already carries the apparent-type rule; `infer_new`'s class path — the
  callee resolves by declaration, not by operand type.

**Ratchet after F1–F4:** `run --check` state is EXACTLY the accepted 4 `IN → OOS:unresolved`
+ 2 progress — the review fixes produced zero additional state changes (no new matched, no new
fp on any in-scope file).
