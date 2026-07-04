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
- [`sprints/sprint-2026-07-04-unstructured-flow-narrowing.md`](sprints/sprint-2026-07-04-unstructured-flow-narrowing.md) —
  backlog `07` / M23 (the flow-node CFG; spec committed, implementation in progress).

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M22 + class completeness (backlog `06`) + constructor accessibility (`20`) shipped** —
  `TK2416`/`TK2515`/`TK2654`/`TK2673`/`TK2674`, plus the official-suite header-alignment harness
  fix (see [`archive/sprint-2026-07-04-class-completeness-ctor-accessibility.md`](archive/sprint-2026-07-04-class-completeness-ctor-accessibility.md)).
  **In flight: unstructured-flow narrowing (`07` / M23)** — spec committed, implementation next.
  Then: generic constraints (`08` / M24) → the type-level evaluation phase (`09`–`12`). New gaps
  filed from reviews: `21` local classes, `22` new-callee forms, `23` static method type params.
