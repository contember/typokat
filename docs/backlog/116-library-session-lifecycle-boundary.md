---
id: 116
title: Collapse library compilation behind one session lifecycle boundary
blocked-by: []
---

# 116 — Collapse library compilation behind one session lifecycle boundary

**Summary.** Replace low-level public routing/runtime states and compiler mode combinations with
one high-level project-local checker session over the immutable declaration product. This is
scheduled as WU5 of the active
[`project-local library universe sprint`](../sprints/sprint-2026-08-22-project-local-library-universe.md).
Non-blocking architecture work, effort L.

## Problem

The library compiler has one roughly 1,100-line core parameterized by partially independent replay
plan, record retention, evidence capture, and owner-site storage policies. Seven wrappers choose
combinations, and optional return products encode whether a result is valid in a given mode.

Lifecycle ownership is also divided across crates. `typokat-library` publicly re-exports low-level
base, route, private execution, provider initialization, and binder-continuation states so the
driver can coordinate them. Several are documentation-hidden but still public Rust API. The
profile/package concern, semantic compiler, provider cache, route decision, fallback execution, and
report assembly therefore have no single orchestration owner.

The shipped library-compiler decomposition sprint was layout-only and intentionally preserved these
semantics. This boundary is now scheduled as WU5 of item
[`113`](113-project-local-library-universe.md); source layout alone does not resolve it.

## Approach / acceptance

Expose a high-level `CheckerSession` (name not binding) with project check operations rather than
base and route state machines. Keep:

- packaged sources, the immutable declaration product, exact profile validation, and census assets
  in `typokat-library`;
- semantic reservation, construction, publication, and cache authority in the checker;
- routing, worker ownership, and report assembly in one non-published orchestration layer.

Replace independent compiler flags and optional outputs with purpose-specific internal entry points
whose products make invalid combinations unrepresentable. Preserve a temporary test-only old-route
parity oracle, the record census, and necessary negative controls until cutover; then delete frozen,
sparse, and complete-source-fallback lifecycle states rather than exposing them through the session.

Before changing public symbols, check for external consumers and record the 0.1 API compatibility
decision. Acceptance requires no low-level public lifecycle types, no crate dependency cycle,
project-local provider/standalone semantic parity, and all sprint gates. Do not move semantic
publication authority out of `Pass`.

Remove the duplicate 2.94 MB packaged-source ownership during this work: product construction should
borrow one validated source table or retain handles/ordinals. Delete the retained private replay
source registry rather than generalizing it.

## Touch points

`crates/typokat-library/src/lib.rs`, `base.rs`, `provider.rs`, and `compiler.rs`;
`crates/typokat-driver/src/driver.rs`; checker library compilation entry points; package/release
tests; examples; public API documentation; and lifecycle parity specifications.

<!-- Origin: 2026-08-20 development-friction audit, findings P1-LIB-003/004 and P2-LIB-001. -->
