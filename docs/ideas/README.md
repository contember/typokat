# ideas

Research, proposals, half-formed thoughts — **no commitment**. One file per idea,
`kebab-case.md`.

An idea has exactly two exits: it **graduates** (becomes a `../backlog/` item or
gets pulled into a `../sprints/` plan) or it's **deleted**. It is never a place for
decided work or status.

<!-- index the ideas here, one line each -->
- [`phpstan-architecture-lessons.md`](phpstan-architecture-lessons.md) — PHPStan comparison
  survey + adversarial peer-review verdicts (adopted → backlog `17` Stage A; rejected
  proposals recorded so they aren't re-proposed).
- [`type-assertion-markers.md`](type-assertion-markers.md) — `// type:` corpus markers
  compared as interned `TypeId`s (string-compare form explicitly rejected).
- [`sota-checker-lessons.md`](sota-checker-lessons.md) — SOTA checker survey (tsgo, ty,
  Pyrefly, Flow, Sorbet, Hack, rustc, papers) + peer-review verdicts; reference shelf for
  the `14`–`17` scale era and profiling-gated relation/evaluator ideas.
*Shipped/graduated: `benchmark-harness` → `tooling/bench/`; `minimal-prelude-slice` →
the shipped backlog `38` sprint; `workspace-crate-split` →
[`sprint-2026-07-28-workspace-crate-split.md`](../sprints/sprint-2026-07-28-workspace-crate-split.md).*
