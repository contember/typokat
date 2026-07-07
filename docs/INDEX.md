# typokat docs — index

The map of everything under `docs/`. Read [`CLAUDE.md`](CLAUDE.md) for the rules.
When sources disagree, precedence is: invariants/hard-rules → active sprint →
decisions → reference → archive.

## Folders

- [`reference/`](reference/README.md) — how the system works now (architecture, dev method, invariants, scope map).
- [`ideas/`](ideas/README.md) — proposals, no commitment.
- [`decisions/`](decisions/README.md) — ADRs (the *why*), immutable.
- [`backlog/`](backlog/README.md) — decided work, not yet scheduled (the roadmap).
- [`sprints/`](sprints/README.md) — active thematic work-plans.
- [`archive/`](archive/README.md) — shipped sprints + reference-worthy records.

> Two code-adjacent docs stay **outside** `docs/`: [`tests/cases/README.md`](../tests/cases/README.md)
> (conformance corpus + marker conventions) and
> [`tooling/official-suite/README.md`](../tooling/official-suite/README.md) (the official-suite
> harness). The public [`README.md`](../README.md) and the hard-rules
> [`CLAUDE.md`](../CLAUDE.md) live at the repo root.

## Active sprints

<!-- list the sprint files currently in sprints/ ; empty between sprints -->
- [`sprint-2026-07-07-intersection-types.md`](sprints/sprint-2026-07-07-intersection-types.md) — backlog `25` / M31: intersection types (`A & B`). Spec committed; implementation next.

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M30 shipped; no active sprint.** The type-level evaluation phase is complete (M24–M28);
  M29 shipped backlog `15` slice 1 (serial local-relative modules); M30 shipped contextual
  literal typing. Scoreboard: in-scope 506, clean-kept 172/219, error-exact 22/287,
  diag-recall 285/1691 (official `enum`/`namespace`/`satisfies`/`as const`/`module` gates
  still closed; unchanged by the FN-fix sprint — its added diagnostics land on corpus not in
  the committed scoreboard). The completion roadmap (refined 2026-07-07) lives in the backlog
  README: a definition of done (checker 1.0) + four tracks (A model completeness `24` `25`
  `39`–`44` = the `lib.d.ts` critical path; B checker completeness `18` `19` `45`–`52`;
  C known-gap tail; D scale ladder `38` `13` `14` `15` `16` `17`).
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). **Next:** the remaining C silent-FN tail
  (`56` `60` `62` `65` `54` `59` `64` `33` `34` `32` `21` `22`), then track A toward `14`.
