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
- **M0–M25 shipped** — M25 (conditional types: the evaluator with work-stack/memo/budget,
  `infer` via a non-widening inference mode, distribution, TK2456/TK2589) closed 2026-07-04
  and **opens the type-level evaluation phase**; M24 (generic constraints) same day. See
  [`archive/`](archive/README.md). Scoreboard: in-scope 491, clean-kept 164/209, diag-recall
  250/1651 (conditional-heavy suite files still gated by mapped/template/variadic buckets —
  signal arrives with `10`–`12`/`24`). **Next: mapped types (`10` / M26)**, then `11`–`12`.
  Type-model gaps from probes: `24` rest elements, `25` intersections, `26` cross-binder
  infer, `27` template-buried conditionals. Small warm-ups: `21`–`23`.
