---
id: 14
title: Full lib.d.ts loading (the standard library)
---

# 14 — full `lib.d.ts` loading

**Summary.** The "mandatory core" (architecture §4) — unlocks checking real-world code. Big. Also
where parallelism **Stage 1** lands. This item is the **full** standard-library load; an earlier
minimal ambient/prelude slice is allowed before this item when it buys useful real-world feedback.

## Problem

Without `lib.d.ts`, `console`, array methods, `Promise`, etc. are absent, so most real code can't be
checked. The lib leans heavily on generics + conditional/mapped types, so those must land first.
That dependency applies to the full library. A deliberately small prelude fixture set may be loaded
earlier if it avoids conditional/mapped-heavy declarations and does not pretend to be full
`lib.d.ts` support.

## Approach / acceptance

Parse and load the standard `lib.d.ts` declarations into the type universe as a shared read-only
prelude. This is also where parallelism **Stage 1** lands — the shared read-only prelude across
per-file workers (architecture §8.2). Acceptance: fixtures using `console`, array methods, and
`Promise` check correctly against tsc.

If an earlier minimal prelude slice exists by the time this item starts, replace it rather than
forking a second ambient-loading path. The full library loader is the canonical mechanism.

## Touch points

`.d.ts` parsing/consumption; the shared prelude in the type universe (parallelism Stage 1 —
architecture §8.2).

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
