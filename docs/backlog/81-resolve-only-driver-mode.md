---
id: 81
title: Resolve-only driver mode (bind + resolve, no relation engine)
blocked-by: [./79-resolution-query-surface.md]
---

# 81 — resolve-only driver mode

**Summary.** A second driver over the same core that answers *"what does this resolve to?"* without
answering *"is this assignable?"* — skipping `is_assignable` and diagnostics entirely. Consumers
that want a code graph or go-to-definition (backlog [`80`](./80-pavouk-resolution-oracle.md), an
eventual LSP) need resolution, not checking, and resolution is the cheaper half.
**Deliberately low priority** — an optimization, not a capability. `79` alone already unblocks
every consumer; this only makes them fast.

## Problem

`driver.rs` has one mode: full check. But the relation engine is, by our own architecture doc, "the
CPU-heavy core" (`src/relate/`, architecture §6) — and a consumer building a code graph never asks
it a single question. It wants: bind, type the receiver of each member access enough to look the
member up, record the declaration it landed on. Assignability, excess-property checks, narrowing
for *diagnostic* purposes, and the whole `ReasonChain` reporting path are dead weight.

Running full `check_project` to extract a graph therefore pays a large constant for output that is
thrown away — the difference between a tool that can run in a pre-commit hook and one that cannot.

## Approach / acceptance

A `resolve` entry point beside `check`, sharing binder + type store + inference, that:

- binds the module graph and populates the resolution map from backlog `79`;
- performs **receiver typing and member lookup** — this still needs real inference (generics,
  `await`, contextual types), so it is *not* "the checker minus types"; it is the checker minus the
  **relation engine** and minus **diagnostic reporting**;
- never calls `is_assignable`, never builds a `ReasonChain`, never renders a type for a message;
- keeps the `incomplete[<surface-id>]` signal — a resolve-only run must still say what it could not
  resolve.

Where narrowing feeds *resolution* (a narrowed union changes which member you land on) it stays;
where it only feeds diagnostics it is skipped. Mapping that boundary precisely is the substance of
this item and should be spec'd before implementation — a wrong cut here silently changes which
declaration a site resolves to, which is the sharpest bug class this mode can have.

**Acceptance.** `resolve` and `check` produce **identical resolution maps** on the whole conformance
corpus (that equivalence is the correctness witness — the mode is only trustworthy if it cannot
resolve differently), and `resolve` is measurably faster on the backlog `80` corpus. Profile before
committing to the split: if the relation engine turns out not to dominate a resolution-shaped
workload, this item is not worth its complexity and should be **dropped** rather than shipped —
same profiling-gate discipline as [`ADR-0001`](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md).

## Touch points

`src/driver.rs` (second entry point), `crates/typokat-check/src/check/checker/` (a resolve-only path through member
access and calls), profiling harness. No changes to `src/relate/`.

<!-- Origin: pavouk/typokat integration design session, 2026-07-14. Explicitly ranked below 79/80:
     capability first, speed later. -->
