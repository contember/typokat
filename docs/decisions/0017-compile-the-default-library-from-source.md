---
id: 0017
title: Compile the default library from source; retire the shipped snapshot
status: accepted
date: 2026-07-26
---

# 0017 — Compile the default library from source; retire the shipped snapshot

## Context

[ADR-0011](0011-freeze-pinned-default-library-base.md) accepted one exact TypeScript 6.0.3 82-file
profile, one source-backed `LibraryCompiler`, an immutable AST-free base, identity-preserving private
deltas, and a same-universe private rebuild for collisions. It assumed first initialization would
perform real source semantic compilation and explicitly deferred serialization until cold-start
evidence existed.

[ADR-0012](0012-ship-the-canonical-default-library-snapshot.md) then narrowly superseded that one
initialization clause. Its case was arithmetic: on 2026-07-21 the source pipeline cost roughly
10.8 seconds against a native TypeScript comparator at roughly 0.3 seconds, and the active sprint's
fork analysis rejected "keep optimizing runtime compilation" on the ground that "roughly 9.83 seconds
sits in statement-check/evidence versus a likely ≤0.15-second target". A 36× gap is not a constant
factor, so precomputation was the honest answer. ADR-0013, ADR-0014 and ADR-0015 then built the
private-collision route on top of a decoded snapshot.

Two things have since changed the facts.

**The optimization work landed.** Cold source compilation of the full profile went from 10.8 s to
1.85 s over the 07-22 → 07-25 window.

**The 9.83-second attribution was wrong.** Two independent measurement passes at `c844680` (span
instrumentation, and `pprof` at 499 Hz over 40 cold release processes — 23,655 samples, 0 % stack
truncation) established that the `statement_check` span was **96 % not statement checking**. It
swallowed `canonical_library_evidence` (411 ms) and `finish_semantic_effects` (96 ms); real
`check_statements` is **19.8 ms**. A further 1,092 ms sat in one entirely uninstrumented function,
`build_collision_replay_index`.

Splitting the run along the only line a comparator can see gives:

| | cold, pinned 82-file profile |
|---|---|
| typokat parse + bind + reserve/fill + publication + statement check | **277 ms** |
| pinned native TypeScript 7.0.2, same bytes, same host, median of 15 | **289 ms** |
| typokat evidence + replay-index generation (no comparator analogue) | 1,152 ms |

So the checking pipeline is already at parity — 0.96× — and every millisecond of the remaining 5.1×
gap is work that exists **only to fill the snapshot's sections**: `canonical_library_evidence`,
`build_collision_replay_index`, `canonical_record_bytes` (computed twice over the same records), and
a `terminal_expression_owners` closure that materializes 2.24 M owner entries to answer three
queries. ADR-0012's premise — that runtime compilation was orders of magnitude above target — no
longer describes the system. The artifact was paying for its own construction.

The remaining argument for the snapshot is that decoding is faster still: admission measured 112 ms
median / 119 ms p95 against source compilation's 277 ms. That is real, and giving it up is a real
cost. It is rejected on product grounds: a precomputed artifact for one pinned profile does not make
*arbitrary user code* faster, and a checker that needs a shipped snapshot to be competitive has not
demonstrated a fast checker. Optimizing the path every project pays is the goal; optimizing away the
one path that can be precomputed is not.

## Decision

**Typokat compiles its default library from the vendored 82 sources in every fresh process. There
is no shipped semantic artifact.**

The 21 MB `canonical.snapshot`, its versioned wire format, its generator, its decoder, its admission
and identity pins, and the external coordinators that verified them are deleted. The vendored source
profile — the 82 `.d.ts` files, `profile.toml`, and the TypeScript licence and notice files — stays;
it is the input, and it always was the authority.

`FrozenLibraryBase` is unchanged as a *type*. Only its provenance changes: it is constructed from
`LibraryCompiler`'s frozen runtime product instead of from a decoded archive. Every ADR-0011
guarantee about what that base is — one immutable AST-free universe, pointer-shared across
non-colliding workers, never mutated by a user delta — remains binding and untouched.

### This restores ADR-0011 rather than replacing it

ADR-0012 is **superseded wholesale**. Because ADR-0012's only effect was to supersede ADR-0011's
runtime-source initialization clause, retiring it restores that clause verbatim: first initialization
performs real semantic compilation. ADR-0011 returns to full force.

ADR-0015 is **superseded wholesale**. Its `AuthenticatedCollisionSeed` is defined as a capability
"constructed only by the admitting `LibraryBaseProvider`" from decoded bytes. With no bytes to admit,
the capability has no constructor and no meaning.

ADR-0013 is **narrowly superseded in its seeding clause only.** A private collision run seeds a
fresh, exclusively owned universe by **compiling the profile from source**, not by independently
decoding a snapshot. Everything else in ADR-0013 stays binding: the fresh-universe-never-overlay
rule, the affected-owner reverse-dependency closure, the append-only replacement rows, the rule that
no affected terminal remains observable, and `LibraryCompiler` plus the checker `Pass` as the sole
lowering and publication authority.

ADR-0014 is **narrowly superseded in its authentication clauses.** Its boundary 1 — "fresh canonical
snapshot decode authenticates and reconstructs the complete semantic prefixes" — has no referent and
is retired. Its boundary 2 survives as a *construction* boundary but not as a *comparison* one: the
source parse/bind still reaches a library-only continuable checkpoint that has performed all
dormant-storage and compilation-global finalization and holds zero user binder rows, event
reservations, and semantic allocations, but it no longer byte/digest-compares that checkpoint against
admitted snapshot sections.

That comparison is deleted rather than reimplemented, and the reason matters. Authentication is only
meaningful between two independent producers. With the artifact gone there is exactly one producer,
so a retained check would compare a value against itself and report success unconditionally. A
vacuous check is worse than no check: it reads like a soundness boundary in the source and defends
nothing.

**The invariant that replaces it is structural.** `UnauthenticatedLibraryBinderCheckpoint` may be
constructed only by `LibraryCompiler::compile_binder_checkpoint`. Provenance is then guaranteed by
the type system at compile time rather than by a digest at run time — a stronger guarantee, not a
weaker one, because it cannot be bypassed by a caller who happens to hold matching bytes. Any change
that lets a checkpoint be constructed elsewhere breaks this ADR.

### Generation work leaves the startup path

`canonical_library_evidence`, `build_collision_replay_index`, `canonical_record_bytes` and the
terminal-owner closure were section producers. Work that exists only to serialize a base that is no
longer serialized does not run at startup. What the base/delta and collision model genuinely
requires — the library's own diagnostics and incomplete outcomes, which ADR-0011 requires be
preserved exactly, and whatever dependency information private replay actually consumes — is
retained, but it is computed for use rather than for encoding, and it is sized against the queries
it answers.

## Consequences

- **The performance claim changes and must be restated, not quietly carried over.** The active
  sprint's binding claim is "at least 2× faster than the pinned native TypeScript 7 executable",
  which the snapshot's 112 ms bought. Source compilation at today's 277 ms is 1.04×. The sprint
  contract must be rewritten to the claim this decision can actually support; it must not be left
  at 2× and quietly missed.
- **The headroom is real but it is not parallelism.** The cold library run is single-threaded
  (`cpu_us/wall_us = 0.994`, one core of sixteen) while the comparator is parallel, which sounds like
  free margin. It is not: on this workload only parse (14 ms) and per-file bind (19 ms) are
  embarrassingly parallel, so the parallel ceiling is ≤30 ms of the 277 ms. The margin has to come
  from reserve/fill and publication, where the time actually is — `check_statements` is 19.8 ms.
- **Startup cost becomes proportional to profile size**, so the profile stops being free. Adding
  library files now costs every process. That is the honest incentive.
- **18,574 lines are deleted against 736 added**, along with 21 MB of checked-in binary, two
  external verification coordinators, and one CI job. The release binary drops from roughly 27 MB to
  **6.8 MB**, and the library test suite from **242 s to 53 s**. Four permanently-failing tests —
  stale-artifact identity comparisons that no ordinary change could keep green — go with them.
- **A further ~6,300 lines of byte-level encode/decode are orphaned rather than removed** — nothing
  writes the format now, but part of it is being used as a traversal that backs live
  reference-integrity assertions. It is gated `#[cfg(test)]` pending
  [backlog `97`](../backlog/97-orphaned-wire-serialization.md).
- **Re-pinning the library's evidence to source truth exposed an unattributed delta**: 273 → 265
  diagnostics, with bytes rising 91,453 → 125,251 and incompletes unchanged. The pins were 102
  commits stale, so the delta predates this decision and is not caused by it, but it must be named
  rather than absorbed — [backlog `98`](../backlog/98-library-diagnostic-count-delta.md).
- **Regeneration ceremony disappears.** Every semantic change previously invalidated the artifact and
  required a clean-tree two-clone reproducible regeneration before the suite could be green. That
  ceremony was a standing tax on ordinary work and is now unnecessary.
- **A whole class of soundness risk disappears**: stale-artifact acceptance, source/snapshot semantic
  drift, decoder-synthesized state, and partial or corrupt base exposure were all failure modes that
  only existed because a second producer existed.
- **Cold start gets slower in absolute terms** — 112 ms to 277 ms — and every future startup
  optimization must now be a real checker optimization. That is the point of the decision, but it is
  a cost, not a free win.

## Alternatives considered

**Keep the snapshot and delete only the generation cost.** The 1,152 ms is generation, so a
snapshot-serving build could in principle generate the artifact only in the explicit developer
command and never at startup. This preserves the 112 ms and the 2× claim. Rejected on the product
ground above: it makes one pinned profile fast and teaches us nothing about arbitrary code, and it
leaves the second-producer risk class in place. This is a genuine trade and it was decided against
deliberately, not because the option failed on its own terms.

**Keep a vacuous authentication boundary for future-proofing.** Retain
`authenticate_library_binder_checkpoint` comparing source digests against source digests, so a second
producer could be reintroduced later. Rejected: a check that cannot fail is not a boundary, and
leaving one in the source misleads every later reader about where the soundness line is. If a second
producer ever returns, it brings its own authentication.

**Lazy or reachability-pruned library loading.** Load only the declarations a compilation reaches.
Rejected here for the same reason ADR-0011 rejected it: the accepted product is one complete frozen
82-file base, a small benchmark must not silently omit untouched declarations, and the library's own
diagnostics must be preserved exactly. Revisiting this needs a new product decision, not a
performance shortcut.
