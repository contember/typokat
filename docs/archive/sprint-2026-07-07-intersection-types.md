# OUTCOME (closed 2026-07-07) — SHIPPED

**M31 intersection types (`A & B`) shipped.** An interned, canonicalized member-set
node (the structural dual of union: `never` absorbs, `X & unknown → X`, `any`/error
absorb, flatten/sort/dedup, empty → `unknown`, single → bare). Relation is
dual-directional — a **target** intersection requires every member; a **source**
intersection relates through its **merged apparent object** (authoritative for object
targets; single-member sufficiency only for non-object targets, since `A <: B` does
not imply `A & C <: B` when the target penalizes a present property). Merged member
access, merged-key excess, contextual fresh-literal shaping, and the M24
circular-constraint walk (`T extends T & X` → `TK2313`) all see the merge; merged-source
recursion terminates via the relation cycle stack (single-contributor) and a coinductive
assume-true guard (multi-contributor).

**Commit map.** Spec/plan `ce3e138` · implementation `f99c0f2` · official-suite
scoreboard ratchet `40c6071`.

**Verification.** `cargo test` (206 unit tests + conformance; corpus grew to 169 files /
558 markers, `m31_intersections/` = 7 fixtures), `cargo clippy --all-targets -- -D
warnings` clean, `tsc 6.0.3 --strict --noEmit` over every M31 fixture (expected errors
only), and official-suite `run --bin ../../target/debug/typokat --check` → 0 regressions
(deliberate ratchet: 10 moved files + 2 progress, all safe-direction — no `matched`
drop, no false-negative rise).

**Review.** Round 1 **FAIL** — a source-merge soundness hole (dropped errors on
optional/index-signature targets: `A <: B` ⇏ `A & C <: B`), a nested-excess over-report
(first-contributor only), and a contextual-literal over-report (intersection targets not
peeled). Fixes landed. Round 2 **FAIL** — the source-merge rewrite recursed unguarded and
**stack-overflowed** on recursive intersection targets (aborting the whole file). Fixed
with cycle-guarded single-contributor delegation + a coinductive assume-true guard for the
multi-contributor merge. Round 3 (focused, soundness-critical) **PASS** — verified the
guard cannot poison the durable cache (8 random permutations → identical diagnostic sets),
termination on the adversarial recursion battery, no buried mismatch dropped, and
order-independence. Two review-driven regression fixtures added
(`source_intersection_targets.ts`, `recursive_targets.ts`).

**Deferred (documented safe-direction divergences).** Disjoint-primitive reduction
(`string & number → never`) is not done — the per-member relation gives the same verdict
(fixtures assert code-only); `&` is not distributed over unions (`(A | B) & C`);
`keyof` / indexed-access over an intersection stay out of subset; an index-signature
target of a source intersection is conservatively rejected; a nested optional target
property contributed by a single member is checked more strictly than tsc; M25's
contravariant `infer` still unions rather than intersects; overload-signature
intersection is out of scope (backlog `40`). The pre-existing finite deep-nesting
native stack overflow (non-cyclic, no intersection involved) is backlog `63(k)`.

---

# Sprint — intersection types `A & B` (2026-07-07)

**Goal.** Ship backlog `25`: add intersection types to the type model — an interned,
canonicalized intersection node, dual relation directions, merged member access +
excess checking, `&`-annotation lowering, and the M24 circular-constraint walk
extension. Land as **M31**.

**Theme.** Kill a silently-permissive false-negative family and take the first step of
track A (the `lib.d.ts` critical path). Today `type AB = { a: number } & { b: string };
const bad: AB = { a: 1 }` passes silently — every `&` annotation lowers to something
permissive, so all downstream diagnostics are suppressed. Intersection is the structural
dual of `Union`: a member-set node with inverted canonicalization and inverted (AND/OR)
relation directions, plus three genuinely-different pieces the review must hunt.

## Refs re-verified at HEAD (2026-07-07, `ae0e19a`)

From the Union end-to-end map — the pieces intersection mirrors:

- ✔ **No `Intersection` tag / column exists** — `TypeTag` enum (`src/types/repr.rs:15-102`)
  has `Union` (`:24`) but no intersection; `Store::unions: Vec<Box<[TypeId]>>`
  (`src/types/store.rs:50`) with `union_members` (`:295`) and `push_union` (`:365`) —
  add the parallel `intersections` column + `intersection_members` + `push_intersection`.
- ✔ **`Interner::union` is the canonicalization template** (`src/types/intern.rs:388-442`):
  flatten nested (`:396-401`), `any`/`error` absorb (`:405`), `unknown` absorbs (`:408`),
  drop `never` (`:413`), sort+dedup (`:417`), collapse 0→`never` / 1→bare (`:421-424`),
  hash-cons (`:430-441`). Intersection **inverts absorption**: `never` absorbs → `never`;
  drop `unknown` (`X & unknown → X`); collapse 0→`unknown` / 1→bare. `any`/`error` still
  absorb to `any`. **Disjoint-primitive reduction (`string & number → never`) is NOT
  done** (see Decisions).
- ✔ **`StructuralKey::Union(&[TypeId])`** (`src/types/hash.rs:60`) + exhaustive
  `structural_hash` arm (`:214-224`) hashing the discriminant then members — add
  `StructuralKey::Intersection` + arm hashing `TypeTag::Intersection.hash_discriminant`
  so a union and an intersection over the same set never collide.
- ✔ **`Substitution::apply` exhaustive match** (`src/types/substitute.rs:67-95`), union arm
  `:82` → `apply_union` (`:233-259`) re-interns via `interner.union` only when a member
  changed (`:254-255`) — mirror as `apply_intersection` → `interner.intersection`.
- ✔ **Relation dispatch** (`src/relate/relation.rs:438-443`): two sequential `if`s in
  `relate_uncached` — source-union → `relate_union_source` (`:1083`, **every** member
  relates, AND), target-union → `relate_union_target` (`:1118`, **some** member, OR).
  Ordering documented `:362-386` (after any-short-circuit, TypeParam-constraint, deferred
  guards; before intrinsic/object rules). Intersection arms slot in here with the
  **inverted** mapping — target-intersection = "every member" (mirror of union *source*),
  source-intersection = merged-object / some-member (see WU3).
- ✔ **Member access over union** (`src/check/checker/expr.rs:685` → `union_member_access`
  `:767-808`): property required on **every** member, result `interner.union(types)`,
  missing → single `TK2339` + `wk.error`. Intersection needs the **merged apparent object**
  (property in **at least one** member) — genuinely different logic, not a copy.
- ✔ **`|` annotation lowering** (`src/check/checker/annotations.rs:63`) →
  `lower_union_annotation` (`:509-515`); the parallel resolvability helper
  `annotation_type_refs_are_locally_resolvable` has a `TSUnionType` arm (`:891`, `_ =>
  false` at `:914`). Add `TSIntersectionType` arms to both. (Grep confirms **zero**
  existing `TSIntersection` handling in `src/`.)
- ✔ **Rendering** (`src/diagnostics.rs:863-873`): exhaustive `render_type_inner` match,
  union joins `" | "`; `array_element_needs_parens` (`:1105`) parenthesizes union/function
  elements — add intersection arm (join `" & "`; parenthesize in array elements and inside
  union renders).
- ✔ **M24 circularity DFS** `constraint_chain_revisits` (`src/check/checker/decls.rs:894-921`):
  already walks bare-parameter **union** members (`:906-910`); add a parallel
  `store.intersection_members(constraint)` branch (one block at `:906-911`) so
  `<T extends T & X>` → `TK2313`.
- ✔ **Other exhaustive `TypeTag` walks** that a new variant forces an arm on:
  `contains_infer` (`relation.rs:1201`), `eval.rs` `operand_undecidable`/`replace_mapped_value`/
  `substitute_infers`/`child_types`/`contains_nodes` (`:472/:1461/:1735/:1941/:2092`),
  `flow.rs:453`. Each descends into / re-interns members — mirror conservatively.

## Work units

### WU1 — Disabled M31 corpus (effort S) — DONE (this spec commit)

- **Scope.** `tests/cases/m31_intersections/` with five fixtures
  (`object_intersection`, `canonicalization`, `relation_directions`, `excess`,
  `circular_constraint`), registered `false` in `MILESTONE_DIRS`, milestone-index row in
  `tests/cases/README.md`. All expectations cross-checked vs `tsc 6.0.3 --strict`.
- **Acceptance.** `cargo test` behavior-neutral while disabled; enabling at HEAD fails
  (every `&` currently silent).

### WU2 — Intersection node: store, canonicalization, lowering, render, substitution (effort M)

- **Problem.** No `Intersection` type node exists; `&` annotations lower to a permissive
  fallback.
- **Verify first.** At HEAD, `cargo run -- check` on any `&` annotation reports nothing.
- **Scope.** The mechanical `Union` mirror with inverted canonicalization: `intersections`
  store column + `intersection_members` + `push_intersection`; `Interner::intersection`
  (flatten, `any`/`error`→`any`, `never` absorbs, drop `unknown`, sort+dedup, 0→`unknown`,
  1→bare, hash-cons); `StructuralKey::Intersection` + hash arm; `apply_intersection`;
  `TSIntersectionType` lowering in `lower_annotation` + `annotation_type_refs_are_locally_resolvable`;
  `render_type_inner` `" & "` join + parenthesization; and the exhaustive-walk arms in
  `contains_infer`, the `eval.rs` traversals, and `flow.rs`. Do **not** call
  `with_indirection` in the lowering (match `lower_union_annotation`; see
  `context.rs:562`).
- **Acceptance / witness.** `canonicalization.ts` passes (collapse/flatten/dedup); a unit
  test cloned from `intern.rs:967-1053` exercises intersection canonicalization + hash-cons;
  a render unit test (cloned from the union render tests) prints `" & "`.
- **Touch points.** `src/types/{repr.rs,store.rs,intern.rs,hash.rs,substitute.rs}`,
  `src/check/checker/annotations.rs`, `src/diagnostics.rs`, `src/relate/relation.rs`
  (`contains_infer`), `src/check/checker/eval.rs`, `src/check/flow.rs`.

### WU3 — Relation directions, merged member access, merged excess (effort L)

- **Problem.** The three genuinely-different pieces; the soundness core.
- **Verify first.** Probe each against `tsc 6.0.3 --strict` (the spec fixtures are the pins).
- **Scope.**
  1. **Target intersection** `S <: A & B` — assignable to **every** member (mirror
     `relate_union_source`: return on first failing member with a member-scoped reason).
  2. **Source intersection** `A & B <: T` — assignable if **some** member `<: T`, OR the
     **merged apparent object** of the intersection `<: T` (both are sound acceptances;
     `toMerge`/`spanTwo` require the merge path). Build the merged apparent object by
     unioning all object members' property sets; a property in several members takes the
     **intersection** of its types (representable now).
  3. **Member access** `x.prop` on `A & B` (`expr.rs:685`) — resolve against the merged
     apparent object: present in ≥1 member → its (intersected) type; in none → `TK2339`.
     Mirror the write side (`assignment.rs:288`) and access-control side (`classes.rs:1092`)
     enough for the corpus.
  4. **Excess against an intersection target** — a fresh object literal's allowed keys are
     the **merged** key set (union of all members' keys + index sigs). **Suppress
     per-member excess** on the individual member relations and run **one** merged excess
     check, else a member-covered key (`a` against member `{b:string}`) is spuriously
     flagged. `excess.ts` `ok` line is the regression net.
- **Acceptance / witness.** `object_intersection.ts`, `relation_directions.ts`, `excess.ts`
  all pass; the `ok`/`toMerge`/`spanTwo`/clean lines stay clean (no spurious excess / no
  over-report from a missing merge path).
- **Touch points.** `src/relate/relation.rs` (dispatch `:438` + two new arms + merged-object
  helper), `src/check/checker/expr.rs`, `src/check/checker/assignment.rs`,
  `src/check/checker/classes.rs`.

### WU4 — M24 circularity, independent review, docs + ratchet (effort M)

- **Problem.** `<T extends T & X>` circularity is unwalked; intersection can silently
  over/under-report in ways the impl's own tests miss.
- **Scope.** Extend `constraint_chain_revisits` (`decls.rs:906`) with an
  intersection-member branch (`circular_constraint.ts` → `TK2313`). Then **independent
  adversarial review** (hunt false negatives: the merged-excess suppression, the source
  merge path, canonicalization edge cases, any dropped error; cross-check vs
  `tsc 6.0.3 --strict`). On PASS: enable `m31_intersections` in `MILESTONE_DIRS`, update
  `tests/cases/README.md` prose + the stale "intersection deferred" cross-refs (generics /
  M25 bullets), `README.md` coverage, `docs/INDEX.md`; run the official-suite
  `run --check` (audit any scoreboard movement — intersection may flip some previously
  silent files to errors; ratchet deliberately).
- **Acceptance / witness.** `cargo test` (all M31 + no regression), `cargo clippy
  --all-targets -- -D warnings`, official-suite `run --check` green (or a deliberate,
  audited ratchet).
- **Touch points.** `src/check/checker/decls.rs`, `tests/conformance.rs`,
  `tests/cases/README.md`, `README.md`, `docs/INDEX.md`, `tooling/official-suite/`, backlog `25`.

## Out of scope (explicit — documented divergences)

- **Disjoint-primitive reduction** (`string & number → never`, `1 & 2 → never`): NOT done
  at canonicalization. The per-member target rule yields the **same verdict** (both members
  can't be satisfied → error), only a different message; `relation_directions.ts` asserts
  those lines **code-only**. Source-position (`string & number <: T`) becomes more
  restrictive than tsc's `never <: T` — over-report, safe.
- **`&` distribution over `|`** (`(A | B) & C = A & C | B & C`): NOT distributed; kept as a
  structural node. Over-report/divergence, documented.
- **`keyof` / indexed-access over an intersection** (`keyof (A & B)`, `(A & B)[K]`) and the
  dual `keyof (A | B) = keyof A & keyof B`: stay out-of-subset (the existing M20/M28
  keyof-of-non-object deferral) — documented, deferred.
- **Function / call-signature intersection** (overload intersection): out of scope (no
  overloads yet — backlog `40`).
- **Optional-method / accessor merging, `exactOptionalPropertyTypes`**: unchanged from M21.

## Decisions

- **Member-set node, no `IntersectionType` struct** — mirror Union's bare `Box<[TypeId]>`
  column (Union has no payload struct either).
- **Keep intersection unreduced for disjoint primitives** — the per-member relation gives
  the correct verdict; eager `never`-reduction is extra machinery with no verdict benefit
  in the corpus (deferred, documented).
- **Source-intersection = some-member OR merged-object** — both acceptances are sound (a
  value of `A & B` structurally satisfies both); the OR is the most permissive-yet-sound
  rule and is required by `toMerge`/`spanTwo`.
- **Merged excess, suppressed per-member excess** — the only correct way to excess-check a
  fresh literal against an intersection target (tsc uses the combined property set).
- **House-style codes** (probed at HEAD): missing property → **TK2741** (both declaration
  and argument position — typokat reports missing uniformly, where tsc wraps in
  TS2322/TS2345); conflicting/disjoint members → **TK2322** (per-member); circular →
  **TK2313**.

## Sequencing

1. Commit the disabled M31 corpus + this sprint plan (WU1).
2. Dispatch one implementation subagent for WU2 → WU3 → WU4-impl (node → relation →
   circularity), leaving `cargo build`/`clippy`/`cargo test` green and flipping
   `m31_intersections` `true`; it must NOT commit.
3. Local verification, then dispatch the independent adversarial review subagent.
4. Fix review findings through the implementation agent; re-review the relation-engine
   changes before committing.
5. Leader commits implementation, then updates docs + official-suite ratchet, closes and
   archives the sprint, deletes backlog `25`.

## Run log

<!-- Append as you work. -->
