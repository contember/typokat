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
- [`sprints/sprint-2026-07-04-class-completeness-ctor-accessibility.md`](sprints/sprint-2026-07-04-class-completeness-ctor-accessibility.md) —
  backlog `06` + `20` (class completeness + constructor accessibility).

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **M0–M22 shipped; object/interface method, call & construct signatures shipped** (backlog `05` / F1 —
  see [`archive/sprint-2026-06-28-object-interface-signatures.md`](archive/sprint-2026-06-28-object-interface-signatures.md);
  earlier current-impl bugs F3–F5 in [`archive/sprint-2026-06-24-impl-bugs.md`](archive/sprint-2026-06-24-impl-bugs.md)).
  Next likely: class-completeness warm-up (`06`) or unstructured-flow narrowing (`07`). New gaps
  filed: `18` duplicate-identifier, `19` not-callable, `20` constructor accessibility on `new`.
