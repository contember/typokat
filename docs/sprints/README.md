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
- [`sprint-2026-07-12-real-project-preview.md`](sprint-2026-07-12-real-project-preview.md) —
  paused at WU0's zero-threshold public-witness gate; no implementation started.
- [`sprint-2026-07-12-pre-lib-hardening.md`](sprint-2026-07-12-pre-lib-hardening.md) —
  active; WU1–WU5 shipped/decided `22`, `56`, `77`, `70`, and `13`; only WU6 final verification
  and sprint closure remain.
