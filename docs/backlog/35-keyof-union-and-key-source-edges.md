---
id: 35
title: keyof over unions + never/template-literal mapped key sources (over-report tail)
---

# 35 — `keyof` over unions + edge key sources in mapped types

**Summary.** Three sound-direction over-reports from the M28 review, all "key source
not an iterable object → defer" cases tsc computes: (a) `Pick`/`Omit` over a UNION
operand (`Omit<A | B, "kind">` — common with discriminated unions), (b) `K = never`
(`Pick<P, never>` should be `{}`), (c) `Record` with template-literal keys
(`` Record<`p_${string}`, number> `` — a pattern index signature in tsc). Documented
as divergences in `docs/reference/divergences.md` at M28 close; this item removes them.

## Problem

`keyof (A | B)` (intersection of key sets — computable as shared literal keys without
an intersection node) and `never`/pattern key sources leave the mapped type deferred,
so valid member accesses/assignments over-report on real-world utility-heavy code.

## Approach / acceptance

Extend `build_keyof`/key iteration: union operand → shared keys; `never` source →
empty map; template-literal keys → pattern index signature (may depend on backlog-25
semantics for the general union case — verify, don't assume). Corpus first with tsc
6.0.3 cross-check; acceptance = the three probes above go clean and the README
divergence entries are deleted.

## Touch points

`src/check/checker/eval.rs` (`build_keyof`, mapped key iteration),
`src/relate/relation.rs` if pattern index signatures land.

<!-- Origin: M28 review round 1 (2026-07-05), MED sound-direction findings. -->
