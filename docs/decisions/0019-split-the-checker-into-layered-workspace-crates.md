---
id: 0019
title: Split the checker into layered workspace crates
status: accepted
date: 2026-07-28
---

# 0019 — Split the checker into layered workspace crates

## Context

The single `typokat` library contains clear architecture layers, but Rust cannot
enforce their dependency direction while they remain modules of one crate. A
measured survey found that most cross-layer use is already acyclic and that the
visibility migration is mechanical. It also found that a workspace split will not
materially improve the common edit loop because `check` remains the dominant
compilation unit; enforced layering, not build time, is the reason to split.

The original survey missed one production cycle: `check::library_compiler` calls
the project frontend in `driver`, while `driver` calls `check`. Moving the plain
input record alone cannot cut that cycle. Preserving the current public Rust API
is not a project requirement, so the architecture should express ownership
directly instead of adding compatibility adapters.

## Decision

We will keep the root `typokat` package as the CLI and thin facade over ten
workspace members:

`typokat-core`, `typokat-types`, `typokat-binder`, `typokat-relate`,
`typokat-diagnostics`, `typokat-surface`, `typokat-frontend`, `typokat-check`,
`typokat-library`, and `typokat-driver`.

`typokat-frontend` owns raw project inputs, parser allocation, import scanning and
dependency ordering, and the borrowed project-program representation consumed by
the checker. Both `typokat-check` and `typokat-driver` depend on it; neither calls
upward into the other.

The remaining dependency direction is:

```text
core                 surface
  ↓
types
  ├──→ binder ──┐
  └──→ relate ──┼──→ diagnostics
                └──→ frontend
                         ↓
                       check
                         ↓
                      library
                         ↓
                       driver
```

An arrow means “is depended on by”; direct dependencies may skip layers but must
never point upward. `class_semantics` belongs to `typokat-types`. The pinned
TypeScript library sources belong to `typokat-library`. Workspace dependencies
and lints are declared centrally. The root package retains `src/lib.rs`,
`src/main.rs`, the integration corpus, and tooling entry points.

Cross-crate items are public only because Rust requires that visibility; each
member keeps modules private where possible and re-exports the smallest surface
needed by its dependants. These internal crate APIs carry no stability promise.

## Consequences

Cargo enforces the architecture graph and can build or test an individual layer.
Wrong-direction imports become manifest cycles or fail the workspace layering
test. Member tests need a workspace-root helper instead of assuming that
`CARGO_MANIFEST_DIR` is the repository root, and source-introspecting tests and
tooling paths must follow their new owners.

The migration promotes several hundred formerly crate-private paths to public
cross-crate APIs. This is intentional internal exposure, not a crates.io semver
commitment.

The common `check` edit remains a large compilation unit, so this decision does
not claim a build-time improvement. Splitting `check` further remains a separate,
profiling-gated refactor.

## Alternatives considered

A virtual workspace manifest would remove the stable root package used by the CLI,
integration tests, and tooling, so it was rejected.

Moving project parsing into `typokat-check` would cut the missed cycle with fewer
crates but would make the semantic checker own physical frontend orchestration.
Keeping a dedicated frontend layer preserves that boundary.

Injecting callbacks or compatibility shims into the existing `library` API would
retain the accidental cycle in the conceptual design. Public API compatibility is
not valuable enough to justify that indirection.
