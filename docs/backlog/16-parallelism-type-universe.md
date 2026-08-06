---
id: 16
title: Parallelism — shared type universe (Stages 1 & 2)
blocked-by: [./15-modules-imports.md]
---

# 16 — Parallelism: shared type universe (Stages 1 & 2)

**Summary.** The per-file driver and shared read-only default-library base already ship (Stages 0
and 1); what remains is hardening the shared *type universe* after modules exist. Architecture §8. **Ownership boundary
(WU7):** this item is the **sole** owner of parallel **cross-file type identity** (Stage 2);
backlog [`15`](./15-modules-imports.md) owns serial resolver breadth and does not duplicate
Stage 2.

## Problem

`driver::check_files` already fans each file's *whole* pipeline (parse→bind→check) across rayon
workers, each with its own arena + interner. This shape is **forced, not chosen**: the oxc AST is
thread-pinned (`!Send + !Sync` — arena `Vec` is `!Send`, nodes hold `Cell`s), so the unit of
parallelism is the per-file pipeline — **not** parse+bind feeding a shared serial checker (that
sketch is wrong: the AST can never reach the checker). It is lossless today only because nothing
crosses a file boundary. Item `15` may first ship a correctness-first whole-repo slice without the
final parallel type-universe strategy; this item is the hardening step that makes cross-file sharing
work under per-file parallel execution.

## Approach / acceptance

- **Stage 1 (shipped)** — the shared read-only default-library base replaced the earlier minimal
  production prelude.
- **Stage 2** — cross-file type identity for the parallel module checker (lands after the
  correctness-first module slice from item 15): the stable structural hash, or a shared *growing*
  interner (the §3.4 knot).

parse+bind stays per-file-parallel and interner-free forever. Acceptance: multi-file checking shares
the prelude + cross-file types across workers without losing type identity, and stays correct under
parallel execution — **deterministically**: identical diagnostics (order and rendered text, including
union display order) across runs regardless of worker count or schedule (the tsgo lesson — it shipped
different errors between runs off encounter-order type ids). Corollary: the stable hash must define a
**content-based canonical member order** — hashing unions over run-local TypeId-sorted members would
make structurally identical types hash differently across workers. (Peer-reviewed 2026-07-09; see
`docs/ideas/sota-checker-lessons.md`.)

## Touch points

The type universe (shared prelude + cross-file identity); `driver::check_files`; the stable structural
hash (`crates/typokat-types/src/types/hash.rs`).

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE; architecture §8). -->
