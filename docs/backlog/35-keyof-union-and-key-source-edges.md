---
id: 35
title: never/template-literal mapped key sources (over-report tail)
---

# 35 — edge key sources in mapped types

**Summary.** Remaining sound-direction over-reports from the M28 review where the key
source is not yet iterable in typokat but tsc computes it: (a) `K = never`
(`Pick<P, never>` should be `{}`), (b) `Record` with template-literal keys
(`` Record<`p_${string}`, number> `` — a pattern index signature in tsc). The
`keyof (A | B)` / `Pick`/`Omit` common-key slice shipped as a b34 byproduct in the
2026-07-08 soundness-tail sprint.

## Problem

`never` and pattern key sources leave the mapped type deferred, so valid
member accesses/assignments over-report on real-world utility-heavy code.

## Approach / acceptance

Extend mapped key iteration: `never` source → empty map; template-literal keys →
pattern index signature (may depend on backlog-25 semantics for the general union
case — verify, don't assume). Corpus first with tsc 6.0.3 cross-check; acceptance =
the remaining probes above go clean and the README divergence entries are deleted.

## Touch points

`crates/typokat-check/src/check/checker/eval.rs` (mapped key iteration),
`src/relate/relation.rs` if pattern index signatures land.

**Follow-up (2026-07-10, completeness-accounting sprint WU5):** an ALIASED keyof as a
non-homomorphic key source (`type Keys = keyof Obj; { [K in Keys]: Obj[K] }`) collapses
the key source — and the alias itself — to `never` (over-report; ledger
`mapped/aliased-keyof-key-source`, witness
`tests/cases/sr_deferred_ledger/b35_aliased_keyof_mapped.ts`).

<!-- Origin: M28 review round 1 (2026-07-05), MED sound-direction findings. -->
