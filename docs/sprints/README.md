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
- [`sprint-2026-08-17-as-const.md`](sprint-2026-08-17-as-const.md) — ordered second; resolve the
  approved bounded parity slice, implement AST-directed const contexts, and compare with PR #29510.
- [`sprint-2026-08-15-library-compiler-decomposition.md`](sprint-2026-08-15-library-compiler-decomposition.md) —
  active move-only decomposition of the `checker::library_compiler` hotspot behind its stable
  facade, with a frozen full-token manifest and lifecycle closure gates.
