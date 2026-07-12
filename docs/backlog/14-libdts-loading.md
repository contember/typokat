---
id: 14
title: Full lib.d.ts loading (the standard library)
blocked-by: [./41-generic-methods.md, ./43-namespaces-declaration-merging.md, ./70-this-parameter-typing.md]
---

# 14 — full `lib.d.ts` loading

**Summary.** The "mandatory core" (architecture §4) — unlocks checking real-world code. Big. Also
where parallelism **Stage 1** lands. This item is the **full** standard-library load; the minimal
ambient/prelude slice (`38`) is allowed before this item when it buys useful real-world feedback.

## Problem

Without `lib.d.ts`, `console`, array methods, `Promise`, etc. are absent, so most real code can't be
checked. The lib's own source text uses nearly the whole type model, including the `RegExp` value
surface that owns regexp literals, which is why this item is
blocked by the remaining model-completeness track: generic methods (`41`),
namespaces + declaration merging (`43`), and `this`-parameter typing / `ThisType<T>` (`70`).
Loading the lib with any of those
still silently-permissive would poison every downstream check. A deliberately small prelude slice
(`38`) may land earlier because it curates its declarations around the gaps.

## Approach / acceptance

Parse and load the standard `lib.d.ts` declarations into the type universe as a shared read-only
prelude. This is also where parallelism **Stage 1** lands — the shared read-only prelude across
per-file workers (architecture §8.2). Acceptance: fixtures using `console`, array methods, and
`Promise` check correctly against tsc. As the full-stack ambient witness, the pinned
`contember/deptective` revision recorded by backlog `72` must no longer produce missing-global or
standard-library-member noise; backlog `15` owns resolving the same project's modules.

If the minimal prelude slice (`38`) exists by the time this item starts, replace it rather than
forking a second ambient-loading path. The full library loader is the canonical mechanism.

## Touch points

`.d.ts` parsing/consumption; the shared prelude in the type universe (parallelism Stage 1 —
architecture §8.2).

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
