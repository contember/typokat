---
id: 11
title: Template literal types (`${A}-${B}`) — M27
blocked-by: []
---

# 11 — Template literal types (`` `${A}-${B}` ``)

**Summary.** Type-level VM phase (tree-walked first). Template literal types over string literal
unions.

## Problem

Template literal types (`` `${A}-${B}` ``) aren't evaluated.

## Approach / acceptance

Evaluate template literal types, distributing over unions of the interpolated parts. Acceptance:
fixtures covering a fixed template, interpolation of a literal union (cartesian expansion), and
assignability of the resulting literal-union type, matching tsc.

## Touch points

Template-literal-type evaluation in the checker; literal-type construction in the type store.

<!-- Origin: dev roadmap M27 (was HANDOFF §3, the type-level VM phase). -->
