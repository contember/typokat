---
id: 108
title: Generic arrow signature descriptors
blocked-by: []
---

# 108 — Generic arrow signature descriptors

**Summary.** A contextually placed generic arrow loses its generic binder descriptor, which makes
two compatible object-method assignments conservatively fail relation.

## Problem

In official `assignmentCompatWithCallSignatures2.ts`, TypeScript 6.0.3 accepts the generic arrows at
stripped scoreboard lines 23 and 27:

```ts
t = { f: <T>(x: T) => 1 };
a = { f: <T>(x: T) => 1 };
```

typokat emits `TK2322` at both sites. This is not a generic call-signature representation gap: B41
ships persistent generic binders. The remaining gap is the arrow expression's descriptor under the
specific object-method target.

## Approach / acceptance

Preserve the generic arrow's binder and callable descriptor through contextual object-literal
checking, then relate it through the existing cache-safe generic-signature path. Do not erase the
binder or specialize the stored signature to one context.

Acceptance is exact: the two surplus `TK2322` records disappear, nearby incompatible parameter
assignments remain errors, reversed query order is stable, and the required randomized differential
gate reports no unintended change against the pre-change binary.

## Touch points

Contextual arrow checking, retained signature descriptors, generic signature relation, focused
fixtures, the randomized differential harness, and the official-suite ratchet.

<!-- Origin: backlog-14 closure divergence-owner audit (2026-08-06). -->
