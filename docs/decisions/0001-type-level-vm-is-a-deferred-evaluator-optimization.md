---
id: 0001
title: Type-level performance lives in the tree-walked evaluator; the bytecode VM is a deferred, profiling-gated refactor
status: accepted
date: 2026-06-25
---

# 0001 — The bytecode VM is a deferred evaluator optimization, not a planned pillar

## Context

The original architecture staged a **bytecode VM** for type-level evaluation (IR → bytecode →
stack VM) as a *planned pillar* — "Phase 3" (architecture §12), milestone M29
(backlog `13`) — promising an order-of-magnitude type-level speedup, citing `tyvm` and
`TypeRunner`. A re-examination (with web research) found the **win is mostly algorithmic, not
bytecode-dispatch**, and the empirical basis for "order of magnitude" is weak:

- **The official Go port (`tsgo`) ships ~10× with no type-level VM at all** — native code
  (3–4×) + shared-memory parallelism (3–4×) + ~2× memory. *Caveat:* `tsgo` was a
  compatibility-mandated port (match `checker.ts` semantics, behaviour, and error messages 1:1),
  so it was structurally *forbidden* to re-architect into a VM. This is therefore a **weak**
  signal, not proof. What survives the caveat: a VM is not *necessary* for SOTA performance, and
  the documented perf pain of `tsc`/`tsgo` is **instantiation + relation**, not interpreter
  dispatch.
- **The two cited "proofs" are exactly what architecture §10 already discounts.** `TypeRunner`'s
  headline numbers (406× cold, 10,052× *warm*) are the "toy + warm-cache + old-JS-`tsc`" triple
  §10 says cannot be extrapolated — the warm figures measure *cache hits* (skip the work), the
  cold figures fold the native-code win into the "VM" win. Both `tyvm` and `TypeRunner` support
  only a narrow subset (no real subtyping/narrowing/`lib.d.ts`) — they never paid the §11.1 wall —
  and both are stalled/experimental proof-of-concepts.
- **Real-world type-perf pain is instantiation + relation, not recursive evaluation.** The
  canonical example, Zod-class validators, is dominated by *instantiation explosion* and
  *structural relation checks*: `zod/v3` produced **>25 000** type instantiations per file,
  `zod/v4` **~175** — a fix achieved by **flattening the generics** (fewer instantiations, smaller
  types), not by a faster evaluator. That cost lives in the relation engine (§6) + the
  instantiation/interner machinery (§3); **a VM does not touch it.**
- **The VM's order-of-magnitude wheelhouse is a narrow slice:** type-level *programs* — parsers
  (route params, SQL/GraphQL → types, template-literal parsers), tuple arithmetic, deep recursive
  transforms with accumulators (`Join`/`Split`/`Paths`/`Deep*`), HKT libraries. By §10's own
  framing, whole-repo speedup is a weighted average "pulled down by the type-level share" — Amdahl
  caps the overall win, since this slice is a *minority* of real checking.
- **The tree-walker → VM gap is mostly algorithmic**, orthogonal to bytecode emission. Bytecode
  buys (a) a dispatch-loop *constant factor* (the 1.5–3× regime, not order-of-magnitude) and (b) a
  disk-cacheable artifact whose validity tracking architecture §7.1/§11.4 admit is "as hard as
  Salsa." The order-of-magnitude itself comes from memoization, accumulator reuse, and arithmetic
  intrinsics — none of which require a VM.

## Decision

**We will fold the algorithmic wins directly into the tree-walked type-level evaluator** as the
conditional/mapped/template/utility milestones (backlog `09`–`12`) are built. These are
correctness/scalability **requirements**, not optional polish:

1. **Memoization** of `(type-fn, args) → result`, keyed on hash-consed argument `TypeId`s (the
   type store already hash-conses). The biggest lever for tree (non-tail) recursion.
2. **Accumulator reuse / tail-rest growth** — grow `[...Acc, X]` in place: O(n²) → O(n).
3. **Explicit heap work-stack (trampoline)** for evaluation, so deep type-level recursion does not
   overflow the host (Rust) stack before the logical limit.
4. **Arithmetic intrinsics** — recognise `Add`/`Sub`/`Lte`-style tuple/template math and compute it
   natively instead of via recursive tuple-length hacks.

**We will NOT build the IR → bytecode → stack VM as a planned pillar.** It is demoted to a
**potential later refactor**, undertaken *only if* profiling on real type-level-heavy code shows the
**interpreter dispatch loop itself** — not the algorithm, not relation/instantiation — is the
bottleneck. Until then it carries no commitment.

## Consequences

- Removes the highest-risk, lowest-evidence item from the critical path: the unmapped
  VM ↔ interpreter boundary (§11.2) and the Salsa-hard bytecode cache (§7.1, §11.4) are no longer
  things the roadmap *must* solve.
- The **de Bruijn migration** (invariants §2) is correctly re-attributed to the **conditional-types
  milestone** (`09`: `infer` + alpha-equivalent hash-consing force it), not "the VM phase."
- Items `09`–`12` acquire explicit **performance acceptance** (memoize / accumulator / explicit
  stack / arith), so nobody ships a naive O(n²) evaluator that a later VM would have to rescue.
- If the bytecode refactor is ever justified by profiling, the technical sketch (architecture
  §7.1–7.4) survives as its design reference — the demotion costs no design knowledge.
- **Accepted downside:** on extreme DSL-heavy code we may sit at the tree-walker's constant-factor
  disadvantage versus a hypothetical VM until/unless the refactor happens. That slice is a minority
  of real checking, and the four algorithmic wins already capture most of the gap.

## Alternatives considered

- **Build the VM as originally planned (Phase 3 / M29).** Rejected: a speculative pillar on weak
  evidence (toys + warm caches + old `tsc`), carrying the project's least-mapped risk, whose win is
  mostly capturable algorithmically in the tree-walker.
- **Delete the VM idea entirely.** Rejected: the DSL-heavy slice is a genuine (if narrow)
  differentiator versus `tsgo`, which tree-walks `checker.ts` logic. Keeping the VM as a
  profiling-gated option preserves that upside at zero present cost.
