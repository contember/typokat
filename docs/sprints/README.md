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
- [`sprint-2026-07-25-checker-scaling.md`](sprint-2026-07-25-checker-scaling.md) —
  active: checker scaling on real code. Six complexity hunts plus a bisect found five nonlinearities,
  none in the type model; the batch removes them and adds the local-layer scan guard that would have
  caught them. Runs alongside the full-lib sprint — different files.
- [`sprint-2026-08-02-default-library-cutover-closure.md`](sprint-2026-08-02-default-library-cutover-closure.md) —
  active backlog `14` closure sprint: production and the authoritative four-row performance gate
  are shipped; WU7 independent review is **CONDITIONAL PASS** with zero HIGH/MEDIUM findings, and
  only exact `d1aa6d4` remote CI plus documentation lifecycle closure remain.
- [`sprint-2026-07-12-real-project-preview.md`](sprint-2026-07-12-real-project-preview.md) —
  paused at WU0's zero-threshold public-witness gate; no implementation started.
- [`sprint-2026-07-16-namespace-binder-refactor.md`](sprint-2026-07-16-namespace-binder-refactor.md) —
  planned behavior-preserving cleanup, not started; gated on the default-library closure sprint's
  lifecycle close, then requires a fresh HEAD re-verification before work starts.
