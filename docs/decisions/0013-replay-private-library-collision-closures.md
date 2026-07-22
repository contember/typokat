---
id: 0013
title: Replay affected library owners in private collision universes
status: accepted
date: 2026-07-23
---

# 0013 — Replay affected library owners in private collision universes

## Context

[ADR-0011](0011-freeze-pinned-default-library-base.md) routes every possible default-library
collision or global-object contribution to one private library-plus-project universe. It selected a
complete source semantic rebuild and explicitly rejected replay metadata until measurement justified
a new decision. [ADR-0012](0012-ship-the-canonical-default-library-snapshot.md) later accepted the
canonical semantic snapshot for ordinary startup but retained that full collision rebuild.

The source compiler is now correct but too slow for the active sprint's collision row. The measured
82-file source path takes about 2.75 seconds: parse and bind together take about 29 milliseconds,
while reserve/fill, publication, and statement checking dominate the remainder. Repeating all
semantic work is therefore the wrong boundary. It also is not a rare fallback: a conservative audit
of the pinned official corpus predicts that script global-object contributors route most projects
private even though direct collisions with a frozen library name are uncommon.

Using the shared base plus an ordinary user delta is unsound. A merge can change `Array<T>`, DOM,
namespace, value, class, intrinsic, or `globalThis` surfaces and every library owner that consumed
them. Conversely, reparsing and rebinding the exact 82 sources is cheap and preserves the ordinary
same-universe declaration-merging model. The missing artifact is an authenticated record of which
semantic publication owners must be rebuilt after a root or slot changes.

## Decision

### Seed a fresh private universe, never overlay the shared one

A private run obtains the existing process-wide permit and independently decodes the canonical
snapshot into a fresh, exclusively owned, initially unsealed runtime. It shares no table, terminal,
cache, or identity object with the process-wide `Arc<FrozenLibraryBase>`; equal numeric IDs are local
to separate universes.

In parallel where the bounded implementation permits it, the private compiler parses and binds the
exact embedded 82-file profile plus the complete user project through its normal combined-universe
binder path. The library portion must reproduce the authenticated snapshot's complete ID prefix
before any user suffix. A prefix mismatch discards the candidate before semantic construction.
Thread joins have deterministic error precedence and do not expose partial state.

The fresh semantic seed supplies only previously published, unaffected library terminals. Directly
modified owners, their complete reverse-dependency SCC closure, and all new user owners are pending.
The compiler rehydrates their AST-backed declarations and runs the existing lexical reservation,
reserve/fill, namespace/value preparation, class/interface completion, atomic publication,
statement checking, identity selection, and event-ledger validation phases. Stale semantic arena
rows may remain unreachable; no affected terminal remains observable. The construction mask is
consumed into one complete private environment before user checking and is never a query-visible
project overlay.

`LibraryCompiler` and the existing checker `Pass` remain the sole lowering and publication
authority. The driver chooses a route, the snapshot decoder reconstructs typed rows, and the replay
index selects work; none of them patches semantic terminals directly.

### Authenticate one compiler-generated replay index

The canonical artifact gains a versioned `CollisionReplayIndex`, generated and validated by the
same source compiler as the semantic snapshot. It contains at least:

- sorted root names with exact value/type/namespace slots and global-object contribution flags;
- replay-owner source sites and retained binder provenance;
- sorted dependency-to-consumer edges, canonical SCC membership, and replay order;
- statement/event owners and the semantic owners they may observe or mutate; and
- per-owner baseline library-record cardinality and digest.

Replay owners cover type groups, value storages, namespaces, class publication metadata,
`GlobalObject`, and library statement/event obligations. Dependencies are recorded at actual
type/value/namespace lookup and semantic-demand boundaries during source compilation, then checked
against retained typed references and binder provenance. Final `TypeId` references alone are not a
sufficient graph because a diagnostic-only or failed demand may leave no published structural edge.
A demand made without an active owner is a generation error.

Preflight seeds exact slots, not just spellings, and always seeds `GlobalObject` for script or
`declare global` value contributors and explicit `globalThis` forms. Replay walks reverse edges to
a fixed point over canonical SCCs. After combined binding, every actually modified existing owner
must be covered by the immutable seed set before semantic work begins.

The index changes snapshot meaning and therefore advances ADR-0012's schema epoch and canonical
artifact identity. Missing, corrupt, reordered, profile-mismatched, or self-consistent but unpinned
index bytes are a typed initialization failure; production never regenerates or falls back to
sources to admit a bad artifact.

### Preserve conservative routing and a correctness oracle

User parsing, external-module classification, root/destructuring/global-object census, and immutable
index lookup may run before route selection. Creating a user delta or binder row, reserving an event,
allocating a semantic ID, or touching evaluator/relation/projection caches may not.

An unknown relevant syntax form selects private execution. Within a valid private universe, an
unsupported replay owner, missing source site, unexpected dependency edge, stale affected identity,
or unexpected library-origin record discards the replay candidate and invokes the complete combined
source compiler under the same permit. It never returns to the unaugmented shared base. The complete
source path remains the differential oracle and fail-closed correctness fallback, not an eligible
result for the sprint's required collision or fanout benchmark rows.

Every approved private-route family, including unique script globals as well as direct library
collisions, must have a witness that takes selective replay and matches the full combined source
oracle after normalization. Opposite project orders retain the ADR-0011 event and identity rules.

This narrowly supersedes ADR-0011's requirement that every collision repeat the complete source
semantic build and its prohibition on authenticated reverse-dependency replay metadata. It retains
the exact profile, one compiler/publication authority, same-universe merging, conservative
preflight, private identity isolation, one-process containment, library-ledger rules, event
ownership, memory ceiling, and prohibition on partial or shared-base fallback. ADR-0012's canonical
snapshot remains the sole admitted semantic seed.

## Consequences

- Private checks still pay complete parsing and binding, but avoid semantic work unrelated to their
  changed roots and global-object surface.
- The common shared route remains WU4's immutable prefix plus local suffix with zero private decode
  or replay work.
- Snapshot generation and validation become stricter and the artifact grows by one authenticated
  dependency/owner index.
- Compiler demand boundaries must carry an active replay owner, adding instrumentation and tests to
  semantically central code.
- A rare unmodelled dynamic dependency may use the slow full-source fallback correctly, but it
  invalidates any performance row that exercises it and remains visible in route receipts.
- Decode and parse/bind may overlap on bounded scoped threads; semantic publication remains ordered
  until a later decision proves a safe parallel boundary.

## Falsifiability

Reject this mechanism if any of the following holds:

1. An approved B14/private benchmark case differs from a full combined source compilation.
2. A legal merge observes a dependency absent from the authenticated closure.
3. A published root reaches a stale affected type, class, type parameter, value, or namespace row.
4. Correctness requires patching a terminal outside the normal compiler publication phases.
5. A library-origin outcome cannot be matched to its baseline owner or a reserved user owner.
6. Opposite user input order changes normalized per-source semantics.
7. Any required collision or fanout sample takes the full-source fallback.
8. Collision or fanout misses the sprint's 2x confidence/p95 gate or 512 MiB ceiling.
9. Generation cannot validate the index against binder provenance, demand instrumentation, typed
   references, and the complete source compiler.
10. Route selection mutates user semantic state or a shared base before preflight completes.

## Alternatives considered

### Keep the complete source semantic rebuild

This remains the simplest correctness oracle and last-resort fallback. It loses because measured
semantic work is about two orders of magnitude larger than parse/bind and cannot meet the collision
performance gate.

### Add user fragments to the shared frozen base

Rejected because it leaks across projects and lets old library consumers retain stale semantics.
Copy-on-write sparse terminal patching without the normal compiler phases has the same soundness
failure and creates a second publication authority.

### Make the standard library demand-lazy

Rejected because the product contract is one complete authenticated ambient universe, not a
reachable-name subset. It also does not solve same-universe declaration merging or diagnostic/event
obligations.

### Persist full AST construction recipes

Rejected because OXC ASTs are thread-pinned and retaining a parallel recipe/evaluator model would be
a larger second authority. Exact sources are already embedded, and parsing them is not the bottleneck.
