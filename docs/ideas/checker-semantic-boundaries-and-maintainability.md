# Checker semantic boundaries and maintainability

## Status (2026-09-09) — graduated

The 2026-09-06 audit is closed as an idea. Everything actionable now has an owner; this file keeps
the evidence, the provenance limits, the refuted proposals (so they are not re-proposed) and the
one open research question. It authorizes nothing by itself.

| Finding | Where it went |
|---|---|
| M12 unused dev-deps · M6 alias-surface duplicate · M7 tuple-rest triplicate | [backlog 125](../backlog/125-checker-duplicate-helper-cleanup.md) → [sprint 2026-09-09](../sprints/sprint-2026-09-09-audit-cleanup-batch.md) WU1–WU3 |
| M4 mirrored pass request · M1 manifest built three times | [backlog 126](../backlog/126-pass-request-and-manifest-ownership.md) → sprint WU5–WU6 |
| M10 duplicated notice formatter | [backlog 129](../backlog/129-reexport-notice-formatter.md) → sprint WU4 |
| M2 partial Rust scanners in test guards · M3 spelling assertions | [backlog 127](../backlog/127-test-guard-parsers-and-shape-assertions.md) |
| M5 hand-synchronized native role indexes | [backlog 128](../backlog/128-typed-native-library-roles.md) |
| M8 inline collision policy in `publish_class_surfaces` | already [backlog 119](../backlog/119-class-publication-preparation.md) |
| M9 manual member-context restoration · M11 driver error identity loss | [backlog 131](../backlog/131-conditional-ownership-cleanups.md), conditional |
| S1–S3 as-const transition defects | repaired by the shipped feature `6b3cd507`; see below |
| S4 / U5 transition coverage gap | [backlog 130](../backlog/130-transition-regression-coverage.md) |
| U0 reproduce and classify | done at closure of the [as-const sprint](../archive/sprint-2026-08-17-as-const.md) |
| U1–U4 repair units | superseded by the sprint's R-A/R-B/R-C repairs; the residual construction-order work is [backlog 120](../backlog/120-semantic-constraint-construction-order.md) |

The detailed finding bodies (evidence lines, why it hurts, smaller change, acceptance) live in the
backlog items. Line numbers below refer to the audit tree, not to `main`.

## Evidence, provenance, and limits

Two evidence sources from 2026-09-06:

1. **Read-only maintainability audit.** Six agents inspected structure, duplication, architecture,
   error/state handling, tests, and dependencies. Four initially high-priority proposals received
   independent adversarial review; a separate review checked the size census. The audit inspected
   the working tree based on `ae145bc828d16cd30e14730344b9fd3e8c0ceb36`, including pre-existing
   uncommitted implementation changes.
2. **As-const implementation report supplied by the user.** It describes regressions discovered
   while changing literal modes, arrow contextual typing, and inference, and distinguishes
   pre-existing gaps from introduced regressions. The audit did not rerun those repros; the sprint
   closure did (U0).

At documentation time HEAD was `fa6cb341295a4284ab36123a01ee1b1411f21a38`; the feature has since
shipped as `6b3cd507`. The maintainability findings were re-located on `main` `81147d86` when they
graduated (2026-09-09).

### Audit baseline

| Check or measurement | Result | Qualification |
|---|---|---|
| `cargo check --all-targets` | Passed | Audit working tree, not a later candidate. |
| `cargo clippy --all-targets -- -D warnings` | Passed | Same qualification. |
| `cargo test` | Interrupted by audit leader, exit 130 | Eight completed binaries: 58 passed, 0 failed, 1 ignored. **Not a full-suite pass.** |
| Targeted source coverage | 54 of 263 Rust files; 20,127 of 235,787 physical lines, 8.54% | Union of recorded ranges under `crates`, `src`, and `tests`; 9 complete files. |
| Source groups covered | 10 of 15 | Crate directories plus root `src` and `tests`. |
| Normal local dependency graph | 14 packages, 45 edges, no cycle backedges | Cargo metadata; not a proof of all conceptual boundaries. |
| Strict token duplication | 4.48%; 7,872 duplicated lines, 856 clones | Rust under `crates`/`src`; inline tests and spec files remain included. |

No substantive source ranges were read in `typokat-core`, `typokat-diagnostics`, `typokat-library`,
`typokat-project-binder-probe`, or `typokat-session-batch`. Vendored library declarations and
generated assets were excluded. No performance claim, complete compatibility assessment, security
certification, full repeated/standalone parity, or randomized differential result follows from
this audit. Recon used tokei, git churn, Cargo metadata/tree, jscpd, and targeted searches; scc,
lizard, ast-grep, cargo-machete, cargo-modules, gitleaks, and osv-scanner were unavailable.
Function complexity was not measured.

### How to read priorities

All twelve retained static findings are **bounded maintenance opportunities**, not demonstrated
urgent correctness defects. The original broader P1 proposals were rejected or narrowed by review.
The transition regressions took precedence and were repaired first; the cleanups follow
Delete → Dedupe → Decompose → Decouple, one reviewable unit each, never mixed with semantic edits.

## Structural observations that do not justify a rewrite

The crates-only census contains 229 Rust files and 220,501 physical lines. Its non-test-named filter
leaves 104 files above 400 lines, 66 above 800 and 39 above 1,200; excluding spec/test-support files
changes the latter counts to 60 and 38, still not production-only LOC. `frontend.rs` intersperses
production and test items; neither its total nor its prefix before the first `cfg(test)` is a
production-size measurement. Large files increase navigation cost, but counting them is not an
ownership design. M8 provides a verified long production function and a specific smaller boundary;
the census does not authorize splitting every file to an arbitrary threshold.

The normal crate graph is acyclic. [ADR-0019](../decisions/0019-split-the-checker-into-layered-workspace-crates.md)
uses Cargo to enforce real dependency direction. The nine-re-export facade has consumers;
`typokat-surface` is an exhaustive OXC drift tripwire. Immutable layered containers preserve shared
prefixes and isolated suffixes. None was established as dead or ceremonial architecture.

`Pass` had 85 production fields and four test-only fields in the audited tree. Its `Deref` forwards
to construction drafts whose access panics after final publication. This is a real implicit phase
contract, but the cited mixed-phase caller returns correctly on the published branch. Global
forwarding removal was rejected as an unproved maintenance improvement: explicit spelling alone
keeps the same runtime precondition. No input-triggered panic was found.

Do not merge whole lowerers or iterative walkers based on token overlap. Infer rewriting retains
SCC-suffix identity taint; mapped rewriting intentionally memoizes completed partial clones.
Declared applications, instantiations, class capabilities and product recipes have distinct policies.
Keep those distinctions and the [binding invariants](../reference/invariants.md).

## Semantic transition findings from the as-const work (design history)

Reported at audit time, repaired in the shipped feature:

- **S1 — literal selection and publication mode could disagree.** A shared helper preserved a
  literal while each container producer separately regularized its widening mode, so a value could
  pass an immediate literal check and misbehave after a copy. Repaired by mode-bearing immutable
  identities and role-aware publication ([ADR-0031](../decisions/0031-preserve-literal-widening-in-immutable-type-identity.md),
  ADR-0035). Witness: `tests/cases/b44_as_const/contextual_container_literal_modes.ts`.
- **S2 — contextual expectation was used as the declared return.** An empty callback was accepted
  as `() => number`; a value-returning callback was rejected under contextual `() => void`. Repaired
  by native contextual P/R ownership (ADR-0039): context guides inference and never masquerades as
  an annotation or replaces the inferred result.
- **S3 — later phases needed literal information the earlier model erased.** Repaired by ADR-0031's
  representation plus checked literal results ([ADR-0041](../decisions/0041-retain-checked-literal-results-for-assignment-diagnostics.md)).
  Fixing representation loss did not authorize reproducing every tsc construction order; that is
  [backlog 120](../backlog/120-semantic-constraint-construction-order.md).
- **S4 — tests missed transitions between phases.** Open; owned by
  [backlog 130](../backlog/130-transition-regression-coverage.md).

The lesson that outlives the repairs: interfaces that allow one semantic decision to be applied by
halves, and rely on later consumers or review to repair the product, are the defect class to watch.

## Open research question — the ownership table

Not scheduled. Before any further semantic-boundary refactor, trace one literal through the real
public checker route and complete this table with exact symbols, inputs, mutations, outputs and
witnesses. For each row ask: can a consumer assemble an inconsistent combination today? If yes,
name the smallest existing owner that can make the decision coherently. Prefer returning an
already-complete mode-bearing `TypeId`; a new result record is justified only if independent
outputs really must travel together; never create a second provenance authority beside the type
store.

| Boundary | Must remain distinguishable | Prohibited shortcut |
|---|---|---|
| Expression → contextual check | actual source value, explicit annotation, contextual expectation, const-owned syntax | treat the expected type as the inferred source or declaration |
| Producer → published member/element/return | complete identity including literal mode; literal-owned readonly vs referenced value | preserve the value but leave contradictory widening state; deep-rewrite referenced containers |
| Published type → inference candidate | original mode-bearing contribution, structural path, applicable fixing policy | recover provenance from syntax or replace a symbolic source with a broad contextual bound |
| Candidate fixing → constraint check | final candidate under the selected policy, dependent constraints, exhaustion | validate one candidate then repair its identity after the query/call |
| Source → copy/assignment | source identity, declared target surface, assignment-expression result, target-flow transfer | assume all three assignment results are the raw RHS or a declaration reset |
| Candidate/loop trial → reporting/cache | exact observation environment, held effects, selected owner, completed dependencies | adopt rejected/trial diagnostics or cache provisional operands as final |

Constraints and context: [ADR-0032](../decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md)
(assignment distinctions) and [ADR-0033](../decisions/0033-separate-contextual-row-views-from-symbolic-signatures.md)
(contextual versus symbolic signatures). Neither is permission to pull its whole scope into a
repair.

## Rejected directions

Rejected by the audit or existing contracts; do not re-propose without new evidence:

- Replacing the hash-consed store, binder, query coordinator, or immutable publication model without
  evidence that one of those foundations causes an observed defect.
- Merging infer/mapped walkers or whole lowerers because their token shapes match.
- Treating a broad `Pass` or every 800-line file as sufficient evidence for a new phase framework;
  removing `Pass`'s `Deref` forwarding globally.
- Reconstructing literal provenance from syntax, adding a parallel graph/whole-result flag, or
  repairing inferred bindings after final constraint checking.
- Replaying bodies or rolling back all checker state to manage effects; deleting or deduplicating
  already committed diagnostics to make a contextual re-walk appear correct.
- Promoting general signature construction, row correlation, candidate fixing or return aggregation
  to an implicit prerequisite for bounded as-const; those have owners (69, 120, 122, 123).
- An identity-first redesign of the re-export notice (suppressing earlier alters admission;
  per-member suppression breaks declaration-wide replacement) — the shared formatter is the fix.
- A cross-crate test framework or generic callback layer for the guards; a new differential
  harness instead of extending the existing grammar.
- Claiming that a documented invariant is enforced merely because a review or text-search guard
  currently catches violations; conversely, replacing useful tests with documentation alone.

**Conclusion.** The static audit demonstrates maintenance debt but not a broken checker foundation.
The implementation report pointed to the more important defect class: interfaces that allow
inconsistent semantic products. That class was validated and repaired along the as-const path;
the remaining work is coverage (130) and bounded cleanup (125–129, 131).
