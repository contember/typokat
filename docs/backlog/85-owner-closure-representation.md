---
id: 85
title: Replay owner closure is quadratic on an accumulating chain
blocked-by: []
---

# 85 — Replay owner closure is quadratic on an accumulating chain

**Summary.** The terminal class-dependency closure materializes one owner `Vec` per expression, so
a chain whose owner set grows link by link costs Σ|owners| = L²/2 for L pairs of real output.
Not live at today's scale (the 82-file profile builds 44 owner expressions) — this is scale-ladder
work, effort M.

## Problem

`terminal_expression_owners` (`src/check/checker/library_compiler.rs`) flattens the owner
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

There is deliberately **no RED spec** for this shape: adding one would have contradicted the
acceptance of `bc59c9f`, which is a real improvement on the shape that was actually regressed.

## Approach / acceptance

Two options, cheapest first:

1. **Consumer refcount.** Track how many expressions consume each input and *move* an input's set
   into the merge when the current expression is its last consumer. The chain's copies become
   moves — still O(L²) memmove, but no allocation and no re-sort; roughly a 10–50× constant.
2. **Shared/persistent owner sets** (the real fix): a persistent set or an interned owner-set id
   with structural sharing, so a chain's prefixes are shared rather than copied.

Acceptance: a guard spec on the accumulating chain asserting `owner_expression_visits` tracks
input growth, alongside the existing shared-spine spec — both shapes linear at once — with the
completeness pin (`..._shared_union_spine_stays_complete`) unchanged. Do not trade one shape's
bound for the other's again.

## Touch points

`src/check/checker/library_compiler.rs` (`terminal_expression_owners`,
`require_terminal_class_dependency_closure`, the `TerminalClassDependencyValidationWork` counters
and their specs).

<!-- Origin: independent adversarial review of dfa62b4 and the bc59c9f follow-up, 2026-07-24. -->
