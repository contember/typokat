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
- [`sprints/sprint-2026-07-05-utility-types.md`](sprints/sprint-2026-07-05-utility-types.md) —
  backlog `12` / M28 (spec committed, implementation in progress).

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M27 + the soundness warm-ups shipped** — after the M24→M27 type-level run, the
  warm-ups sprint killed three silent-FN families (interface `extends` composition incl.
  class/alias bases, alias-cycle `TK2456` + legal-recursion reads incl. the canonical
  `Json` shape, negative literals); see [`archive/`](archive/README.md). Scoreboard:
  in-scope 500, clean-kept 168/215, error-exact 22/285, diag-recall 253/1659. **Next:
  utility types (`12` / M28)** — closes the type-level phase — then the `13` profiling
  gate and the real-world track (`14` lib.d.ts, `15` modules). Known-gap items: `24`–`27`,
  `30`–`32` (`31` object-literal contextual typing and `32` eager-keyof forward refs are
  the new review byproducts). Small warm-ups: `21`–`23`.
