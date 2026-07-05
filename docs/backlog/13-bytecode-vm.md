---
id: 13
title: Post-evaluator profiling gate — bytecode VM only if profiling demands it
blocked-by: [./12-utility-types.md]
---

# 13 — Post-evaluator profiling gate (bytecode VM demoted to a deferred refactor)

**Summary.** Originally "build the bytecode VM (M29)". Re-scoped per
**[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)**: the
order-of-magnitude type-level wins are **algorithmic** and belong in the *tree-walked* evaluator;
the IR → bytecode → stack VM is a **potential later refactor**, not a planned pillar. The evidence
(tsgo ships ~10× with no VM; the cited VM "proofs" are stalled toys measuring warm caches; Zod-class
pain is instantiation + relation, not evaluation) is in the ADR.

## Decided work moved into `09`–`12`

The algorithmic wins are **required acceptance inside the evaluator milestones themselves** so nobody
ships a naive O(n²) evaluator and hopes this item rescues it later:

1. **Memoization** `(type-fn, args) → result`, keyed on hash-consed argument `TypeId`s. Biggest
   lever for tree (non-tail) recursion (`Flatten<L> & Flatten<R>`). Add depth limit + cycle
   detection.
2. **Accumulator reuse** (tail-rest): grow `[...Acc, X]` in place — O(n²) → O(n).
3. **Explicit heap work-stack (trampoline)** for evaluation — don't recurse on the host (Rust)
   stack, or deep type-level code overflows it before the logical limit.
4. **Arithmetic intrinsics** — intercept `Add`/`Sub`/`Lte`-style tuple/template math and compute it
   natively instead of recursing over tuple lengths.

Acceptance for those lives in `09`–`12`. This item must not be used to postpone the basic evaluator
guardrails.

## Acceptance for this item

After `09`–`12` land, profile at least one deliberately type-level-heavy corpus and one ordinary
application-style corpus. If the remaining hot path is relation/instantiation/allocation, close this
item with the profiling notes and do **not** build a VM. Carving type-level evaluation into IR →
bytecode → stack VM (architecture §7.1) is undertaken **only if** profiling shows the *interpreter
dispatch loop itself* — not the algorithm, not relation/instantiation — is the bottleneck. Trigger:
a measured, reproducible type-level-dispatch hot spot that the four wins above do not flatten. If it
ever lands, architecture §7.1–7.4 is its design reference and the risk in §11.2 re-arms.

## Touch points

Profiler setup and benchmark fixtures; optionally the conditional/mapped/template/utility evaluator
if profiling proves dispatch overhead. No new IR/bytecode/VM unless the trigger above fires.

<!-- Origin: dev roadmap M29 (was HANDOFF §3, the type-level VM phase). Re-scoped by ADR-0001. -->
