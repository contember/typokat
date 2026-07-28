# sprints

Active thematic work-plans being executed **now**. One file per sprint:
`sprint-YYYY-MM-DD-<theme>.md` (copy [`_template.md`](_template.md)). Multiple may
be active at once.

A sprint is the unit of work. Running it unattended ("do everything autonomously")
is just a sprint with no human watching — same file, same rules.

Lifecycle (full detail in [`../CLAUDE.md`](../CLAUDE.md)):

1. **Create** — copy the template; re-verify load-bearing facts at HEAD before
   planning on them.
2. **Run** — work the WUs; append discoveries/blockers to `## Run log`; graduate
   entries to a `../decisions/` ADR or a `../backlog/` item.
3. **Close** — stamp the `OUTCOME` header, `git mv` to `../archive/`, delete/rescope
   consumed backlog items, refresh affected `../reference/`, update
   [`../INDEX.md`](../INDEX.md).

## Active

<!-- one line per active sprint; empty between sprints -->
- [`sprint-2026-07-28-workspace-crate-split.md`](sprint-2026-07-28-workspace-crate-split.md) —
  active: split the monolithic library into ten enforced workspace layers behind the root facade,
  including a dedicated frontend crate that removes the real check/driver cycle.
- [`sprint-2026-07-25-checker-scaling.md`](sprint-2026-07-25-checker-scaling.md) —
  active: checker scaling on real code. Six complexity hunts plus a bisect found five nonlinearities,
  none in the type model; the batch removes them and adds the local-layer scan guard that would have
  caught them. Runs alongside the full-lib sprint — different files.
- [`sprint-2026-07-21-full-lib-performance-cutover.md`](sprint-2026-07-21-full-lib-performance-cutover.md) —
  active backlog `14` delivery sprint: exact TypeScript 6.0.3 full-host base, production cutover,
  and a fail-closed fresh-process target of at least 2× native TypeScript 7 on every approved row.
- [`sprint-2026-07-12-real-project-preview.md`](sprint-2026-07-12-real-project-preview.md) —
  paused at WU0's zero-threshold public-witness gate; no implementation started.
- [`sprint-2026-07-16-namespace-binder-refactor.md`](sprint-2026-07-16-namespace-binder-refactor.md) —
  planned behavior-preserving cleanup, not started; refs re-verified 2026-07-22 and now gated on
  the full-lib performance sprint closing (it rewrites the same three files).
