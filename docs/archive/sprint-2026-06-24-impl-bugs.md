> **OUTCOME — shipped 2026-06-25.** Three of the four planned current-impl bugs fixed (F3–F5 /
> backlog 01–03); **WU4 / item 04 dropped as a non-bug** (below). Each WU ran the full
> spec → impl → independent adversarial review → fix → commit loop; every review hunted false
> negatives and cross-checked tsc 6.0.3 — and two of the three caught a real defect before commit.
>
> **Commit map:**
> - sprint plan → `46c7175`
> - WU1 (F3 class member collection): spec `4f2a839` → impl `96bb85d` (review caught a `this`-init
>   `any` false-negative **and** an obligations-channel double-report; both fixed before commit)
> - WU2 (F4 destructuring access): spec `1b78df3` → impl `5c3d106` (review PASS)
> - WU3 (F5 object+union readonly): spec `7017ce5` → impl `3886a8b` (review PASS; hash-identity
>   change cleared of false positives)
> - this OUTCOME + backlog cleanup → (this commit)
>
> **Verification:** `cargo test` 172 unit + conformance green; `cargo clippy --all-targets -D warnings`
> clean; official-suite `run --check` **0 regressions** on every WU, scoreboard re-saved per WU.
> Net: clean-kept **152→163**, diag-recall **185→197**, error-exact 15→16. All fixture expectations
> cross-checked against tsc 6.0.3.
>
> **Backlog closed:** 01, 02, 03 deleted (shipped). **04 deleted as a non-bug** — its premise was
> wrong: tsc 6.0.3 never emits both TK2540 and TK2322 for one readonly assignment (outside a ctor,
> readonly blocks → TK2540 only; inside the declaring ctor, readonly is allowed → TK2322 only), and
> typokat already matches this exactly (`assignment.rs` readonly gate). Implementing the requested
> "cascade" would have *introduced* a divergence; spec-first + tsc-cross-check caught it before code.
>
> **Discovered (WU3):** item 03's bug was broader than "union" — object-type & interface members
> dropped `readonly` entirely (`lower_object_annotation` / `lower_interface_members`), and the headline
> repro canonicalizes to that plain-object case. Fixed both the object-readonly and genuine-union paths.
>
> **Deferred (safe-direction false negatives, documented in each review):** nested/array/
> assignment-target destructuring + `for-of`/`catch` (WU2); property-missing-on-some-union-member,
> intersection bases, element-access targets, accessor signatures in type literals (WU3).

# Sprint — current-impl bugs F3–F6 (2026-06-24)

**Goal.** Fix the four current-implementation bugs the official-suite harness surfaced
(findings F3–F6 / backlog [`01`](../backlog/01-class-member-collection.md)–[`04`](../backlog/04-readonly-cascade-precision.md)):
class member-collection over-reports `TK2339`, and three smaller soundness/precision gaps in
member-assignment and destructuring under-report.

**Theme.** All four are bugs in **already-shipped** milestones (M11–M16), not new features — each is a
contained fix with a fixture witness, biased toward restoring soundness. Success condition for the
batch: each WU's new fixture dir passes, the existing conformance corpus and the official-suite
regression scoreboard show **no regression**, and `clippy` stays clean. WU1 is the headline (it kills
~150 spurious `TK2339`); WU2–WU4 close false-negatives/precision.

## Refs re-verified at HEAD (2026-06-24)

Re-read the load-bearing facts in the actual code before planning on them.
`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ **Un-annotated class fields are dropped** — `src/check/checker/classes.rs:357-359`
  (`let Some(annotation) = prop.type_annotation … else { continue }`): a field with an initializer but
  no annotation creates **no member**. The initializer is still walked for nested-function checking
  (no member contributed).
- ✔ **Constructor parameter properties create no members** — `classes.rs:403-410`: a `Constructor`
  only lowers `method.value.params` into the ctor *signature* (`lower_signature_parameters`); the
  `public/private/protected/readonly` modifier on a param produces no class member.
- ✔ **`TK2339` over-report site** — `src/check/checker/expr.rs:529-533` (object base) and `571-576`
  (union base): emitted after a failed property lookup. This is what fires for a dropped member.
- ✔ **Member-assignment skips non-object (incl. union) bases** —
  `src/check/checker/assignment.rs:177-194`: property looked up only via `object_type(base_ty)`;
  a union base has no `object_type` → `found == None` → silent `return` at 192-194. No `TK2540`, no
  `TK2322`.
- ✔ **readonly assignment early-returns, suppressing the value-type cascade** —
  `assignment.rs:209-225`: emits `TK2540` then `return` at line 224, **before** the type-assignability
  obligation that would push `TK2322` at `assignment.rs:230-242`. The inline comment (221-223) is the
  deliberate-but-wrong short-circuit item 04 targets.
- ✔ **Access-control check** lives in `check_member_access_control()`
  `src/check/checker/classes.rs:765-797` (`TK2341` private 774-783, `TK2445` protected 785-795),
  called from `expr.rs:513-517` on a successful member read. **Destructuring never reaches it** —
  `parameter_name()` returns `None` for any non-identifier pattern (`calls.rs:694-700`) and
  `binding_decl_id()` likewise (`assignment.rs:388-395`); destructured names are currently unbound.
- ✔ **Fixture dirs are toggled in `MILESTONE_DIRS`** — `tests/conformance.rs:27-51` (`&[(&str, bool)]`,
  only `true` rows run, loop at `:69`). A new dir registered `false` runs nothing until flipped →
  **preserves the dev-method's "spec commit is behavior-neutral" property** (see Decisions).

## Work units

Each WU follows the per-item build loop in [`../reference/dev-method.md`](../reference/dev-method.md) §1:
**leader writes the fixture spec first** (new dir, registered `false`, committed on its own) →
implementation subagent (flips the dir `true`, leaves build/clippy/test green, no commit) →
**independent adversarial review** subagent (hunts false-negatives, cross-checks `tsc --strict`) →
fix → leader verifies and commits.

### WU1 — Class member-collection: parameter properties + initializer-inferred fields (effort L)

- **Problem.** `classes.rs:357-359` drops every un-annotated field, and `classes.rs:403-410` turns a
  constructor's parameter-property modifiers into nothing. So `constructor(public x: number)` and
  `f = () => 1` / `g = 2` yield no member, and every access over-reports `TK2339` at
  `expr.rs:529-533`. ~150 spurious `TK2339` across ~38 official-suite files; the hand-written class
  corpus missed it because its fixtures all annotate fields.
- **Verify first.** Confirm the two skip sites above still match HEAD. Probe with `tsc --strict` the
  exact inferred types: a non-readonly initialized field **widens** (`g = 2` → `number`, not `2`); a
  parameter property's member inherits the param's annotation type + its accessibility + `readonly`.
  Confirm an un-modified ctor param stays a plain parameter (no member).
- **Scope (priority order).** (1) Parameter properties: when a ctor param carries
  `public/private/protected/readonly`, emit an instance member (name, annotation type, visibility,
  `readonly`) **in addition to** keeping the ctor signature. (2) Initializer-inferred fields: an
  un-annotated `PropertyDefinition` with an initializer infers its member type from the initializer
  (with field-init widening). (3) Ensure the **existing** access-control + type checks then run on
  these members, so `c.y` becomes `TK2341` (private), not `TK2339`.
- **Acceptance / witness.** New dir `tests/cases/f3_class_member_collection/` exercising both forms,
  including the backlog-01 repro: `c.x` clean, `c.y` → `TK2341`, `new D().g` clean. No regression on
  `m11_classes`/`m13_modifiers`/`m14_readonly`. Official-suite scoreboard: the spurious-`TK2339` count
  drops; `run --check` stays green (no new regression).
- **Touch points.** `src/check/checker/classes.rs` (`collect_class_own_members`, `fill_class`); the
  member-type/access-control checks that consume collected members; possibly `src/binder/bind.rs`
  (the binder's deferred-param-property note, `bind.rs:~333`). The expr `TK2339` site is the symptom,
  not the fix.

### WU2 — Accessibility checked through object-destructuring (effort M)

- **Problem.** `let { priv } = new K()` / `function f({ priv }: K)` never run the private/protected
  access check (`classes.rs:765-797`), because destructured names are unbound
  (`calls.rs:694-700`, `assignment.rs:388-395`). tsc reports `TS2341`/`TS2445`; typokat is silent
  (false-negative).
- **Verify first.** Confirm destructuring is still entirely unbound on both paths. Find where a
  variable-declaration destructuring pattern is *walked* today (decl checking) and where a destructured
  function parameter is walked (`lower_parameters`, `calls.rs:606-642`). Decide the minimal hook: we
  only need to **run the access check per destructured property against the statically-known source
  type** — not to type the bindings.
- **Scope.** For an **object**-destructuring pattern over a value whose type is statically known
  (variable declaration `let { p } = expr` and function parameter `({ p }: K)`), resolve each named
  property on the source type and run `check_member_access_control`. Object patterns with plain named
  properties only.
- **Acceptance / witness.** New dir `tests/cases/f4_destructuring_access/`: destructured `private` →
  `TK2341`, destructured `protected` from outside → `TK2445`, matching `tsc --strict`; a destructure
  of a public member stays clean. No regression.
- **Touch points.** Destructuring-pattern walking in decl + parameter paths
  (`src/check/checker/decls.rs`, `calls.rs`); `check_member_access_control` (reused as-is).

### WU3 — readonly / property enforced through a union member access (effort M)

- **Problem.** `u.value = 12` where `u: A | B` and `value` is `readonly` is silent: `assignment.rs:177-194`
  looks the property up only via `object_type(base_ty)`, so a union base falls through to the
  `None → return` at 192-194 (false-negative, no `TK2540`).
- **Verify first.** Confirm the object-only lookup at `assignment.rs:177-189`. Probe `tsc --strict` for
  the union-readonly rule: assigning to a member that is `readonly` on the accessed union — confirm
  `TS2540`, and check the mixed case (readonly on some members only) so the fixtures don't over-claim.
  Reuse the read-side union resolution (`union_member_access`, `expr.rs:546-583`) as the model.
- **Scope.** When the assignment base is a union, resolve the property across union members and enforce
  `readonly` (emit `TK2540`) as the non-union path already does; keep property-existence behavior
  consistent with the read side. Scope the fixtures to the backlog-03 repro (readonly on all members).
- **Acceptance / witness.** New dir `tests/cases/f5_union_readonly/`: assigning to a `readonly` member
  of `A | B` → `TK2540`, matching tsc. No regression on `m4_unions`/`m14_readonly`.
- **Touch points.** `src/check/checker/assignment.rs` (`check_member_assignment` — extend the base
  resolution to union bases); reuse `expr.rs` union member access.

### WU4 — readonly assignment value-type cascade (effort S)

- **Problem.** `this.ro = 5` (where `ro: 1` is `readonly`) emits only `TK2540`; the `TK2322`
  (`5` not assignable to `1`) that tsc also emits is suppressed by the `return` at
  `assignment.rs:224`, which short-circuits before the obligation at `assignment.rs:230-242`. The line
  *is* flagged, so this is a precision gap, not a soundness hole.
- **Verify first.** Confirm the early `return` at 224 and that the obligation block (230-242) is the
  only `TK2322` source on this path. Check `m14_readonly` fixtures whose readonly RHS *matches* the
  member type — they must **stay** single-diagnostic (no new `TK2322`).
- **Scope.** After emitting `TK2540`, still collect the type-assignability obligation (don't `return`),
  so `TK2322` also fires when the RHS isn't assignable — without regressing the matching-RHS cases.
- **Acceptance / witness.** New dir `tests/cases/f6_readonly_cascade/`: assigning an out-of-type value
  to a `readonly` member reports **both** `TK2540` and `TK2322`; a matching-type readonly assignment
  reports `TK2540` only. No regression.
- **Touch points.** `src/check/checker/assignment.rs` (`check_member_assignment`, the readonly branch).

## Out of scope (explicit)

- **Full destructuring type support** — WU2 only runs the access check; it does **not** type the
  destructured bindings, handle array patterns, nested patterns, defaults, or renaming. (Future
  backlog if needed.)
- **Element-access assignment targets** (`obj[k] = …`) and **compound-assignment readonly**
  (`+=` on a readonly member) — remain deferred (`assignment.rs:48-56`, `:156-160`).
- **Static un-annotated fields / computed keys** in WU1 — keep behavior consistent with the instance
  path; do not expand to computed keys.
- **WU3 mixed-union readonly subtleties** beyond the backlog-03 repro — if `tsc` disagrees on
  partially-readonly unions, document it rather than over-fitting.

## Decisions

- **Bug-fix fixtures go in dedicated new dirs `f3_…`–`f6_…`, registered `false` in `MILESTONE_DIRS`
  and flipped `true` by the implementation commit.** Rationale: adding fixtures to an *already-enabled*
  milestone dir would make the standalone spec commit fail tests, breaking the dev-method's
  spec/implementation separation (`reference/dev-method.md` §1). Naming by official-suite finding ID
  (F3–F6) keeps the trace from finding → fixture. Add a one-line note in `tests/cases/README.md` for
  these fix-dirs. *(This sets a convention for all future bug-fix fixtures — promote to an ADR if a
  second sprint reuses it.)*
- **Field-init widening (WU1):** a non-readonly initialized field widens its literal initializer
  (`g = 2` → `number`), matching tsc; verify per-fixture against `tsc --strict`.

## Sequencing

| WU | Touches | Parallel? |
|---|---|---|
| WU1 | `classes.rs` member collection | independent — start first (largest, highest value) |
| WU2 | decl + param destructuring walk | independent of WU1/WU3/WU4 |
| WU3 | `assignment.rs::check_member_assignment` | **serialize with WU4** (same function) |
| WU4 | `assignment.rs::check_member_assignment` | do right after WU3 (or as one combined edit) |

WU1 and WU2 can run as parallel tracks. WU3 → WU4 must be serialized (same function, would conflict);
the same implementation subagent should carry both. Each WU still runs the full spec → impl → review →
commit loop; the leader writes every spec and makes every commit.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- WU4 / item 04 is a non-bug at HEAD (tsc never emits TK2540+TK2322 together; typokat already
  matches). → graduated to OUTCOME; item 04 deleted.
- WU3 / item 03 was broader than "union" — object-type & interface `readonly` was dropped in
  lowering; the headline repro canonicalizes to that plain-object case. → graduated to OUTCOME.
- WU1 review caught two pre-commit defects (this-init `any` FN; obligations double-report). → fixed,
  graduated to the WU1 commit message + OUTCOME.
