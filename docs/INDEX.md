# typokat docs — index

The map of everything under `docs/`. Read [`CLAUDE.md`](CLAUDE.md) for the rules.
When sources disagree, precedence is: invariants/hard-rules → active sprint →
decisions → reference → archive.

## Folders

- [`reference/`](reference/README.md) — how the system works now (architecture, dev method, invariants, scope map, `tsc` divergences).
- [`ideas/`](ideas/README.md) — proposals, no commitment.
- [`decisions/`](decisions/README.md) — ADRs (the *why*), immutable.
- [`backlog/`](backlog/README.md) — decided work, not yet scheduled (the roadmap).
- [`sprints/`](sprints/README.md) — active thematic work-plans.
- [`archive/`](archive/README.md) — shipped sprints + reference-worthy records.

> Two code-adjacent docs stay **outside** `docs/`: [`tests/cases/README.md`](../tests/cases/README.md)
> (how to write/read the conformance fixtures — marker conventions, **not** the divergence ledger,
> which lives in [`reference/divergences.md`](reference/divergences.md)) and
> [`tooling/official-suite/README.md`](../tooling/official-suite/README.md) (the official-suite
> harness). The public [`README.md`](../README.md) and the hard-rules
> [`CLAUDE.md`](../CLAUDE.md) live at the repo root.

## Active sprints

<!-- list the sprint files currently in sprints/ ; empty between sprints -->

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M32 shipped; signature shape shipped.** The type-level evaluation phase is complete (M24–M28);
  M29 shipped backlog `15` slice 1 (serial local-relative modules); M30 shipped contextual
  literal typing; M31 shipped intersection types (`A & B`) — backlog `25`, the first track-A
  step; M32 shipped backlog `24` rest elements and `39` optional/default parameters; the
  2026-07-08 soundness-tail sprint shipped `64` `34` `33` `54` `59` and deferred `65`
  to a dedicated inference-policy sprint. The completion roadmap (refined 2026-07-07) lives in the backlog
  README: a definition of done (checker 1.0) + four tracks (A model completeness `40`–`44`
  = the remaining `lib.d.ts` critical path; B checker completeness `18` `19` `45`–`52`;
  C known-gap tail; D scale ladder `38` `13` `14` `15` `16` `17`). **Now:** schedule `65`
  separately when inference candidate priority/variance is the sprint theme, and continue track A
  with overloads/generic methods/namespaces before full `lib.d.ts`.
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). The follow-up quick-wins sprint then closed `64` `34`
  `33` `54` `59`; the remaining C silent-FN tail is now `56` `60` `62` `65` `32` `21` `22`
  `66` `67`, with `65` explicitly policy-heavy.
