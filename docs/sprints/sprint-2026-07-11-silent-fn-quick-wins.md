# Sprint — silent-FN quick wins (2026-07-11)

**Goal.** Close backlogs `67` and `66`: enforce the modeled `ReturnType` constraint
and report incompatible protected-over-protected overrides without disturbing valid
cross-space utility evaluation or nominal class semantics.

**Theme.** Both items are release-blocking dropped-error families whose diagnostic
machinery already exists. The sprint supplies the missing declaration constraint and
the missing protected-pair admission rule, then removes their deferred-ledger owners.

## Refs re-verified at HEAD (2026-07-11)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `ReturnType<T>` is the only implemented utility alias whose lib-style callable
  constraint is omitted; its body already uses the sound `never[]` callable shape —
  `src/prelude.ts:21`.
- ✔ Alias instantiation already lowers explicit arguments, resolves parameter ids, and
  runs the shared `TK2344` constraint query before substituting/evaluating the template —
  `src/check/checker/decls/resolve.rs:253-310`.
- ✔ Focused probes show `T extends (...args: never[]) => unknown` rejects `number` and
  accepts nullary, non-nullary, and rest function signatures without introducing `any`.
- ✔ Override collection currently admits only public↔public pairs through two explicit
  visibility guards — `src/check/checker/classes/inheritance.rs:45-74`.
- ⚠ The backlog anticipated a nominal-relation bypass, but the current bespoke override
  engine compares member `TypeId`s directly and already implements method bivariance,
  covariant returns, and strict field/accessor comparison —
  `src/check/checker/statements.rs:1437-1518`. The global nominal relation must not change.
- ✔ Disabled acceptance witnesses already exist for both dropped errors under
  `tests/cases/sr_deferred_ledger/`; dedicated focused corpora can copy and strengthen
  them without enabling unrelated deferred items — `tests/conformance.rs:120-122`.
- ✔ Both families are release-blocking incomplete criteria in
  `docs/backlog/completion-1.0.toml` (`C-utility-alias-constraint` and
  `C-protected-override-compat`).

## Work units

### WU0 — focused spec-only corpora (effort S)

- **Problem.** The existing witnesses share the permanently disabled
  `sr_deferred_ledger` directory, so either fix cannot be enabled independently and the
  current controls are narrower than the implementation paths they protect.
- **Verify first.** Re-run each ledger fixture directly at old HEAD and cross-check every
  error/clean verdict against `tsc 6.0.3 --strict`.
- **Scope.** Add disabled `b67_utility_alias_constraint/` and
  `b66_protected_override_compat/` corpora. For `67`, cover invalid `number` plus valid
  nullary, non-nullary, and rest function arguments and prove the evaluated return type.
  For `66`, cover incompatible and compatible protected methods, covariant return, a
  strict protected field mismatch, and public/private/mixed-visibility controls that keep
  their existing disposition. Register both as `false` and document them in the corpus
  index.
- **Acceptance / witness.** Commit the disabled corpora independently. Enabling each at
  old HEAD fails only on its intended missing diagnostic; clean controls already pass.
- **Touch points.** `tests/cases/b67_utility_alias_constraint/`,
  `tests/cases/b66_protected_override_compat/`, `tests/cases/README.md`,
  `tests/conformance.rs`.

### WU1 — enforce the modeled `ReturnType` constraint (effort S)

- **Problem.** `ReturnType<number>` evaluates to `never` without first emitting
  `TK2344`, because the prelude declaration has no recorded constraint.
- **Verify first.** Confirm the prelude pass retains/lower constraints into the shared
  type-parameter constraint column and that valid represented function signatures relate
  to `(...args: never[]) => unknown`.
- **Scope.** Add the narrow callable constraint to the canonical source-backed
  `ReturnType` declaration. Reuse the existing `never[]` soundness choice and shared M24
  alias-instantiation constraint path; do not special-case the utility name or introduce
  permissive `any`.
- **Acceptance / witness.** Enable `b67_utility_alias_constraint`; invalid non-callable
  arguments emit one `TK2344`, all valid function controls stay clean and preserve their
  return types, and existing M24/M28 corpora retain diagnostic identities.
- **Touch points.** `src/prelude.ts`, focused utility/prelude unit tests only if needed,
  `tests/conformance.rs`.

### WU2 — protected-pair override compatibility (effort S/M)

- **Problem.** Incompatible protected overrides never enter the existing phase-2
  `TK2416` queue, while compatible protected redeclarations must remain clean.
- **Verify first.** Probe protected method parameter bivariance, covariant/invalid
  returns, protected fields, public↔protected visibility changes, private redeclarations,
  accessors, optional/rest arity, and generic-base deferrals against `tsc 6.0.3`.
- **Scope.** Admit exactly public↔public and protected↔protected pairs into the existing
  override queue. Keep private and mixed-visibility pairs out, preserve current generic/
  error/span/arity gates, and leave the global nominal relation untouched.
- **Acceptance / witness.** Enable `b66_protected_override_compat`; incompatible
  protected method/field shapes emit `TK2416`, compatible and covariant controls stay
  clean, and b06/M13/M15 inheritance/modifier suites do not regress.
- **Touch points.** `src/check/checker/classes/inheritance.rs`, focused checker tests if
  needed, `tests/conformance.rs`.

### WU3 — independent adversarial review and closure (effort M)

- **Problem.** A tiny filter/constraint diff can still create false positives by
  over-constraining valid functions or by sending legal protected redeclarations through
  nominal checks.
- **Verify first.** A reviewer independent of implementation starts from WU0 and reruns
  focused probes against `tsc 6.0.3 --strict`.
- **Scope.** Hunt `ReturnType` callable shapes, deferred/free arguments, function
  variance, protected method/field/accessor combinations, inheritance depth, visibility
  changes, and diagnostic duplication/order. On PASS, run all quality gates and the
  official-suite identity ratchet; remove shipped divergence records, complete both
  manifest criteria, delete backlogs `66`/`67`, and archive this sprint with a factual
  outcome.
- **Acceptance / witness.** Independent PASS with zero unexplained false negative or new
  false positive; format, full tests, clippy, release build, manifest/divergence
  validators, and official-suite `run --check` all pass.
- **Touch points.** Read-only implementation diff and probes; then
  `docs/reference/divergences.md`, `docs/backlog/completion-1.0.toml`,
  `docs/backlog/README.md`, `docs/backlog/66-protected-override-compat.md`,
  `docs/backlog/67-utility-alias-constraint-enforcement.md`, docs indexes/archive, and
  the scoreboard only for audited intended movement.

## Out of scope (explicit)

- Global protected/private nominal assignability and the safe-direction parity tail in
  backlog [`63`](../backlog/63-review-parity-tail.md); this sprint only changes the
  override-local admission rule.
- Visibility-narrowing `TS2415`, static-side `TS2417`, unequal-arity override parity,
  generic bases, and optional/rest-specific override expansion; retain their current
  owners/dispositions.
- Other utility aliases (`Parameters`, `ConstructorParameters`, `InstanceType`,
  `ThisType`, `Awaited`, `NoInfer`) and general constraint approximation (`37`).
- Numeric stringification (`30`), aliased `new` (`22`), non-callable diagnostics (`19`),
  and static generic methods (`23`/`41`); code tracing showed they are not honest fillers
  for this sprint.

## Decisions

- Use the existing `never[]` callable model with `unknown` return for the `ReturnType`
  constraint. No `any`, utility-name special case, or second constraint mechanism.
- Extend only the existing override queue's visibility predicate. Do not weaken or bypass
  the relation engine's nominal-origin invariant globally.
- Keep the sprint to two release-blocking silent-FN families. A third item would reduce
  review depth without sharing implementation machinery.
- Follow the mandatory loop: leader-owned disabled spec commit, subagent implementation,
  different-agent adversarial review, leader verification/commits, then docs closure.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Both focused corpora committed disabled and tsc-cross-checked. |
| 2 | WU1 | `b67` enabled; constraint and M24/M28 suites green. |
| 3 | WU2 | `b66` enabled; class override/modifier/accessor suites green. |
| 4 | WU3 | Independent PASS, full gates, audited closure/archive. |

WU1 and WU2 touch disjoint implementation paths, but run sequentially so each diagnostic
family gets an isolated implementation commit and review evidence.

## Run log

<!-- Append discoveries, deviations, and blockers here. Graduate durable findings to an
     ADR/backlog/reference document; leave only transient execution notes in the run log. -->

