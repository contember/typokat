---
id: 85
title: Replay owner closure is quadratic on an accumulating chain
blocked-by: []
---

# 85 — Replay owner closure is quadratic on an accumulating chain

**Summary.** The terminal class-dependency closure materializes one owner `Vec` per expression, so
a chain whose owner set grows link by link costs Σ|owners| = L²/2 for L pairs of real output.
**Live at today's scale**: the 82-file profile materializes 2.24 M owner entries to answer 3
queries — 22% of cold library compilation. Effort M.

## Problem

`terminal_expression_owners` (`crates/typokat-check/src/check/checker/library_compiler.rs`) flattens the owner
expressions in one ascending pass, memoizing every expression (`bc59c9f`). That is exactly
input+output on a shared pass-through spine, which is what the previous root-only memoization
regressed on (`dfa62b4`). It is *not* linear on an **accumulating chain**: `T0 → T1 → … → T(L-1)`
where each `Ti` is also the direct root of its own owner group — concretely a chain of type
aliases each in its own type group — with a single class at the tail. Every link mints a two-input
`Union` whose owner set is one larger than its predecessor's, so the retained sets are quadratic
while the emitted closure is L pairs. Measured with a throwaway probe during the `bc59c9f` review:

| depth | output pairs | `owner_expression_visits` |
|---|---|---|
| 256 | 256 | 33,406 |
| 1024 | 1,024 | 526,846 |

15.77× for a 4× input. Root-only memoization was O(L) on this shape, so the two strategies have
mirror-image failure modes; reversing the walk (owner leaves → class roots) mirrors it again.
This is inherent — per-class owner closure is transitive-closure/BMM-hard — so no choice of walk
direction fixes it. Only the *representation* can.

**The real profile hits it.** A `pprof` run over 40 cold release processes at `c844680` measured
`terminal_expression_owners` at **410,589 µs — 22.19% of cold library compilation**, with:

```
expressions=7073  unions=3681  merge_elems_copied=6,496,054
owner_entries_final=2,236,463  max_owner_set=2445  direct_owners=3392
sem_nodes=16913  components=13542
class_owner_inputs=3  require_dependency_calls=865
```

2.24 M owner entries and 6.50 M element copies materialized to answer **3** class queries yielding
865 dependencies. Ablating the pass saved **439 ms** of a 1.86 s run. An earlier estimate of "44
owner expressions at real profile scale" was wrong: every test that compiles the 82-file profile is
`#[ignore]`d, so the suite it was measured over never ran the real profile.

The dominant cost is therefore not the accumulating-chain shape but **materializing the whole
library's owner sets for a handful of queries** — the closure is not demand-driven. Fixing that
subsumes the chain bound for the profile, though the chain bound stays real for the scale ladder.

There is deliberately **no RED spec** for this shape: adding one would have contradicted the
acceptance of `bc59c9f`, which is a real improvement on the shape that was actually regressed.

## Approach / acceptance

Three options, in the order the evidence justifies:

1. **Demand-drive the closure** (the profile's actual win). Evaluate only the expressions reachable
   from the expressions actually queried — 3 of 7,073 on the real profile — instead of flattening
   every expression up front. Worth ~410 ms of a 1.86 s cold compile on its own.
2. **Consumer refcount.** Track how many expressions consume each input and *move* an input's set
   into the merge when the current expression is its last consumer. The chain's copies become
   moves — still O(L²) memmove, but no allocation and no re-sort; roughly a 10–50× constant.
3. **Shared/persistent owner sets**: a persistent set or an interned owner-set id with structural
   sharing, so a chain's prefixes are shared rather than copied. Bounds the chain shape itself.

Acceptance: a guard spec on the accumulating chain asserting `owner_expression_visits` tracks
input growth, alongside the existing shared-spine spec — both shapes linear at once — with the
completeness pin (`..._shared_union_spine_stays_complete`) unchanged. Do not trade one shape's
bound for the other's again.

## Touch points

`crates/typokat-check/src/check/checker/library_compiler.rs` (`terminal_expression_owners`,
`require_terminal_class_dependency_closure`, the `TerminalClassDependencyValidationWork` counters
and their specs).

<!-- Origin: independent adversarial review of dfa62b4 and the bc59c9f follow-up, 2026-07-24. -->
