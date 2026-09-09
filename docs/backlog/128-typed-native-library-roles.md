---
id: 128
title: Replace the hand-synchronized eight-slot native library role protocol with a typed role
blocked-by: []
---

# 128 — Replace the hand-synchronized eight-slot native library role protocol with a typed role

**Summary.** Native library roles (`String = 2`, `Object = 6`, `CallableFunction = 7`, …) are
positional agreements across four typed components that the compiler does not enforce. Graduated
from the maintainability audit in
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(finding M5). Effort M; non-blocking maintenance. Re-verified on `main` `81147d86` (2026-09-09).

## Problem

The ordered role names live in
`crates/typokat-check/src/check/checker/project_local/entry.rs:519-524`, indexed selection in
`crates/typokat-check/src/check/checker/library_identities.rs:115-125`, and numerical demand routing
(`NativeSourceSurface::String => 2`, `::Function => 7`) in
`crates/typokat-check/src/check/query/mod.rs:3273-3277`; the plan and request carry the result as
`library_identity_groups: [Option<TypeGroupId>; 8]`. These indexes are role positions, not
`TypeId`s or serialized keys. A producer reorder compiles and selects the wrong demand target. No
current wrong mapping was established.

## Approach / acceptance

One small role enum with a canonical name/order mapping; keep the compact array representation
behind typed accessors. Match the existing named `NativeSourceSurface` variants directly instead of
encoding and decoding numbers. Preserve exact global binding authentication and project-local
identities; do not broaden the supported native surfaces.

Acceptance: cover every role, missing/unavailable roles, same-spelled user bindings, and the
existing query/exhaustion paths with the current witnesses. Add one negative control that a
reordered producer no longer compiles or is rejected at construction.

Gates: `cargo test`, `cargo clippy --all-targets -- -D warnings`, ordinary exact conformance with
the current receipt. No fixture or scoreboard movement is expected.

## Touch points

`crates/typokat-check/src/check/checker/project_local/entry.rs`,
`crates/typokat-check/src/check/checker/library_identities.rs`,
`crates/typokat-check/src/check/query/mod.rs`, the plan/request carriers in
`crates/typokat-check/src/check/checker/mod.rs` and
`crates/typokat-check/src/check/checker/project_local/project_local_pass_port.rs` (after
[126](126-pass-request-and-manifest-ownership.md) lands, only one carrier remains).

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->
