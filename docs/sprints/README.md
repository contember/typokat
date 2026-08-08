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
- [`sprint-2026-08-08-acyclic-source-reexports.md`](sprint-2026-08-08-acyclic-source-reexports.md) —
  active: admit only acyclic local named source re-exports through the existing Bundler resolver and
  direct value/type slot projection; defaults, namespaces, stars, cycles, packages, and the public
  witness remain deferred.
- [`sprint-2026-07-25-checker-scaling.md`](sprint-2026-07-25-checker-scaling.md) —
  active: checker scaling on real code. Six complexity hunts plus a bisect found five nonlinearities,
  none in the type model; the batch removes them and adds the local-layer scan guard that would have
  caught them. It previously ran alongside the now-archived full-library sprint in different files.
- [`sprint-2026-07-16-namespace-binder-refactor.md`](sprint-2026-07-16-namespace-binder-refactor.md) —
  planned behavior-preserving cleanup, now unblocked by the default-library closure; requires a
  fresh HEAD re-verification before WU1 starts.
