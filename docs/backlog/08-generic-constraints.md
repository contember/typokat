---
id: 08
title: Generic constraints (<T extends U>) — M24
blocked-by: []
---

# 08 — Generic constraints (`<T extends U>`)

**Summary.** Constraint checking on type arguments + the constraint as a type parameter's *apparent
type*. Prerequisite for conditional types and many real patterns.

## Problem

Type-parameter constraints (`<T extends U>`) aren't checked or used: a bad type argument isn't
rejected (`TK2344`), and a constrained parameter doesn't expose its constraint's members (so
`T extends {x:number}` doesn't permit `t.x`).

## Approach / acceptance

Three pieces: (1) constraint checking on type arguments (`TK2344`); (2) the constraint as the type
parameter's **apparent type**, so member access on a `T` resolves through it; (3) constraint-based
inference. Acceptance: fixtures for a violated constraint (`TK2344`), member access via a constraint,
and inference using a constraint, all matching tsc.

## Touch points

Type-parameter repr; relation engine (constraint check); the inference engine; member access via the
apparent type.

<!-- Origin: dev roadmap M24 (was HANDOFF §3, mid-term). -->
