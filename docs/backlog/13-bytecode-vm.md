---
id: 13
title: The bytecode VM for type-level evaluation — M29
blocked-by: [./09-conditional-types.md, ./10-mapped-types.md]
---

# 13 — The bytecode VM (type-level evaluation)

**Summary.** The order-of-magnitude marquee (architecture §7). Per §12 it comes **after** the
relation engine + narrowing + tree-walked conditional/mapped types — never before.

## Problem

Once conditional/mapped types exist tree-walked (items 09, 10), type-level evaluation is the hot path
and wants a real VM rather than a tree-walker.

## Approach / acceptance

Carve type-level evaluation into the IR → bytecode → stack VM with tail-call / accumulator-reuse /
memoization and specialized arithmetic instructions (architecture §7). Acceptance: the conditional/
mapped/utility corpus from items 09–12 passes unchanged on the VM, with the performance win the
architecture targets and no behavioral divergence from the tree-walked baseline.

## Touch points

A new type-level IR + bytecode + stack VM; the conditional/mapped evaluation entry points (re-routed
to the VM).

<!-- Origin: dev roadmap M29 (was HANDOFF §3, the type-level VM phase). -->
