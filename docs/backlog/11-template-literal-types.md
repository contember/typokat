---
id: 11
title: Template literal types (`${A}-${B}`) — M27
blocked-by: []
---

# 11 — Template literal types (`` `${A}-${B}` ``)

**Summary.** Type-level evaluation phase (tree-walked; bytecode VM deferred — ADR-0001). Template
literal types over string literal unions.

## Problem

Template literal types (`` `${A}-${B}` ``) aren't evaluated.

## Approach / acceptance

Evaluate template literal types, distributing over unions of the interpolated parts. Acceptance:
fixtures covering a fixed template, interpolation of a literal union (cartesian expansion), and
assignability of the resulting literal-union type, matching tsc.

Performance/scalability acceptance is part of this milestone: expansion must be explicit-stack based,
memoized for repeated interpolations, and guarded by a clear size/depth limit so cartesian products
fail deliberately instead of blowing up memory or the Rust stack. Include a stress fixture for a
multi-slot union interpolation.

## Touch points

Template-literal-type evaluation in the checker; literal-type construction in the type store;
evaluator memoization/work-stack/size-limit machinery.

<!-- Origin: dev roadmap M27 (was HANDOFF §3, the type-level VM phase). -->
