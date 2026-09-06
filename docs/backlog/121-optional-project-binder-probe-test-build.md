---
id: 121
title: Repair optional project-binder probe test compilation
blocked-by: []
---

# 121 — Repair optional project-binder probe test compilation

**Summary.** Restore the optional workspace probe's test build. Effort S; engineering maintenance,
not an `as const` compatibility requirement.

## Problem

`cargo test --workspace` enables the project-binder probe and fails to compile the test branch of
`crates/typokat-library/src/project_binder_projection.rs`: `normalized_import_export_rows` accesses
`BoundProjectBinder.normalized`, which is no longer a field. This was reproduced on clean
`cff41c56` before the source-reference changes. The default-member `cargo test` route is unaffected.

Reproduce without running the full suite:

```sh
cpu-lease run -n 2 -- cargo check -p typokat-library --features project-binder-probe --tests
```

## Approach / acceptance

Use the current binder projection owner to retain the probe's intended normalized import/export
comparison. Do not merely replace it with the less precise non-test placeholder count, remove its
assertions, or add a dummy field to the production binder. Pin the comparison contract first.
The command above and affected probe tests must pass; default-member tests and Clippy must remain
green. No checker semantic or library publication change is required by this issue.

## Touch points

`crates/typokat-library/src/project_binder_projection.rs`, the optional project-binder probe's test
fixtures and feature wiring. The [as-const sprint](../sprints/sprint-2026-08-17-as-const.md) records the
baseline reproduction.
