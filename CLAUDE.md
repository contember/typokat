# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

typokat — a from-scratch **TypeScript type checker in Rust**. A *checker, not a
compiler*: it parses, binds, and type-checks TS and reports `tsc`-style diagnostics.
Error codes mirror tsc (`TK2322` ≡ `TS2322`). Emit, JS runtime semantics, and module
resolution are out of scope **by design**; the goal is to preserve the type model.
M0–M22 are implemented. Coverage: [README.md](./README.md). Full design:
[ts-checker-architecture.md](./ts-checker-architecture.md).

## Commands

```sh
cargo run -- check path/to/file.ts          # check one file (exit 1 if diagnostics)
cargo test                                  # unit tests + conformance corpus
cargo test conformance                      # just the marker-driven conformance harness
cargo test <name>                           # a single unit test by name
cargo clippy --all-targets -- -D warnings   # must stay clean
```

The conformance corpus is a single `#[test]` that runs every *enabled* milestone
directory — toggle scope via the `MILESTONE_DIRS` table in `tests/conformance.rs`.

## Architecture (the big picture)

Pipeline in `src/driver.rs`: parse (via `oxc`) → bind → check. Four pillars (details
in [ts-checker-architecture.md](./ts-checker-architecture.md)):

- **Type store** (`src/types/`) — every type is a hash-consed `TypeId(u32)` into an
  arena (no `Rc<RefCell>`), so structural equality is an integer compare.
  Substitution-aware; identity-bearing property metadata is folded into the hash.
- **Binder** (`src/binder/`) — a scope graph with **multi-slot symbols** (value /
  type / namespace spaces), which is what lets a class be both a type and a value,
  and what nominal classes key on.
- **Relation engine** (`src/relate/`) — `is_assignable`, the CPU-heavy core. A 3×`u32`
  cache, an **assume-true-until-disproven cycle stack** for recursive types, and
  `Relation::No(ReasonChain)` (never a bare `bool`) so reporting runs the same path.
  Carries a cache-soundness fix (architecture §6.3) — regressing it drops errors
  order-dependently, the sharpest bug class in the project.
- **Statement checker** (`src/check/`) — a flow-sensitive interpreter (a narrowing
  environment that forks at `if`/`else`/`switch`) plus the generic **inference
  engine** (`infer`, a separate machine from the relation engine). Split into
  `checker/` submodules.

**Soundness > completeness**: when in doubt, over-report (the safe direction). Every
deliberate `tsc` divergence is documented in `tests/cases/README.md`.

## How work is done here (mandatory method)

Milestone-by-milestone, **spec-first** — the process in [HANDOFF.md](./HANDOFF.md) §1
is what kept the project sound, follow it exactly: write the fixture corpus first (the
acceptance spec) and commit it on its own, then implement, then run an **independent
adversarial review** (hunts false negatives, cross-checks against real `tsc --strict`).
Implementation goes through subagents; the leader supervises and commits. The
soundness/architecture **invariants you must not break** are in HANDOFF.md §2; the
roadmap (next: M23 unstructured-flow narrowing, M24 generic constraints, then the
type-level VM) in §3.

## Testing

Two layers:

1. **Conformance corpus (the spec)** — `tests/cases/mN_*/*.ts` fixtures carrying inline
   `// error[TK…]: substring` markers, diffed by `tests/conformance.rs`. Marker
   conventions, type-display rules, and every documented `tsc` divergence:
   [tests/cases/README.md](./tests/cases/README.md).
2. **Official TypeScript suite harness** — `tooling/official-suite/` runs typokat
   against the real microsoft/TypeScript conformance baselines as a triage dashboard
   plus a committed regression scoreboard (`run --check` exits 1 on any regression). It
   is a black-box harness (shells out to the prebuilt binary), independent of the
   checker build. Details: [tooling/official-suite/README.md](./tooling/official-suite/README.md).
