---
id: 07
title: Unstructured-flow narrowing (the flow-node CFG) — M23
blocked-by: []
---

# 07 — Unstructured-flow narrowing (the flow-node CFG)

**Summary.** The biggest remaining narrowing gap and the most likely to surprise on idiomatic code.
All current narrowing is **structured** (`if`/`else`/`switch`); unstructured flow doesn't narrow.

## Problem

`if (x === null) return; …` does **not** narrow the code after the early `return`; nor do `throw`,
loops, `&&`/`||`/ternary, or assignment-in-flow. These are syntactically identical to the supported
cases, so they look in-scope and turn up as false negatives against the official suite
(`controlFlow/`, `typeGuards/typeof*`).

## Approach / acceptance

Build the flow-node CFG in `src/check/flow.rs` (the `FlowNode` stub anticipates it) and resolve a
reference's type by walking the CFG backward applying guards. **Reuse the existing narrowing
operations** — they were written flow-model-agnostic for exactly this. Architecture §5 — this is the
"native interpreter" the design wants. Acceptance: fixtures for early-`return`/`throw`,
`&&`/`||`/ternary, and assignment-in-flow narrow correctly and match `tsc --strict`.

## Touch points

`src/check/flow.rs` (the flow-node CFG), the checker's narrowing env.

<!-- Origin: dev roadmap M23 (was HANDOFF §3, mid-term). -->
