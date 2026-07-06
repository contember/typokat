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
- [`sprints/sprint-2026-07-06-contextual-literals.md`](sprints/sprint-2026-07-06-contextual-literals.md) —
  M30 contextual typing of fresh literals.

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M29 shipped; M30 contextual literals active.** M28 completed the type-level
  evaluation phase (prelude utilities, deferred `keyof`, modifier-preserving Pick, recursive
  mapped-alias seeding, string intrinsics). M29 ships backlog `15` slice 1: correctness-first
  local-relative modules and cross-file named imports/exports in one serial type universe;
  parallel Stage 2 identity and full resolver semantics remain deferred. Scoreboard stays
  in-scope 506, clean-kept 171/219, error-exact 22/287, diag-recall 285/1691 (official module
  gates remain closed). **Now:** backlog `31` / M30 target-aware contextual typing for fresh
  object, array, and tuple literals. **Next:** the `13` profiling gate, then `14` full `lib.d.ts` /
  parallelism Stage 1, then backlog `15` slice 2 / `16` cross-file identity. Known-gap items:
  `24`–`27`, `30`–`37` (`33`–`37` are the M28 review byproducts; `31` object-literal contextual
  typing unblocks literal-heavy corpora). Small warm-ups: `21`–`23`.
