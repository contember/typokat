---
id: 12
title: Utility types (Partial, Record, Pick, …) — M28
blocked-by: [./09-conditional-types.md, ./10-mapped-types.md]
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

## Touch points

The prelude/built-in type definitions; the mapped/conditional evaluation from items 09 and 10.

<!-- Origin: dev roadmap M28 (was HANDOFF §3, the type-level VM phase). -->
