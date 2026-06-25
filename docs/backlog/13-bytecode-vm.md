---
id: 13
title: Type-level evaluator performance — algorithmic wins now, bytecode VM only if profiling demands it
blocked-by: [./09-conditional-types.md, ./10-mapped-types.md]
---

# 13 — Type-level evaluator performance (bytecode VM demoted to a deferred refactor)

**Summary.** Originally "build the bytecode VM (M29)". Re-scoped per
**[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)**: the
order-of-magnitude type-level wins are **algorithmic** and belong in the *tree-walked* evaluator;
the IR → bytecode → stack VM is a **potential later refactor**, not a planned pillar. The evidence
(tsgo ships ~10× with no VM; the cited VM "proofs" are stalled toys measuring warm caches; Zod-class
pain is instantiation + relation, not evaluation) is in the ADR.

## Decided work (do this in the tree-walker, as part of `09`–`12`)

These are **performance acceptance** on the conditional/mapped/template/utility evaluator — required,
so nobody ships a naive O(n²) evaluator (architecture §7.2):

1. **Memoization** `(type-fn, args) → result`, keyed on hash-consed argument `TypeId`s. Biggest
   lever for tree (non-tail) recursion (`Flatten<L> & Flatten<R>`). Add depth limit + cycle
   detection.
2. **Accumulator reuse** (tail-rest): grow `[...Acc, X]` in place — O(n²) → O(n).
3. **Explicit heap work-stack (trampoline)** for evaluation — don't recurse on the host (Rust)
   stack, or deep type-level code overflows it before the logical limit.
4. **Arithmetic intrinsics** — intercept `Add`/`Sub`/`Lte`-style tuple/template math and compute it
   natively instead of recursing over tuple lengths.

**Acceptance:** a deliberately deep/recursive type-level fixture (e.g. a tuple-arithmetic or
template-literal-parser corpus) type-checks without stack overflow and without O(n²) blowup; the
conditional/mapped/utility corpus from `09`–`12` passes with these wins in place.

## Deferred (the bytecode VM — no commitment)

Carving type-level evaluation into IR → bytecode → stack VM (architecture §7.1) is undertaken **only
if** profiling on real type-level-heavy code shows the *interpreter dispatch loop itself* — not the
algorithm, not relation/instantiation — is the bottleneck. Trigger: a measured, reproducible
type-level-dispatch hot spot that the four wins above don't flatten. Until then this stays an
option, not a roadmap item. If it ever lands, architecture §7.1–7.4 is its design reference and the
risk in §11.2 re-arms.

## Touch points

The conditional/mapped/template/utility evaluator in the checker (memoization table, accumulator
handling, explicit work-stack, arithmetic intrinsics). No new IR/bytecode/VM unless the deferred
trigger fires.

<!-- Origin: dev roadmap M29 (was HANDOFF §3, the type-level VM phase). Re-scoped by ADR-0001. -->
