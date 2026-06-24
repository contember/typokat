---
id: 14
title: lib.d.ts loading (the standard library)
blocked-by: [./09-conditional-types.md, ./10-mapped-types.md]
---

# 14 — `lib.d.ts` loading

**Summary.** The "mandatory core" (architecture §4) — unlocks checking real-world code. Big. Also
where parallelism **Stage 1** lands.

## Problem

Without `lib.d.ts`, `console`, array methods, `Promise`, etc. are absent, so most real code can't be
checked. The lib leans heavily on generics + conditional/mapped types, so those must land first.

## Approach / acceptance

Parse and load the standard `lib.d.ts` declarations into the type universe as a shared read-only
prelude. This is also where parallelism **Stage 1** lands — the shared read-only prelude across
per-file workers (architecture §8.2). Acceptance: fixtures using `console`, array methods, and
`Promise` check correctly against tsc.

## Touch points

`.d.ts` parsing/consumption; the shared prelude in the type universe (parallelism Stage 1 —
architecture §8.2).

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
