<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/:

> **OUTCOME — shipped YYYY-MM-DD.** <one-paragraph result.> Commit map: WU1 → <sha>,
> WU2 → <sha>, … Verification: <the gate command + numbers>. Backlog closed:
> <ids deleted/rescoped>. Deferred: <honest notes>.
-->

# Sprint — class completeness + constructor accessibility (2026-07-04)

**Goal.** Ship backlog [`06`](../backlog/06-class-completeness-checks.md) (override compatibility
`TK2416` + abstract-member completeness `TK2515`/`TK2654`) and
[`20`](../backlog/20-constructor-accessibility-on-new.md) (private/protected constructor
accessibility on direct `new C()` — `TK2673`/`TK2674`).

**Theme.** Both items close known **false negatives** in the class-checking path — the worst
outcome for a checker — without opening the narrowing CFG (`07`) or the type-level phase. They
share touch points (`classes.rs` lowering, `ClassInfo`, the direct-`new` path) so batching them
avoids re-reading the same code twice. Success: the hand-written corpora prove all five codes
against `tsc 6.0.3 --strict` behavior, the official suite shows progress on the
`inheritanceAndOverriding` / accessibility rows with zero regressions, and all existing gates stay
green.

## Refs re-verified at HEAD (2026-07-04, f47b56b)

Re-read the load-bearing facts in the actual code before planning on them.
`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ **Both 06 checks are explicitly deferred today** — `src/check/checker/classes.rs:183-185`
  lists method-**override compatibility** (`TK2416`) and the abstract-member-not-implemented
  completeness check (`TK2515`) in `fill_class`'s DEFERRED block.
- ✔ **Override composition does no compatibility check** — `src/check/checker/classes.rs:146-150`:
  the instance type composes base members with the class's own, the own member **overriding** the
  base's on a name conflict (`compose_members`); nothing relates the two types.
- ✔ **Abstract members are indistinguishable after lowering** — `src/check/checker/classes.rs:158-164`:
  an `abstract method(): T;` is built as an ordinary function-typed property; only the **class's**
  `abstract` keyword is recorded (`ClassInfo::is_abstract`) and enforced at `new` (`TK2511`).
  Per-member abstractness is not tracked anywhere, so `TK2515` needs new bookkeeping.
- ✔ **Constructor visibility is read but not carried** — `src/check/checker/classes.rs:285-298`:
  `has_public_constructor(class)` (AST-level) gates the static-side construct signature (F1/WU3
  fix), but the visibility never reaches `ClassInfo`.
- ✔ **`ClassInfo` has no ctor-visibility field** — `src/check/checker/context.rs:170` (`Copy`
  struct: `ctor`, `instance`, `static_side`, `class_id`, `super_ctor`, `is_abstract`).
- ✔ **Direct `new C()` checks no accessibility** — `src/check/checker/calls.rs:372-381`
  (`infer_new` resolves the callee through `class_ctors` and goes straight to arity/args);
  `TK2511` shows the shape an accessibility gate takes (named class's own flag, `new_span`).
- ✔ **`private`/`protected` member-access context machinery exists** (M13) — the checker already
  knows the enclosing class and the subclass walk (`class_parents`) for `TK2341`/`TK2445`; the
  ctor-accessibility check reuses that notion of "inside the declaring class / inside a subclass".
- ✔ **`DiagnosticCode` is a closed enum with constructor helpers** — `src/diagnostics.rs:20`
  (enum), e.g. the `TK2511` helper at `src/diagnostics.rs:158-165`. New codes: `TK2416`,
  `TK2515`, `TK2654`, `TK2673`, `TK2674`.
- ✔ **Official-suite rows this should move** — `tooling/official-suite/scoreboard.txt`:
  `derivedClassOverridesWithoutSubtype.ts` (0 matched / 2 fn), `derivedClassFunctionOverridesBaseClassAccessor.ts`
  (0/2), `derivedClassOverridesPrivates.ts` (0/4), `derivedClassOverridesProtectedMembers4.ts` (0/4),
  `derivedClassOverridesPublicMembers.ts` (0/12) and the `classPropertyAsPrivate/Protected` rows
  (partial fn) sit in scope of these codes.

### tsc 6.0.3 probe results (the spec's ground truth)

Probed with `tsc --strict --noEmit` (scratchpad probes, 2026-07-04):

- **`TS2416`** fires on the **derived member name** for methods, function-typed properties, plain
  properties, and accessor overrides, with a nested reason chain
  (`Property 'm' in type 'Derived' is not assignable to the same property in base type 'Base'.`).
  It also fires when an abstract member is implemented **incompatibly**.
- **Method bivariance**: a method-declaration override narrowing a param (`m(x: string | number)` →
  `m(x: string)`) is **accepted** by tsc (bivariant method params) but the same shape as a
  function-typed **property** is **rejected** (strict contravariance). typokat lowers methods as
  function-typed properties and relates contravariantly (architecture §6.5) → it will
  **over-report** the bivariant-only method case (safe direction, documented divergence).
- **`TS2515` vs `TS2654`**: exactly **one** missing abstract member → `TS2515`
  (`Non-abstract class 'C' does not implement inherited abstract member go from class 'Mid'.` —
  member name **unquoted**, attributed to the **direct base**, not the declaring class); **two or
  more** → one aggregated `TS2654`
  (`Non-abstract class 'Impl' is missing implementations for the following members of 'AB': 'go', 'val'.`
  — names quoted, declaration order). Both fire on the **class name** span. A member implemented in
  an abstract intermediate class satisfies the requirement; abstract **accessors** and abstract
  **fields** count as members; implementing an abstract method via an initialized field is accepted.
- **`TS2673`/`TS2674`** fire on the **whole `new` expression**
  (`Constructor of class 'C' is private and only accessible within the class declaration.` /
  `… is protected and only accessible within the class declaration.`). `new` inside the declaring
  class (incl. `static` members) is fine for `private`; inside a subclass is fine for `protected`
  but **not** for `private`.
- **Adjacent codes deliberately out of scope** (see below): `TS2415` (visibility-narrowing /
  incorrectly-extends), `TS2417` (static-side incompat), `TS2426` (accessor-vs-function kind
  mismatch), `TS2675` (extending a private-ctor class).

## Work units

Each WU follows [`../reference/dev-method.md`](../reference/dev-method.md) §1: leader writes the
fixture spec first (new dir, registered `false`, committed on its own) → implementation flips it
`true` and leaves gates green → independent adversarial review hunts false negatives and
cross-checks `tsc --strict` → fix → leader verifies and commits.

### WU1 — Class-completeness checks: `TK2416` + `TK2515`/`TK2654` (effort M)

- **Problem.** An incompatible override is silently accepted (`classes.rs:146-150`), and a
  non-abstract subclass may leave inherited abstract members unimplemented (`classes.rs:158-164`)
  — both false negatives.
- **Verify first.** Run the probe fixtures through `cargo run -- check` at HEAD to confirm zero
  diagnostics today; confirm `compose_members` is the single composition point.
- **Scope.** (1) After composing, for every **own instance member** that overrides a base member of
  the same name, require own-type → base-type assignability via the relation engine (reporting
  mode); failure = `TK2416` on the derived member name with the tsc message shape + reason chain.
  (2) Track per-class **own abstract member names** (methods, fields, accessors) in declaration
  order; compose a **pending set** down the chain (base's pending − own concrete members + own
  abstract members); a **non-abstract** class with a non-empty pending set reports `TK2515` (one
  member, unquoted name, attributed to the **direct base**) or `TK2654` (≥2, quoted names,
  declaration order) on the class name. Both checks skip out-of-subset shapes (generic bases,
  unresolvable members) — existing deferrals unchanged.
- **Acceptance / witness.** New corpus dir `tests/cases/b06_class_completeness/` (registered
  `false` in the spec commit): incompatible method/property/accessor override (`TK2416`, incl. the
  incompatibly-implemented-abstract case), compatible overrides stay clean, single missing abstract
  member (`TK2515`, direct-base attribution through an abstract mid), multiple missing (`TK2654`),
  implemented-in-abstract-mid stays clean, abstract field + accessor count. No regression in
  `m11`–`m16` class corpora.
- **Touch points.** `src/check/checker/classes.rs` (fill/compose path), a per-class abstract-member
  side table (`context.rs`), `src/diagnostics.rs` (3 new codes + helpers), `tests/conformance.rs`,
  `tests/cases/README.md`.

### WU2 — Constructor accessibility on direct `new C()`: `TK2673`/`TK2674` (effort S)

- **Problem.** `new PrivCtor()` / `new ProtCtor()` outside the permitted context is accepted
  (`calls.rs:372-381` never looks at ctor visibility) — a false negative filed from the F1/WU3
  review.
- **Verify first.** Confirm at HEAD that the probe cases produce no diagnostics; confirm the M13
  enclosing-class/subclass context is queryable at the `infer_new` site.
- **Scope.** Carry the constructor's visibility on `ClassInfo` (from the same AST fact
  `has_public_constructor` reads today, kept `Copy`); in `infer_new`'s direct-class path, gate on
  it: `private` → allowed only lexically inside the declaring class; `protected` → declaring class
  or a subclass (the M13 `class_parents` walk); violation = `TK2673`/`TK2674` on the `new`
  expression span, message per tsc. A derived class inheriting a private/protected constructor
  keeps the base's visibility for this check (tsc behavior). Existing public-ctor behavior and the
  F1/WU3 static-side gating stay unchanged.
- **Acceptance / witness.** New corpus dir `tests/cases/b20_ctor_accessibility/` (registered
  `false` in the spec commit): outside-`new` on private (`TK2673`) and protected (`TK2674`),
  allowed contexts stay clean (inside declaring class incl. statics; subclass for `protected`),
  private-in-subclass rejected, inherited-ctor visibility respected. No regression in `m11`–`m16`
  or `f1_object_interface_construct`.
- **Touch points.** `src/check/checker/context.rs` (`ClassInfo`), `src/check/checker/classes.rs`
  (fill), `src/check/checker/calls.rs` (`infer_new`), `src/diagnostics.rs` (2 new codes),
  `tests/conformance.rs`, `tests/cases/README.md`.

### WU3 — Adversarial review + official-suite ratchet (effort S/M)

- **Problem.** Both WUs add *new* diagnostics next to existing relation/composition machinery; the
  risk is a dropped error (wrong pending-set composition, a visibility gate that a context sneaks
  past) or an over-report storm on the official suite.
- **Verify first.** Targeted official-suite rows before/after each WU.
- **Scope.** Independent reviewer per WU: replays fixtures against `tsc --strict`, hunts false
  negatives (deep chains, mixed abstract/concrete hierarchies, statics, generic-adjacent shapes),
  checks no order-dependence (relation cache), and classifies every official-suite delta. Ratchet
  (`tsofficial.py run --check`, then `--save`) after both WUs land.
- **Acceptance / witness.** `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release`, `tsofficial.py run --check` green; scoreboard delta committed with the
  ratchet.
- **Touch points.** `tooling/official-suite/scoreboard.txt`, `tests/cases/README.md` (divergences).

## Out of scope (explicit)

- **`TS2415`** (class incorrectly extends — visibility narrowing, private-member redeclaration) and
  **`TS2417`** (static-side override incompat) — separate checks over the static side / modifier
  lattice; file later if the suite shows value.
- **`TS2426`** (accessor-vs-function kind mismatch) — typokat's `TK2416` still catches the type
  incompatibility in the probed shapes; the kind-specific second diagnostic is deferred.
- **`TS2675`** (extending a class with a private constructor) — heritage-clause check, not the
  direct-`new` path; stays a known false negative, noted in `20`'s close-out.
- **Method-declaration bivariance** — typokat relates contravariantly (architecture §6.5); the
  bivariant-only method-override shape is a **documented over-report** (safe direction), recorded
  in `tests/cases/README.md`; fixtures avoid asserting it either way.
- **Generic base classes** (`class S extends Box<string>`) — composition against the unsubstituted
  template is an existing documented deferral; the new checks must not crash on it but need not be
  precise there.

## Decisions

- **Mirror tsc 6.0.3's `TS2515`/`TS2654` split exactly** (single member → `TK2515` with
  direct-base attribution and unquoted name; ≥2 → aggregated `TK2654` with quoted, declaration-order
  names). Verified by probe, not by the (older) backlog text, which predates the aggregated code.
- **Corpus dirs named by backlog id** (`b06_…`, `b20_…`) — extends the `fN_…` bug-fix-corpus
  convention to roadmap items that fix already-shipped milestones; documented in
  `tests/cases/README.md`.
- **Both new checks run at class-fill time** (composition site), not per-`new`-site — matches
  where the information lives and keeps `infer_new` changes minimal (WU2 only adds the
  accessibility gate).

## Sequencing

| WU | Touches | Parallel? |
|---|---|---|
| WU1 | `classes.rs` compose path + diagnostics | first |
| WU2 | `ClassInfo` + `infer_new` + diagnostics | after WU1 commits (same files) |
| WU3 | review + ratchet | review after each WU; ratchet at the end |

Specs for both WUs can be written and committed up front (both dirs registered `false`).
Implementations serialize — same files.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->
