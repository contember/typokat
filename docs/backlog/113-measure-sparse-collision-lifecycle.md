---
id: 113
title: Measure and simplify the sparse collision lifecycle
blocked-by: []
---

# 113 — Measure and simplify the sparse collision lifecycle

**Summary.** Decide with current repeated-consumer evidence whether sparse collision replay earns
its cross-cutting complexity; if it does not, keep the frozen fast path for collision-free checks
and route collisions through complete-source compilation. Non-blocking architecture work, effort L.

## Problem

The default library has three semantic lifecycles:

1. a frozen shared base with an ordinary project suffix;
2. a frozen shared base with a private sparse collision epoch;
3. complete-source compilation for standalone checks.

The sparse route is a deliberate result of
[`ADR-0020`](../decisions/0020-build-source-native-sparse-collision-epochs.md), but it makes an
optional provider optimization a core invariant. The 2026-08-20 development-friction audit counted
526 layered/frozen/sparse references across 22 production files; `fork_sparse_delta` alone appeared
72 times across 13 files. A new binder row, semantic table, identity, event owner, cache boundary,
or replay-sensitive query must work with both ordinary suffixes and sparse prefix replacement.

[`ADR-0021`](../decisions/0021-use-complete-source-compilation-for-standalone-cli-checks.md) already
removed sparse replay from the standalone CLI lifecycle and explicitly deferred deletion of the
shared collision machinery to a separate lifecycle/API decision. The only obvious in-repository
production consumer that benefits from repeated shared-base checks is the official-suite batch.
The repository does not currently pin its shared/private/fallback route frequencies or compare the
sparse route with complete-source collision fallback at that lifecycle.

## Approach / acceptance

Instrument the repeated provider route without changing its verdicts. Record at least:

- shared, private, and complete-fallback counts for the pinned official-suite batch;
- collision owner/group closure size and changed semantic rows;
- batch wall time and peak RSS;
- base initialization time separately from per-project work;
- normalized diagnostics, incomplete records, library census, and semantic probes.

Build a bounded comparison route that retains `FrozenLibraryBase` for collision-free projects but
sends every colliding provider project through the existing complete-source compiler. Declare the
acceptable batch-time and RSS thresholds before running the comparison. Use collision, fanout,
global augmentation, class/namespace, record-order, randomized file-order, and shared/cold parity
witnesses.

This item has two acceptable outcomes:

1. **Delete.** If the comparison meets the declared thresholds, write an ADR that supersedes
   ADR-0020 and vertically remove `CollisionReplayPlan`, `PrivateCollisionEpoch`, sparse semantic
   overrides, private replay source retention, and replay-ownership mechanisms used only by this
   lifecycle. Preserve the surviving route's soundness witnesses.
2. **Retain with evidence.** If the comparison fails, record the measurements and the exact
   repeated-consumer requirement in a decision. Keep sparse replay provider-local and stop treating
   it as infrastructure that future lifecycles must reuse by default.

Do not delete the frozen collision-free base or replace the provider with eager full-library
compilation on every repeated check as part of this item.

## Touch points

`crates/typokat-library/src/base.rs`, `crates/typokat-library/src/provider.rs`,
`crates/typokat-check/src/check/checker/library_compiler/*.rs`, binder/type-store layered storage,
collision and replay specifications, the official-suite batch, full shared/cold parity, and a new
superseding ADR if the selected lifecycle changes.

<!-- Origin: 2026-08-20 development-friction audit and adversarial library-lifecycle review. -->
