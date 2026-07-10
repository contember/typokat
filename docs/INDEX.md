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
- **M0–M33 shipped; 2026-07-10 soundness-review sprint shipped** (archived) — all ten
  verified review findings fixed (statements/loops/throw, switch-local scope,
  type-only exports, local overloads, any&never, intersection nominal origin, mapped
  recursion guard, keyof domain, annotation depth budget), the official-suite ratchet
  is diagnostic-identity based (re-baselined 345/1674), CI is checked in
  (`.github/workflows/ci.yml`, pinned toolchain), and the 1.0 plan is executable:
  [`backlog/completion-1.0.toml`](backlog/completion-1.0.toml) validated by
  `tests/manifest.rs`, with the pinned TS 6.0.3 lib audit
  ([`backlog/lib-audit-6.0.3.md`](backlog/lib-audit-6.0.3.md)) naming the es5
  blockers: `41` generic methods, `43` namespaces/merging, `70` this-parameter
  typing. Backlog `38` is GO (ADR-0003). **Now:** resume track A (`41` first) or run
  the `38` prelude slice.
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). The follow-up quick-wins sprint then closed `64` `34`
  `33` `54` `59`; the dedicated inference-policy sprint then closed `65`. The remaining C
  silent-FN tail is now `30` `56` `60` `62` `32` `21` `22` `66` `67` (`30` reclassified
  into it 2026-07-10 — a dropped-error under-report, not a safe FP).
