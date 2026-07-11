> **OUTCOME — shipped 2026-07-11.** Backlog `38` is closed: the existing source-backed
> prelude now carries declared value types into both single-file and serial-project
> user passes, providing a deliberately bounded `console` (`log`/`warn`/`error`) and
> numeric `Math` surface. The required adversarial review first found two high-severity
> cross-space shadowing false negatives; the follow-up fix made all value/type consumers
> slot-aware, and independent re-review passed. Commit map: sprint plan → `3e33517`, WU1
> spec → `42f66b5`, blocker record → `5e9f756`, ADR-0004 → `4840326`, WU2 + review fixes
> → `b634803`, scoreboard ratchet → `c9ff140`. Verification: `cargo fmt --check` ·
> `cargo test` (285 unit + 14 conformance-harness + 4 divergence + 7 incomplete + 10
> manifest + 5 surface, 0 failed) · `cargo clippy --all-targets -- -D warnings` ·
> `cargo build --release` · official-suite unit tests (34) · freshly fetched 874-test
> corpus `run --check` (0 regressions). Scoreboard: 396→422 in-scope, 478→452 OOS,
> `clean-kept` 116/149→126/159, zero lost matched identities or new false positives.
> Backlog closed: `38`; manifest `D-minimal-ambient-prelude` complete. Deferred: primitive
> wrapper members and array instance methods stay with the model/full-lib path; `73` still
> gates the honest preview in `72`.

# Sprint — minimal ambient prelude (2026-07-11)

**Goal.** Deliver a deliberately small, tsc-checked ambient declaration slice through
the existing prelude compilation unit, creating earlier real-world signal without
claiming `lib.d.ts` fidelity.

**Theme.** Backlog `38` was the approved, replaceable bridge from the
utility-type-only prelude to the pinned-project preview
in [`72`](../backlog/72-real-project-preview-readiness.md). It adds only declarations
the implemented model can faithfully check, measures their official-suite effect, and
leaves one canonical loading path for backlog [`14`](../backlog/14-libdts-loading.md)
to replace.

## Refs re-verified at HEAD (2026-07-11)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ The shipped prelude is source-backed and currently contains only the ten utility
  aliases plus four string-intrinsic markers — `src/prelude.ts:1-25`.
- ✔ Every ordinary check parses that source as a real TypeScript program, binds it
  before the user module, reserves/fills its declarations in the same type universe,
  and keeps user declarations as normal inner-scope shadows —
  `src/check/checker/mod.rs:42-43`, `src/check/checker/mod.rs:80-125`; `src/binder/bind.rs:123-130`.
- ✔ The current prelude has direct regression coverage for clean parsing/checking and
  user shadowing, so ambient additions can extend an established test seam rather than
  add a second loader — `src/check/checker/tests/utility_types.rs:28-56`.
- ✔ M32 supports rest signature shape and M33 supports overloads; generic *method*
  declarations are still a blocker of full `lib.d.ts` and are out of this slice —
  [`../backlog/lib-audit-6.0.3.md`](../backlog/lib-audit-6.0.3.md),
  [`../backlog/41-generic-methods.md`](../backlog/41-generic-methods.md).
- ✔ The conformance harness supports a disabled, behavior-neutral backlog corpus that
  is flipped on by the implementation commit — `tests/conformance.rs:69-125`.
- ✔ The official-suite scoreboard is an identity ratchet: a save is valid only after
  auditing intended status and diagnostic-identity movement —
  [`../../tooling/official-suite/README.md`](../../tooling/official-suite/README.md).
- ⚠ `Array<T>` is an existing special type representation rather than a normal
  prelude-declared interface — `src/check/checker/decls/resolve.rs:195-207`. WU1 must
  prove any proposed array-member declaration reaches that representation; it may not
  assume that an `interface Array<T>` automatically supplies members to `T[]`.
- ⚠ The remaining expression-shape tail in backlog `73` is deliberately inventoried
  but non-emitting. This sprint must not describe a clean result on arbitrary project
  code as complete; backlog `72` remains blocked by `73`.

## Work units

### WU1 — spec-only curated ambient corpus and feasibility ledger (effort M)

- **Problem.** The approved `38` scope names useful globals and member families, but
  the actual prelude currently has no ambient values and the type model has a distinct
  representation for arrays. Adding a familiar lib declaration without proving its
  lookup path would create either a false promise or a silent no-op.
- **Verify first.** At HEAD, run focused `tsc 6.0.3 --strict` and typokat probes for
  candidate `console`, `Math`, primitive `.length`/non-generic methods, and simple
  `Array` members. Trace each successful/failed typokat lookup through annotation
  lowering, expression member access, and call checking. Record the expected `TK`
  identity, the tsc verdict, and whether the declaration shape is already modeled.
- **Scope.** Add disabled `tests/cases/b38_minimal_ambient_prelude/` fixtures and
  register them `false` in `MILESTONE_DIRS`. The corpus must pin, for every admitted
  name: clean resolution, a wrong-argument or wrong-assignment diagnostic where
  applicable, user shadowing, and a nearby unresolved control for a deliberately
  absent member. Write a compact curated-surface ledger in the fixture README: exact
  declaration text/name, exercised shape, tsc result, and rationale for inclusion or
  rejection. Start from `console`, `Math`, string/number members, and simple array
  members, but admit only candidates proved to work through existing semantics.
- **Split gate.** If a candidate requires primitive boxing, a new array instance-member
  representation, generic methods, namespace/declaration merging, a second prelude
  loader, or a special project-only shim, stop after the spec commit. Record the
  evidence and ask for a new architectural decision or a rescope; do not invent a
  compatibility path in this sprint.
- **Acceptance / witness.** The commit is behavior-neutral with the corpus disabled;
  each fixture fails at old HEAD only for the missing ambient surface it specifies,
  while controls prove the fixture itself is within the implemented syntax. Every
  expected diagnostic is cross-checked against pinned `tsc`.
- **Touch points.** `tests/cases/b38_minimal_ambient_prelude/`,
  `tests/cases/README.md`, `tests/conformance.rs`, focused scratch probes, and the
  fixture-local ledger.

### WU2 — load the bounded ambient declarations through the canonical prelude (effort M)

- **Problem.** Existing user code can only resolve the M28 utility aliases; otherwise
  standard ambient values and their admitted members are unresolved.
- **Verify first.** Re-run WU1's admitted fixtures and inspect the prelude bind/reserve/
  fill path. Confirm all added declarations parse clean as both trusted prelude source
  and ordinary user source, preserve existing utility aliases, and retain ordinary
  user shadowing.
- **Scope.** Per [ADR-0004](../decisions/0004-prelude-value-type-handoff.md), extend
  the existing source-backed prelude pipeline so its declared value types are lowered
  and handed into the user pass in both single-file and serial-project modes. Then add
  only WU1's admitted `console` and numeric `Math` declarations to `src/prelude.ts`.
  Reuse the existing prelude/binder path without a second source string, special global
  lookup, new type universe, or user-project shim. Add narrow unit coverage for
  prelude cleanliness, value-table alignment, and user shadowing where the conformance
  corpus cannot observe the invariant. Flip the `b38_minimal_ambient_prelude` corpus
  to enabled in the same implementation commit.
- **Acceptance / witness.** Every enabled b38 fixture produces the same clean/error
  verdict as `tsc 6.0.3 --strict`; each declared callable rejects its pinned bad call,
  absent members stay unresolved, and a user declaration shadows the ambient one
  without duplicate-name noise. Existing M28/M32/M33 and all previous conformance
  fixtures retain their diagnostic identities.
- **Touch points.** `src/prelude.ts`, `src/check/checker/mod.rs`, potentially
  `src/check/checker/tests/utility_types.rs`,
  `tests/cases/b38_minimal_ambient_prelude/`, and `tests/conformance.rs`.

### WU3 — independent adversarial fidelity and soundness review (effort M)

- **Problem.** A compact ambient surface can accidentally be too permissive, leak into
  a user shadow, or appear to support a declaration whose member/call path is not
  actually checked.
- **Verify first.** Start from the WU1 ledger and current diff, independently rerun
  each admitted declaration against `tsc 6.0.3 --strict`, and inspect the checker path
  rather than relying on the implementation rationale.
- **Scope.** Probe overload/rest arity, argument and return mismatches, method/property
  member lookup, literal versus widened primitive receivers, array behavior where
  admitted, user shadowing in value and type spaces, duplicate names, and diagnostics
  from the prelude itself. Search for accidental generic-method-shaped declarations or
  widening to `any`/error type. Classify every mismatch as a defect, deliberate safe
  over-report with an existing owner, or an exclusion from the curated surface.
- **Acceptance / witness.** An independent review returns PASS with concrete probes
  and zero unexplained false negative. Any discovered false negative is fixed against
  a new fixture and re-reviewed before closure.
- **Touch points.** Read-only implementation diff, WU1 corpus/ledger, existing M28/M32/
  M33 fixtures, focused `tsc` probes.

### WU4 — audited official-suite ratchet and sprint closure (effort M)

- **Problem.** The point of `38` is measurable early real-world signal; a prelude
  change without an audited scoreboard could silently trade unresolved tests for false
  positives or incomplete outcomes.
- **Verify first.** Build the release binary, fetch the pinned official corpus, run the
  existing scoreboard check before any re-save, then aggregate all movement by
  unresolved/incomplete identity and diagnostic identity. Confirm each newly in-scope
  file uses only the curated surface and contains no unclassified regression.
- **Scope.** Save the official-suite scoreboard only for audited intended progress;
  record the unresolved-bucket and `clean-kept` deltas in the sprint outcome. Run the
  full quality gate, update public limitations/reference docs if behavior claims
  changed, delete or rescope backlog `38`, mark its completion-manifest criterion
  complete, archive this sprint, and update the docs indexes. Do not claim `72` is
  ready while `73` remains incomplete.
- **Acceptance / witness.** `cargo fmt --check`, `cargo test`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, official-suite
  unit tests, and freshly fetched `run --check` all pass. The saved scoreboard has no
  unexplained lost matched identity, new false positive, or in-scope-to-unsupported
  regression; all intended progress is attributable to a declaration in the WU1 ledger.
- **Touch points.** `tooling/official-suite/scoreboard.txt`,
  `tooling/official-suite/`, `README.md`, `docs/backlog/completion-1.0.toml`,
  `docs/backlog/README.md`, `docs/INDEX.md`, `docs/sprints/README.md`,
  `docs/archive/`, and backlog `38` at closure.

## Out of scope (explicit)

- Full `lib.d.ts`, lib file discovery/loading, and parallel shared-prelude work —
  backlog [`14`](../backlog/14-libdts-loading.md) owns the canonical replacement.
- Generic methods, namespaces/declaration merging, `this` parameters, enums, and
  `satisfies`/`as const` — Track A backlogs [`41`](../backlog/41-generic-methods.md),
  [`43`](../backlog/43-namespaces-declaration-merging.md),
  [`70`](../backlog/70-this-parameter-typing.md), [`42`](../backlog/42-enums-type-side.md),
  and [`44`](../backlog/44-satisfies-as-const.md).
- A new primitive-boxing or array instance-member architecture. WU1 established
  that it is required for a proposed name, the sprint stops and asks rather than
  silently widening its design.
- Project discovery, tsconfig handling, resolver breadth, package declarations, and
  the public preview CLI — backlog [`72`](../backlog/72-real-project-preview-readiness.md)
  and [`15`](../backlog/15-modules-imports.md).
- Closing the `infer_expr` emission tail or representing arbitrary unknown-project
  code as complete — backlog [`73`](../backlog/73-unsupported-surface-audit.md).

## Decisions

- Use the existing `PRELUDE_SOURCE` compilation unit as the only ambient-loading
  mechanism. Backlog `14` replaces its content/path; it never coexists with a second
  loader.
- Keep the admission rule semantic, not aspirational: a familiar standard-library
  name is included only when fixtures prove it resolves and checks through existing
  modeled behavior. The narrow surface is a feature, not a partial fidelity claim.
- Keep the mandatory development loop intact: WU1 is a disabled spec-only commit,
  WU2 enables it, WU3 is independently reviewed, and WU4 audits and closes it.
- The sprint creates earlier signal but does not change the preview dependency: `72`
  still waits for `73`'s complete surface-accounting criterion.
- ADR-0004 approves the localized prelude value-type handoff discovered in WU1. It
  must preserve declaration-index alignment in both checker entry points and may not
  turn a visible prelude value into an error-typed false-clean.

## Sequencing

| Order | Work | Gate |
| --- | --- | --- |
| 1 | WU1 | Commit the disabled corpus and ledger before modifying behavior. |
| 2 | WU2 | Proceed only with declarations that pass WU1's feasibility gate. |
| 3 | WU3 | Independent review of the implementation and new fixtures. |
| 4 | WU4 | Audit scoreboard movement and close only after all quality gates pass. |

WU1 may gather `tsc` probes in parallel with read-only path tracing, but WU2–WU4 are
strictly ordered by their witnesses.

## Run log

<!-- Append as you work: discoveries, deviations, blockers. Graduate each entry:
     changed the *why* → ../decisions/NNNN ; new future work → ../backlog/NN ;
     transient → leave it (dies with the sprint on archive). After graduating,
     trim to a one-line pointer ("→ ADR-0007"). -->

- 2026-07-11 — WU1 shipped as `42f66b5`: the disabled corpus and ledger admit only
  `console` and numeric `Math`; primitive wrapper and array instance members hit the
  planned model-boundary gate. A follow-up code trace found a stronger blocker:
  the prelude pass fills type declarations only and discards value declaration types,
  so a `declare const console` or `Math` in `src/prelude.ts` binds a name but cannot
  provide a checked value type to user code. WU2 is paused pending independent
  architecture review and an explicit decision; no alternate loader or value-seeding
  path will be introduced implicitly.
- 2026-07-11 — ADR-0004 accepts the narrow canonical handoff after independent review:
  retain/lower prelude value declarations into the user pass in both entry points;
  no second loader, global-name special case, primitive boxing, or array-member model.
- 2026-07-11 — WU2 first review FAILED: a type-only local name hid an inherited prelude
  value, and a value-only local name hid an inherited prelude type. The repair made
  value/type lookup slot-aware across all consumers, added single/project regressions,
  and passed independent re-review before `b634803`.
- 2026-07-11 — WU4 audited 26 `OOS:unresolved → IN` official-suite transitions. The
  saved ratchet has zero regressions on a fresh full-corpus check; see `c9ff140`.
