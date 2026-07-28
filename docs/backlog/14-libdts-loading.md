---
id: 14
title: Full lib.d.ts loading (the standard library)
blocked-by: []
---

# 14 — full `lib.d.ts` loading

**Summary.** The "mandatory core" (architecture §4) — unlocks checking real-world code. Big. Also
where parallelism **Stage 1** lands. The pinned explicit-input model gate permits this item to
start. It is the **full** standard-library load;
the minimal ambient/prelude slice (`38`) is allowed before this item when it buys useful real-world
feedback.

**Active delivery contract.** The in-memory and collision semantics are accepted in
[`ADR-0011`](../decisions/0011-freeze-pinned-default-library-base.md). Its startup clause was
narrowly superseded by [`ADR-0012`](../decisions/0012-ship-the-canonical-default-library-snapshot.md)
in favour of a shipped snapshot, and then **restored** by
[`ADR-0017`](../decisions/0017-compile-the-default-library-from-source.md), which retires that
snapshot: the library is compiled from its 82 vendored sources in every process. The measurement
that changed the decision is that typokat's cold parse+bind+check of the pinned library is 277 ms
against native TypeScript 7's 289 ms on the reference host, and the 1.85 s the source path used to
cost was 62 % artifact generation with no comparator analogue. The
[`2026-07-21 performance-cutover sprint`](../sprints/sprint-2026-07-21-full-lib-performance-cutover.md)
owns the production base/delta and collision paths, the atomic driver cutover, and a fail-closed
fresh-process target against pinned native TypeScript 7 on every approved semantic row. The earlier
[`2026-07-16 feasibility sprint`](../archive/sprint-2026-07-16-full-lib-loading.md) removed the
first substitution and diagnostic-rendering barriers but ended at WU0 NO-GO: its authoritative
5.00 s cold gate still exited 143 after 5.268 s. It did not authorize or run WU1–WU8. No loader or
cutover has shipped yet; `crates/typokat-check/src/prelude.ts` remains the production path until the active sprint's
atomic acceptance gates pass.

## Problem

Without `lib.d.ts`, `console`, array methods, `Promise`, etc. are absent, so most real code can't be
checked. The lib's own source text uses nearly the whole type model, including the `RegExp` value
surface that owns regexp literals. The final namespace value-side prerequisite shipped at
`23bad42`, and its WU7 adversarial review and official-suite ratchet are complete.
Generic method, call, and construct signatures shipped with B41; explicit receiver parameters and
contextual `ThisType<T>` shipped with B70; member projection and loading the declarations that expose those
signatures remain this item's responsibility.
Loading the lib with any of those
still silently-permissive would poison every downstream check. A deliberately small prelude slice
(`38`) may land earlier because it curates its declarations around the gaps.

The post-WU7 official-suite adjudication supplies an exact missing-library witness set. Newly
reachable annotations now report `TK2304` for `Error`, `Promise`, `Generator`, `AsyncGenerator`,
and `CloseEvent` in `dependentDestructuredVariables.ts`; `Number`/`String` in the two
`*IndexerConstrainsPropertyDeclarations.ts` files; `Object` in
`callSignaturesThatDifferOnlyByReturnType.ts` and `subtypingWithConstructSignatures2.ts`; and the
second `Date` occurrences in `C3<Date>` at harness lines 83/105/127/149 of
`subtypesOfTypeParameterWithConstraints.ts`. `Iterable` in `partiallyNamedTuples2.ts` and the
implicit `Array` heritage of `arityAndOrderCompatibility01.ts` are the same host-library boundary,
not checker regressions. The label/heritage work made these dependencies honest; this item owns
supplying them, not suppressing their diagnostics piecemeal.

## Approach / acceptance

Parse and load the standard `lib.d.ts` declarations into the type universe as a shared read-only
prelude. This is also where parallelism **Stage 1** lands — the shared read-only prelude across
per-file workers (architecture §8.2). Acceptance: fixtures using `console`, array methods, and
`Promise` check correctly against tsc. The Bundler-compatible full-stack ambient witness selected
with backlogs `72`/`15` must no longer produce missing-global or standard-library-member noise;
`contember/deptective` remains a candidate only if it qualifies under that resolver profile.
Backlog `15` owns resolving the same witness's modules.

The exact official witnesses above must leave the harness's `OOS:unresolved`/host-heritage buckets
without replacing `TK2304` by an error-type fallback: all named globals resolve from the loaded
library, `StrNum extends Array<string | number>` composes its real heritage surface, and the
pre-existing diagnostics in those files remain measurable. Run the full official-suite ratchet
after the loader corpus passes; do not rebaseline away any newly exposed dependency.

If the minimal prelude slice (`38`) exists by the time this item starts, replace it rather than
forking a second ambient-loading path. The full library loader is the canonical mechanism.

## Pinned start gate

The authoritative gate is
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml), committed through
`b424e74`, `5951968`, and `3f641ea`, with the standalone namespace-value checker at `23bad42`. It
concludes **GO for starting backlog 14**: type-side namespaces, reopenings, all 28 interface+var
pairs, repeated interfaces, local `Array<T>` heritage, and both `Intl` type/value witnesses pass.
The raw artifact retains exactly four `TK2430` diagnostics and 181 incompletes — 173 owned by `75`
and eight by `50` — with no namespace-owned outcome. The synthetic semantic suffix is exactly 66
`TK2322`, including `deep.Intl.value`, and no `TK2304` or added incomplete.

This machine GO authorizes loader work now. It is not evidence that the library is loaded, that
this backlog item is complete, or that checker 1.0 is
ready. This item still owns the two canonical `TK2430` diagnostics on `CallableFunction` and
`NewableFunction` versus `Function`; two surplus diagnostics at those sites are parity-only backlog
`63`. Backlogs `50` and `75` independently block checker 1.0, and the loader must preserve their
explicit outcomes rather than approximate them.

## Touch points

`crates/typokat-driver/src/driver.rs`, `crates/typokat-check/src/check/checker/mod.rs`,
`crates/typokat-check/src/check/checker/decls/`, and the shared prelude/type
universe paths selected by its architecture design (parallelism Stage 1 — architecture §8.2). The
accepted detailed design remains in
[`ADR-0011`](../decisions/0011-freeze-pinned-default-library-base.md); the archived
[`2026-07-16 feasibility attempt`](../archive/sprint-2026-07-16-full-lib-loading.md) records the
failed gate and optimization evidence but is not an active delivery contract. The
[`active performance-cutover sprint`](../sprints/sprint-2026-07-21-full-lib-performance-cutover.md)
re-verifies the implementation touch points and owns delivery. The current explicit-input
readiness fixture is not the loader.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->
