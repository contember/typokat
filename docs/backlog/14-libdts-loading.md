---
id: 14
title: Full lib.d.ts loading (the standard library)
blocked-by: [./43-namespaces-declaration-merging.md]
---

# 14 — full `lib.d.ts` loading

**Summary.** The "mandatory core" (architecture §4) — unlocks checking real-world code. Big. Also
where parallelism **Stage 1** lands. This item cannot start while backlog `43` retains its
standalone-namespace architecture stop. It is the **full** standard-library load; the minimal
ambient/prelude slice (`38`) is allowed before this item when it buys useful real-world feedback.

## Problem

Without `lib.d.ts`, `console`, array methods, `Promise`, etc. are absent, so most real code can't be
checked. The lib's own source text uses nearly the whole type model, including the `RegExp` value
surface that owns regexp literals, which is why this item remains blocked by the final namespace
value-side prerequisite in `43`.
Generic method, call, and construct signatures shipped with B41; explicit receiver parameters and
contextual `ThisType<T>` shipped with B70; member projection and loading the declarations that expose those
signatures remain this item's responsibility.
Loading the lib with any of those
still silently-permissive would poison every downstream check. A deliberately small prelude slice
(`38`) may land earlier because it curates its declarations around the gaps.

## Approach / acceptance

Parse and load the standard `lib.d.ts` declarations into the type universe as a shared read-only
prelude. This is also where parallelism **Stage 1** lands — the shared read-only prelude across
per-file workers (architecture §8.2). Acceptance: fixtures using `console`, array methods, and
`Promise` check correctly against tsc. The Bundler-compatible full-stack ambient witness selected
with backlogs `72`/`15` must no longer produce missing-global or standard-library-member noise;
`contember/deptective` remains a candidate only if it qualifies under that resolver profile.
Backlog `15` owns resolving the same witness's modules.

If the minimal prelude slice (`38`) exists by the time this item starts, replace it rather than
forking a second ambient-loading path. The full library loader is the canonical mechanism.

## Pinned start gate

The authoritative gate is
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml), committed through
`b424e74`, `5951968`, and `3f641ea`. It currently concludes **NO-GO**. Type-side namespaces,
reopenings, all 28 interface+var pairs, repeated interfaces, and local `Array<T>` heritage pass.
The sole blocker to starting this item is backlog `43`'s standalone `Intl` namespace-value
incomplete.

After `43` clears, this item owns two canonical `TK2430` diagnostics on `CallableFunction` and
`NewableFunction` versus `Function`. Two surplus diagnostics at those sites are parity-only backlog
`63`. Backlogs `50` (8 predicate incompletes) and `75` (179 annotation-shape incompletes)
independently block checker 1.0, not the start of loader implementation; the loader must preserve
those explicit outcomes rather than approximate them.

## Touch points

`src/driver.rs`, `src/check/checker/mod.rs`, `src/check/checker/decls/`, and the shared prelude/type
universe paths selected by its architecture design (parallelism Stage 1 — architecture §8.2). The
current explicit-input readiness fixture is not the loader.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
