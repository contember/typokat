---
id: 115
title: Parse provider projects once through a prepared frontend product
blocked-by: []
---

# 115 — Parse provider projects once through a prepared frontend product

**Summary.** Replace collision preflight's second parser authority with a frontend-owned
`PreparedProject` consumed by the project-local binder and checker session. This is scheduled as
WU3 of the active
[`project-local library universe sprint`](../sprints/sprint-2026-08-22-project-local-library-universe.md).
Non-blocking development and runtime improvement, effort L.

## Problem

Every provider-backed `check_source`, `check_files`, and `check_project` call parses user source for
collision preflight in `crates/typokat-library/src/collision_preflight.rs`, then parses it again in
the frontend before checking. The official-suite batch pays this cost for every frame.

The two walks also divide ownership of parser failures, module classification, import-edge
accounting, and global-binding census. A syntax or frontend-policy change must preserve two
implementations and their failure ordering. The duplicate parse is not required by OXC thread
affinity: routing and frontend work already execute inside the same scoped large-stack worker.

## Approach / acceptance

Introduce a frontend-owned prepared product that remains inside one worker and owns the OXC
allocators/programs for its lifetime. It should provide the pre-semantic facts needed to construct
the project:

- canonical file/module inventory and source ordering;
- parse diagnostics and panic outcome;
- module classification and relative-import graph;
- global source-binding census used to assemble the project-local global scope;
- the existing auxiliary/frontend accounting products needed by the checker session.

Complete preparation before binder or semantic mutation, then pass the same programs and source
facts into the project-local binder. The candidate route has no collision classification. During
migration, the temporary test-only control may derive its routing receipt from the same prepared
facts. Preserve parse diagnostic precedence, panic precedence, report order, Bundler resolution
provenance, and the rule that ASTs never cross their worker/lifetime boundary.

Corpus acceptance covers ordinary and augmented global projects, parse errors, parser panic
controls, multi-file projects, path-order determinism, and all public checker entry points. Add a
parser invocation receipt or equivalent test witness proving one parse per source. Compare
official-batch wall time before and after, but do not make a performance threshold the correctness
gate.

While consolidating parser ownership, extract the exact duplicated auxiliary parser-diagnostic
conversion helper; do not unify otherwise distinct frontend lifecycles merely to remove that copy.

## Touch points

`crates/typokat-frontend/src/frontend.rs`, `crates/typokat-library/src/collision_preflight.rs`,
`crates/typokat-library/src/base.rs`, `crates/typokat-driver/src/driver.rs`, auxiliary parsing,
provider route tests, project tests, and official-suite batch instrumentation.

<!-- Origin: 2026-08-20 development-friction audit, finding P1-LIB-002. -->
