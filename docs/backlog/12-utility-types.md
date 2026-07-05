---
id: 12
title: Utility types (Partial, Record, Pick, …) — M28
---

# 12 — Utility types (`Partial`, `Record`, `Pick`, …)

**Summary.** Type-level evaluation phase (tree-walked; bytecode VM deferred — ADR-0001). Most fall
out of mapped/conditional types once those exist; a few are built-in.

## Problem

The common utility types aren't available: `Partial`, `Required`, `Readonly`, `Pick`, `Record`,
`Exclude`, `Extract`, `Omit`, `NonNullable`, `ReturnType`, `Parameters`.

## Approach / acceptance

Define the utility types — most as ordinary mapped/conditional type definitions (so they "fall out"
of items 09/10), a few as built-ins where they can't be expressed. Acceptance: fixtures using each
utility type resolve to the expected shape and check against tsc.

Performance/scalability acceptance is part of this milestone: utility definitions must exercise the
already-built conditional/mapped evaluator machinery from `09`–`10`; do not introduce a second
ad-hoc evaluator path. Include at least one recursive/deep utility-style fixture to confirm the
memoization, depth-limit, and work-stack guardrails still hold under ordinary library-style use.

## Touch points

The prelude/built-in type definitions; the mapped/conditional evaluation from items 09 and 10; any
small built-in utility hooks that cannot be expressed in the type language.

<!-- Origin: dev roadmap M28 (was HANDOFF §3, the type-level VM phase). -->
