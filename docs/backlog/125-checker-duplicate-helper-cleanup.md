---
id: 125
title: Delete unused dev-dependencies and dedupe three checker helpers
blocked-by: []
---

# 125 — Delete unused dev-dependencies and dedupe three checker helpers

**Summary.** Three independently reviewed, behaviour-neutral cleanups graduated from the
maintainability audit in
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(findings M12, M6, M7). Effort S each; non-blocking maintenance. Re-verified present on
`main` after the as-const sprint closed (2026-09-09).

## Problem

- **M12** — `sha2` is a direct dev-dependency of `crates/typokat-driver/Cargo.toml` and root
  `Cargo.toml` declares a Unix-only `rustix` dev-dependency; neither has a consumer in its owning
  package's targets.
- **M6** — `construction_alias_surface` (`checker/annotations/composites.rs`) and
  `class_interface_alias_surface` (`checker/decls/mod.rs`) have byte-identical bodies; one
  construction-time shadowing rule is maintained twice.
- **M7** — `tuple_rest_container_is_array_like` exists three times
  (`annotations/composites.rs`, `classes/type_syntax.rs`, `project_local/lowerer.rs`), differing only
  in store access and visited-set representation.

## Approach / acceptance

Land as three separate commits in this order, each with the unchanged-behaviour witnesses named:

1. Delete only the two unused dev-dependency entries; relevant test builds (including Unix root
   tests) and the normal workspace gates pass. Do not remove feature-activating edges.
2. Delete `class_interface_alias_surface` and reuse the existing checker-visible helper; local
   ready/unavailable versus inherited aliases, cycles and class/interface heritage keep exact outcomes.
3. One pure helper over `&Store` and `TypeId` for the tuple-rest predicate; keep infer declaration
   handling, diagnostic ownership and the project-local recipe fallback outside it. Readonly wrappers,
   parameter chains/cycles, absent constraints, arrays and tuples keep exact outcomes. Do not merge
   the lowerers.

Gates: `cargo test`, `cargo clippy --all-targets -- -D warnings`, ordinary exact conformance with the
current receipt. No fixture or scoreboard movement is expected; any movement is a finding.

## Touch points

`Cargo.toml`, `crates/typokat-driver/Cargo.toml`,
`crates/typokat-check/src/check/checker/{annotations/composites.rs,decls/mod.rs,classes/type_syntax.rs,project_local/lowerer.rs}`.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->
