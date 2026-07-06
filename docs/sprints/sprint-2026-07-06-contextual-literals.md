# Sprint — contextual typing for fresh literals (2026-07-06)

**Goal.** Ship backlog [`31`](../backlog/31-object-literal-contextual-typing.md):
target-aware literal preservation for fresh object, array, and tuple literals in
checked assignment positions.

**Theme.** Remove a common false-positive family before broader real-project work:
literal-heavy configs, discriminant fields, literal arrays, and tuple returns should
not reject their own fresh literal initializers when a concrete target type is known.

## Refs re-verified at HEAD (2026-07-06)

- ✔ **Object literals still widen member values unconditionally** —
  `infer_object_literal` widens every property value before interning the object
  (`src/check/checker/expr.rs:218`).
- ✔ **Array literals still widen element values unconditionally outside tuple
  contexts** — `infer_array_literal` unions widened element types
  (`src/check/checker/expr.rs:243`).
- ✔ **Only declaration tuple contexts use the contextual initializer hook today** —
  `infer_initializer` recognizes an array literal only when the declaration
  annotation is already a tuple (`src/check/checker/expr.rs:274`,
  `src/check/checker/statements.rs:188`).
- ✔ **Declaration annotations are the existing target-aware site** —
  `check_declarator` lowers the annotation before the initializer and pushes the
  assignment obligation against that target (`src/check/checker/statements.rs:204`).
- ✔ **Call arguments are inferred before parameter types are known to the argument
  inference loop** — `infer_call` builds `arg_types` with plain `infer_expr`, then
  later evaluates `param_types` and calls `check_call_arguments`
  (`src/check/checker/calls.rs:271`, `src/check/checker/calls.rs:331`).
- ✔ **Return checks currently infer first, then relate to the declared target** —
  `check_return` uses `infer_expr`, and arrow expression bodies do the same
  (`src/check/checker/statements.rs:138`, `src/check/checker/calls.rs:781`).
- ✔ **Fresh-literal provenance already matters for generic constraint inference** —
  `infer_type_arguments` has the M24 fresh object/array exemption, explicitly
  documenting the missing contextual pass this sprint closes for concrete targets
  (`src/check/infer.rs:99`).

## Work units

### WU1 — Disabled M30 corpus (effort S)

- **Problem.** Backlog `31` has no acceptance corpus; current fixtures document tuple
  declaration contexts only.
- **Verify first.** Cross-check the new fixtures with `tsc 6.0.3 --strict --noEmit`.
- **Scope.** Add `tests/cases/m30_contextual_literals/` with object-member,
  argument/return, array, tuple, nested, and no-context widening witnesses. Register
  the dir disabled in `MILESTONE_DIRS`.
- **Acceptance / witness.** `cargo test` is behavior-neutral while the directory is
  disabled; enabling it at HEAD fails on the expected false-positive family.
- **Touch points.** `tests/cases/m30_contextual_literals/**`,
  `tests/conformance.rs`, `tests/cases/README.md`.

### WU2 — Target-aware literal inference hook (effort L)

- **Problem.** Fresh literals are inferred without the target type and then related,
  so their literal members/elements widen too early.
- **Verify first.** Probe at HEAD: `const x: { kind: "a" } = { kind: "a" }` reports
  `TK2322`.
- **Scope.** Extend the existing `infer_initializer(scope, expr, context)` hook into
  a general target-aware literal path:
  fresh object members use the matching target property as context, array elements
  use the target array element when the target is an array, and tuple elements keep
  the existing positional model. Preserve ordinary no-context widening.
- **Acceptance / witness.** Object/array/tuple declarations in the M30 corpus accept
  matching literals and reject wrong literal values; no-context witnesses still widen.
- **Touch points.** `src/check/checker/expr.rs`,
  `src/check/checker/statements.rs`, relation-facing obligations only as needed.

### WU3 — Argument and return target threading (effort M)

- **Problem.** Calls, `new`, `super`, declared returns, and arrow expression bodies
  all infer expressions before relating them to known parameter/return targets.
- **Verify first.** Probe at HEAD: `takesShape({ kind: "circle" })` and
  `function f(): Shape { return { kind: "circle" } }` over-report.
- **Scope.** Use the same target-aware literal path for concrete parameter and
  declared-return targets after callee/constructor signatures are known. Keep generic
  type-argument inference's raw argument pass intact; only the final assignability
  obligation should use contextually typed fresh literal sources.
- **Acceptance / witness.** M30 argument/return fixtures pass; generic inference
  behavior and fresh-literal clamp exemption remain unchanged.
- **Touch points.** `src/check/checker/calls.rs`,
  `src/check/checker/statements.rs`.

### WU4 — Review, docs, and ratchet (effort M)

- **Problem.** Contextual typing can easily become too broad and silently accept
  values that should stay widened.
- **Verify first.** Re-run M30 fixtures through `tsc --strict`; probe no-context
  widening and mismatched literal cases explicitly.
- **Scope.** Independent adversarial review for false negatives and regressions;
  update docs to mark M30 shipped and remove the now-stale tuple/array contextual
  typing deferral text.
- **Acceptance / witness.** `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  and official-suite `run --check` are green; `m30_contextual_literals` is enabled.
- **Touch points.** `tests/conformance.rs`, `tests/cases/README.md`, `README.md`,
  `docs/INDEX.md`, backlog `31`.

## Out of scope (explicit)

- General contextual typing of function expressions, object methods, overload
  selection, JSX, computed properties, spreads, destructuring/default initializers,
  `as const`, readonly tuple inference, contextual typing through object-union target
  selection, and full lib-driven contextual typing.
- Fixing generic fresh-literal constraint excess diagnostics (`TS2353`) beyond concrete
  target contexts; M24's fresh-literal clamp provenance must keep its current behavior
  unless a concrete parameter type is already available.

## Decisions

- **Reuse `infer_initializer` as the target-aware entry point.** It is already the
  declaration-context hook and keeps no-context `infer_expr` behavior as the default.
- **Context preserves only fresh literal source shape.** Identifiers and typed values
  are not reshaped; they continue through ordinary assignability.
- **Keep source types narrow only inside the obligation.** The declared symbol still
  takes the annotation type, and no-context inference still widens.

## Sequencing

1. Commit the disabled M30 corpus and this sprint plan.
2. Dispatch implementation subagent for WU2/WU3.
3. Run local verification, then dispatch independent adversarial review.
4. Fix review findings through the implementation agent.
5. Commit implementation/docs, then close/archive the sprint and delete or re-scope
   backlog `31`.

## Run log

<!-- Append as you work. -->
