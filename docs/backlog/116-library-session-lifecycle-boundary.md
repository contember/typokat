---
id: 116
title: Collapse library compilation behind one session lifecycle boundary
blocked-by: [./113-measure-sparse-collision-lifecycle.md, ./114-lazy-default-library-index-spike.md]
---

# 116 — Collapse library compilation behind one session lifecycle boundary

**Summary.** After the library lifecycle decisions settle, replace low-level public routing/runtime
states and compiler mode combinations with one high-level repeated-check session and explicit
internal products. Non-blocking architecture cleanup, effort L.

## Problem

The library compiler has one roughly 1,100-line core parameterized by partially independent replay
plan, record retention, evidence capture, and owner-site storage policies. Seven wrappers choose
combinations, and optional return products encode whether a result is valid in a given mode.

Lifecycle ownership is also divided across crates. `typokat-library` publicly re-exports low-level
base, route, private execution, provider initialization, and binder-continuation states so the
driver can coordinate them. Several are documentation-hidden but still public Rust API. The
profile/package concern, semantic compiler, provider cache, route decision, fallback execution, and
report assembly therefore have no single orchestration owner.

The active library-compiler decomposition sprint is layout-only and intentionally preserves these
semantics. Its result must be remeasured after items [`113`](113-measure-sparse-collision-lifecycle.md)
and [`114`](114-lazy-default-library-index-spike.md); source layout alone does not resolve this item.

## Approach / acceptance

Choose one lifecycle owner after the sparse/frozen/lazy decisions are recorded. For a supported
repeated-check API, expose a high-level `CheckerSession` (name not binding) with project check
operations rather than base and route state machines. Keep:

- packaged sources, exact profile validation, and census assets in `typokat-library`;
- semantic reservation, construction, publication, and cache authority in the checker;
- routing, worker ownership, and report assembly in one non-published orchestration layer.

Replace independent compiler flags and optional outputs with purpose-specific internal entry points
whose products make invalid combinations unrepresentable. Preserve separate test-only full replay,
record census, storage-layout negative controls, and complete-source parity oracles where their
validated lifecycle still exists.

Before changing public symbols, check for external consumers and record the 0.1 API compatibility
decision. Acceptance requires fewer low-level public lifecycle types, no crate dependency cycle,
unchanged provider/standalone diagnostics and route receipts, and the full surviving lifecycle
gates. Do not move semantic publication authority out of `Pass`.

Remove the duplicate 2.94 MB packaged-source ownership during this work: compilation should borrow
one validated table or retain handles/ordinals. If item 113 deletes sparse replay, delete its retained
private source registry rather than generalizing it.

## Touch points

`crates/typokat-library/src/lib.rs`, `base.rs`, `provider.rs`, and `compiler.rs`;
`crates/typokat-driver/src/driver.rs`; checker library compilation entry points; package/release
tests; examples; public API documentation; and lifecycle parity specifications.

<!-- Origin: 2026-08-20 development-friction audit, findings P1-LIB-003/004 and P2-LIB-001. -->
