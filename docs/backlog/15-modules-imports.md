---
id: 15
title: Modules / imports / module resolution (whole-repo checking)
blocked-by: []
---

# 15 — Modules / imports / module resolution

**Summary.** Module-resolver **breadth** — whole-repo checking in a **serial** type universe;
also where I/O cost dominates. The correctness-first local-relative slice shipped as M29; the
remaining work here is full resolver breadth. **Ownership boundary (WU7):** this item owns
resolver breadth only. Parallel cross-file type identity (Stage 2) is owned solely by backlog
[`16`](./16-parallelism-type-universe.md) — do not duplicate it here.

## Problem

M29 added a serial `check_project` path for provided local relative `.ts` files with named
imports/exports. Real projects still need package/tsconfig resolver breadth: `node_modules`
packages, `tsconfig` resolver options (`paths`/`baseUrl`/`moduleResolution`), `.d.ts`
consumption, default/namespace/star imports, re-exports, and (guarded) module cycles.

## Approach / acceptance

Extend the serial `check_project` path with resolver breadth in one type universe:

- **Correctness-first whole-repo slice (shipped as M29):** local relative module resolution +
  import/export wiring runs serially. Acceptance: multi-file fixtures with imports/exports check
  correctly.
- **Resolver breadth (this item's remaining work):** package/`tsconfig`/`.d.ts` resolution and
  the wider import/export forms, still **serial**. Acceptance: fixtures resolving packages and
  tsconfig-mapped paths check correctly vs tsc.

Crossing file boundaries under **parallel** execution — cross-file type identity via the stable
structural hash or a shared growing interner (§3.4 knot, architecture §8.2) — is **not** this
item; it is backlog `16` Stage 2. If resolver breadth ships while checking is still serial,
that is correct-but-serial by design (document it as such); the parallel type-universe strategy
lands with `16`.

## Touch points

Module resolution + import/export binding (package/tsconfig/`.d.ts`); the serial `check_project`
path. (Cross-file identity and `driver::check_files` are backlog `16`.)

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
