---
id: 0020
title: Build source-native sparse collision epochs over the frozen library base
status: accepted
date: 2026-07-28
---

# 0020 — Build source-native sparse collision epochs over the frozen library base

## Context

The source-compiled `FrozenLibraryBase` makes the ordinary user-delta route faster than the pinned
native TypeScript comparator, but a legal project declaration can change a library-owned type,
value, namespace, or global-object surface. The ordinary immutable-prefix delta cannot perform that
merge. The guard tier records the attempted write; it does not publish the merged meaning.

ADR-0011 therefore selected a complete private library-plus-project source rebuild. ADR-0013 later
selected affected-owner replay in a fresh private universe, and ADR-0017 changed that universe's
seed from a decoded snapshot to an independent source compilation. Those choices are sound, but
their required second complete library compilation costs roughly the same as the process-wide base
compile. The `collision` row would therefore run at about half the comparator's speed. The active
sprint makes that result a stop condition: the row cannot be removed or relabelled, and a complete
source rebuild may be a correctness oracle but not the accepted benchmark route.

The old full replay-index generator is not an answer either. Its canonical-record rendering and
transitive terminal-owner closure added about 1.1 seconds to startup and existed largely to produce
the retired snapshot. The live source compiler already records direct dependency evidence, then
drops it on the deferred-index path. The decision is whether that live evidence can admit a sparse
private epoch without restoring a shipped artifact, a second semantic publisher, or stale shared
meaning.

## Decision

### Compile once from source, then create source-native private epochs

Every fresh process still compiles the complete vendored 82-file profile from source and publishes
one immutable `FrozenLibraryBase`. No semantic snapshot, persisted construction recipe, serialized
replay product, or AST is shipped.

That same source compilation also constructs a sealed, process-local `CollisionReplayPlan` from the
dependency trace the normal compiler observes. A colliding project creates a private collision
epoch over the source-compiled base. The epoch may structurally share only proven-unaffected
immutable prefix rows. It privately owns:

- typed sparse binder overrides and every user suffix;
- pending masks, append-only replacement rows, and publication terminals for affected owners;
- semantic graph identity and every reselected semantic identity;
- evaluator, projection, relation, substitution, flow, and provisional-cycle caches; and
- library replay events and user events in separate fresh domains.

Numeric IDs are meaningful only within the epoch that interprets them. No mutable row, terminal,
cache, event store, identity object, or user suffix crosses between projects.

This narrowly supersedes ADR-0011's complete-private-rebuild and no-prefix-sharing clauses,
ADR-0013's “fresh universe, never overlay the shared one” storage clause and complete-profile
parse/bind requirement, and ADR-0017's requirement that each collision seed independently compile
the profile.

For successful sparse epochs it also narrowly supersedes ADR-0014's surviving boundary 2 and 3:
the private route does not build a second complete library-only binder checkpoint before user
binding. The source-compiled base's sealed binder and plan are the prefix provenance. Replay sites
are keyed by exact profile identity, file ordinal, source span, declaration kind, and binder owner.
Only files containing affected sites are reparsed; their exact vendored bytes and keys are checked
before fresh nodes are matched to the retained provenance. A missing, duplicate, or mismatched site
discards the sparse epoch. ADR-0014's complete checkpoint remains binding for the full-source oracle
and fallback.

This does not change ADR-0017's process-start rule: the complete profile is still compiled from
source once in every fresh process. ADR-0012 and ADR-0015 remain superseded; this decision does not
restore either snapshot design.

### Bind through typed overrides and publish only through the checker

Preflight parses and classifies the real user inputs inside the large-stack worker before creating
a delta, reserving an event, allocating a semantic ID, or touching a semantic cache. A shared-route
capability is tied to that exact successful receipt. Uncertain relevant syntax and every possible
library/global-object collision select the private route.

The private binder exposes one prefix-or-override-or-suffix view. A legal merge into a base
aggregate keeps its binder identity and creates an epoch-local typed override for that exact row;
new declarations allocate after the frozen counters. Ordinary frozen-prefix mutation APIs remain
unchanged and cannot acquire this authority.

After combined user binding, the exact mutation ledger must be contained by the conservative
preflight seeds. The scheduler closes those seeds over direct reverse dependencies and SCCs. Before
any semantic query, every affected owner enters an epoch-local `Pending` state. A query cannot fall
through from `Pending` to an inherited base terminal.

`SemanticQueryCoordinator` consults the epoch's pending/replacement mapping before same-ID success,
relation/evaluator cache lookup, or provisional-cycle handling. Pending, poisoned, exhausted, or
otherwise provisional results promote no terminal, identity, relation, evaluator, or cache write.
Every raw semantic access that could bypass this boundary is rejected by the replay admission
guards.

Affected library source sites and all user owners then run through the existing
`LibraryCompiler`/checker `Pass` reservation, fill, publication, identity, statement, and event
phases. Changed structural rows are append-only; old affected rows may remain physically shared but
must be unreachable. Only `Pass` may lower or publish semantic state. The replay planner, binder
override layer, provider, and driver never patch a semantic terminal.

The epoch becomes observable atomically only after validating the mutation ledger, affected
closure, dependency containment, terminal and identity replacement, stale-row unreachability,
event ownership, cache isolation, and deterministic ordering. Otherwise it is discarded.

### Retain a compact plan, not the retired generator

The production `CollisionReplayPlan` contains only data consumed by routing and sparse replay:

- exact root-slot and global-object seeds;
- replay-owner source sites;
- direct reverse dependencies and root-slot consumers;
- statement-owner mappings needed to reserve affected library events;
- exact per-owner structured record cardinalities and fingerprints; and
- exact prefix boundaries plus construction health counters.

The record fingerprints are computed from structured drained records during the one source compile;
they are not retained records or rendered census lines. This preserves ADR-0018's replay contract
without putting the 875 library-owned records in the published base.

The plan does not compute canonical manifest bytes, rendered census lines, the all-owner transitive
terminal-expression closure, or an eager SCC materialization over every owner. Runtime scheduling
computes closure and ordering over the affected induced graph.

The complete test-only generator is a comparison oracle for the compact representation, but it is
not independent evidence because both consume the same dependency trace. Admission therefore also
cross-checks the plan against the binder/source census, retained binder provenance, typed-reference
coverage, and raw semantic-access guards, and differentially compares admitted sparse results with
the complete source compiler. The comparison and each independent coverage guard must have a known
broken negative control.

Plan construction must be measured as part of the ordinary fresh-process route. A plan whose cold
cost makes `fast-clean` or `fast-errors` miss the binding gate falsifies this decision.

### Keep the complete source path as containment, not success

The complete combined source compiler remains the differential correctness oracle and fail-closed
fallback under the existing process-wide permit. A dependency escape, unsupported replay owner,
missing source site, incomplete replacement, or validation failure discards the sparse epoch and
uses that path. It never returns to an unaugmented shared base.

A required `collision` or `fanout` sample that takes the fallback is a performance failure. The
sprint stops; the row is not removed, renamed, or declared out of scope.

Sparse epochs and complete-source fallbacks initially share the same one-process-wide permit. No
fanout may exceed one concurrently live private semantic universe until a later measured decision
sets another explicit cap.

### Resolve fragment ordering before enabling replay

Backlog `105`'s test/production type-group ordering split is a prerequisite evidence gate. Before
private replay is enabled:

1. compare library-ordinal and source-key order for every merged library type group;
2. select and pin one canonical order from that evidence;
3. remove the `cfg(test)` behavioural split; and
4. make ordinary compilation and replay consume the same ordering service.

Passing tests after deleting the second sort are not evidence because no current gate observes the
difference. The ordering decision and its witness ship separately from the collision epoch.

## Consequences

- One process performs one complete default-library source compilation. Collision work scales with
  user input plus the affected closure rather than the full library.
- The shared fast path and its immutable-prefix mechanics remain unchanged.
- Private collision implementation gains real complexity: typed binder overrides, pending masks,
  sparse terminal replacements, and compact replay-plan validation.
- Shared immutable storage is now allowed across private epochs; shared mutable state and inherited
  affected meaning remain forbidden.
- Every new semantic demand boundary must carry a replay owner or fail plan construction.
- The compact plan adds startup time and resident memory even in processes that see no collision;
  both are binding measurements, not assumed free.
- The full-source combined route remains maintained as the correctness oracle and recovery path,
  while being explicitly ineligible for the required collision/fanout benchmark result.
- The route remains reversible: disabling the sealed epoch API restores the correctness-first
  fallback without changing stored formats or the ordinary shared-delta path.

## Falsifiability

Reject the sparse epoch and stop WU5/WU7 if any of the following holds:

1. Any admitted merge or opposite input order differs from `tsc 6.0.3 --strict` or the complete
   source-combined oracle after normalization.
2. A binder mutation escapes preflight, or an actually changed prefix row is absent from the
   mutation ledger.
3. Any affected terminal, global-object member, semantic identity, declaration value, event, or
   library-dependent user read reaches stale inherited state.
4. Correctness requires semantic publication outside the ordinary checker `Pass`.
5. A successful sparse collision compiles or binds all 82 library files again, scans or clones the
   complete base, or invokes the retired canonical manifest/record work.
6. The compact production plan misses an edge or site found by the full generator, binder/source
   census, binder provenance, typed-reference coverage, or raw-access guards, or any known-broken
   negative control does not fire.
7. Any mutable cache, event store, identity object, or suffix is shared across epochs.
8. Fragment ordering still differs between test and production when replay is enabled.
9. `fast-clean` or `fast-errors` loses the binding gate because of plan construction, or
   `collision`/`fanout` takes full-source fallback, misses the strict `>1.00` confidence/p95 gate,
   exceeds 512 MiB, or loses deterministic bounded fanout.

## Alternatives considered

### Keep the complete private source rebuild

This is the simplest correctness oracle and fallback. It is the best choice if collision
performance is not a product requirement. It loses here because a second complete library compile
makes the required collision row fail, and the active sprint forbids redefining that failure away.

### Clone whole tables on first private write

Table-granular copy-on-write avoids per-row override routing and is the smallest implementation.
It is the best choice if affected merges touch few small table families and work proportional to
the unaffected base is acceptable. It loses because the committed scale contract requires zero
clone, scan, remap, or materialization work over unaffected base storage; the large semantic tables
would violate that contract before the benchmark answered anything.

### Patch affected terminals directly

Direct overlay patching is smaller than replay through `Pass`. It loses because it creates a
second publication authority, can bypass identity/event invariants, and makes dependency omissions
silently expose stale base meaning.

### Restore the full replay-index generator or a shipped artifact

The retired generator supplies strong authenticated metadata, and a snapshot gives the fastest
fixed-profile startup. Both are best if the product is optimizing only the pinned library. They
lose because their expensive/persisted work does nothing for arbitrary user code, and ADR-0017's
source-compiled product boundary remains binding.
