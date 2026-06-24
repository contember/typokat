---
id: 15
title: Modules / imports / module resolution (whole-repo checking)
blocked-by: []
---

# 15 — Modules / imports / module resolution

**Summary.** Whole-repo checking; also where I/O cost dominates. This is parallelism **Stage 2**.

## Problem

typokat checks a single file. Real projects need modules, imports, and module resolution to check
across file boundaries.

## Approach / acceptance

Implement module resolution + import/export wiring for whole-repo checking. Crossing the file
boundary is parallelism **Stage 2**: cross-file type identity via the stable structural hash, or a
shared *growing* interner (the §3.4 knot — architecture §8.2). Acceptance: a multi-file fixture with
imports/exports checks correctly, and cross-file type identity holds.

## Touch points

Module resolution + import/export binding; cross-file type identity (stable structural hash or shared
interner — architecture §8.2).

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
