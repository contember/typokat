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
<!-- (none) -->

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M26 shipped** — M26 (mapped types: template-mapper node, modifier arithmetic, union
  distribution, TK2456) closed 2026-07-05 on top of M25 conditional types and M24 generic
  constraints (all three in one 24h push; see [`archive/`](archive/README.md)). Scoreboard:
  in-scope 495, clean-kept 166/211, diag-recall 250/1657. **Next: template literal types
  (`11` / M27)**, then utility types (`12`). Type-model gaps from review probes: `24` rest
  elements, `25` intersections, `26` cross-binder infer, `27` template-buried conditionals,
  `28` interface extends composition, `29` silent alias-cycle permissiveness. Small
  warm-ups: `21`–`23`.
