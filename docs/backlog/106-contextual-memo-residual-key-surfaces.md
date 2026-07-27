---
id: 106
title: Two residual surfaces in the contextual-walk memo are argued, not constructed
blocked-by: []
---

# 106 — Two residual surfaces in the contextual-walk memo are argued, not constructed

**Summary.** Backlog `95`'s memo is sound by *construction* almost everywhere: declaration types are
tracked through a single accessor rather than named in a key, and states that cannot be summarized
are refused outright. Two places are not — one rests on a prose argument about a mechanism, the
other on a 64-bit hash. Neither is known to be wrong, and the implementer flagged both rather than
letting them pass silently. Effort S. File-and-forget is the failure mode here; this item exists so
the next reader finds them named.

## The two surfaces

### 1. One-shot consumption is argued from the mechanism, not enforced by the key

`namespace_values` are documented as "consumed exactly once at their source sites", and
`function_groups` publication is likewise a mutation that a *skipped* walk does not perform. Neither
appears in `WalkEnvironment`.

The argument for correctness is: a memo hit means a walk of that same node already ran, so the
one-shot consumption already happened, and the un-memoized build's second walk would have found it
consumed too. That is very likely right — but it reasons about what the mechanism does, where the
rest of the design reasons from what the code cannot do. The difference matters the next time
somebody adds a second consumer of either structure, because nothing will fail.

**What would close it:** a `#[cfg(test)]` counter asserting that consumption count per site is
identical with and without the memo, over the differential corpus. If that is impractical, a
refusal condition — the shape the design already uses for everything it cannot summarize — is the
cheaper answer than a longer argument.

### 2. `type_params` enters the key as a 64-bit hash

`WalkEnvironment` hashes `type_param_scopes` + `static_class_type_param_barriers` +
`enclosing_classes` rather than comparing them. A collision serves an entry from a different
type-parameter environment, which is a wrong-types outcome and therefore potentially a dropped
diagnostic — the sharpest bug class in this project.

At 64 bits the probability is negligible in practice. But it is the one **probabilistic** element in
a design whose whole argument is structural, and this project's stated stance is that when in doubt
it over-reports. A probability, however small, is not the safe direction; it is a small chance of the
unsafe one.

**What would close it:** measure whether an exact comparison is actually expensive. Empty
type-param frames are already skipped from the hash (worth 2× on its own, because `lookup_type_param`
walks the whole stack so an empty frame shadows nothing), which suggests the live frames are few and
an exact compare may cost nothing worth having. If it does cost, keep the hash and say so here with
the measurement, so the trade is recorded rather than inherited.

## Also inherited from the reverted attempt, and already guarded

The records half — `ProvisionalArgumentWalk::{Settled, Held, Memoized}` plus the mandatory `settle`
closure that re-walks any still-`Memoized` argument nothing superseded — carries a known exposure:
the recovery walk runs at frame close rather than at the argument's original position. It is pinned
by `retained_raw_walks.ts` and `nested_retained_raw_walks.ts` and showed no divergence across 14,000
differential programs. Listed here for completeness, not as open work.

## Touch points

`src/check/checker/context.rs` (`DeclTypes`, `WalkEnvironment`), `src/check/checker/calls.rs`.

<!-- Origin: self-reported by the backlog 95 work unit, 2026-07-27, as the two weakest links in its
     own soundness argument. Filed so they are not lost in a commit message. -->
