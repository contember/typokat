---
id: 117
title: Add a focused marker-aware conformance edit loop
blocked-by: []
---

# 117 — Add a focused marker-aware conformance edit loop

**Summary.** Run one exact logical fixture or corpus through the real marker-aware conformance
harness without replacing the central inventory or weakening the full closure gate. Non-blocking
development tooling, effort S.

## Problem

`tests/conformance.rs` owns the accepted central inventory, project grouping, marker comparison,
route receipts, and frozen ordering fingerprint. Its ordinary test always checks the full inventory,
so fixture-driven development jumps from manual CLI probing to a roughly two-to-three-minute gate.
There is no exact case or corpus selector that preserves multi-file grouping and marker semantics.

Moving registration into per-corpus manifests is not the fix. The central table deliberately makes
the spec-first `false -> true` enablement change visible and distinguishes whole directories,
selected fixtures, and multi-file projects. Directory metadata would spread that authority across
more than one hundred files while still requiring a global order lock.

## Approach / acceptance

Add an exact selector over the already discovered logical inventory. The concrete CLI shape may use
test arguments or a small repository-owned harness, but it must support:

- one exact logical case, including all files of a multi-file project;
- one registered corpus;
- explicit handling of disabled fixtures, with rejection by default;
- deterministic ambiguity and unknown-selector errors;
- the same parser, marker comparison, type-display, and driver route as ordinary conformance;
- a receipt clearly labelled as focused and insufficient for closure.

Keep `conformance_inventory_matches_frozen_receipt` and the ordinary full inventory unchanged as the
only PR/closure authority. Add unit/integration witnesses for single-file, multi-file, disabled,
unknown, and corpus selection.

As a local robustness fix, stop unsynchronized process-global panic-hook mutation in marker-parser
tests. Prefer structured parse errors; if panic compatibility must remain, serialize replacement
and restore it through RAII.

## Touch points

`tests/conformance.rs`, marker parser tests/helpers, `tests/cases/README.md`, and contributor command
documentation. No fixture-registration migration.

<!-- Origin: 2026-08-20 development-friction audit and adversarial test-loop review. -->
