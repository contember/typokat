---
id: 68
title: same-name contravariant `infer` over-reports to `never` (should intersect)
---

# 68 — same-name contravariant `infer` over-reports to `never`

**Summary.** FP / tsc-parity tail (safe, over-report direction). Same-name `infer`
appearing in **multiple contravariant positions** resolves to a conservative `never`
where tsc **intersects** the candidates — rejecting values tsc accepts.

## Problem

Verified vs `tsc 6.0.3 --strict`:

```ts
type Foo<T> = T extends { a: (x: infer U) => void; b: (x: infer U) => void } ? U : never;
type R = Foo<{ a: (x: string | number) => void; b: (x: number) => void }>;
const r1: R = "hello"; // tsc: error (R = number);   typokat: error (R = never)
const r2: R = 123;     // tsc: clean  (R = number);   typokat: ERROR  (R = never)  ← over-report
```

tsc intersects the two contravariant candidates: `(string | number) & number = number`,
so `r2` is fine and only `r1` errors. typokat resolves `U` to `never`, so it rejects
`r2` too — a false positive (safe direction). With **disjoint** candidates
(`string`, `number`) both agree on `never`, which is why the divergence only shows on
overlapping candidates.

Note: an earlier note (M25 sprint / the pre-audit divergence ledger) described this as
"unions where tsc intersects". Empirically the current behavior is a
conservative-to-`never` over-report, **not** a union (a union would make `r1` clean —
an FN — which does not happen). The ledger entry was corrected in the 2026-07-07 audit.

## Approach / acceptance

Collect same-name `infer` candidates by **variance** (mirroring tsc's
`candidates`/`contraCandidates` split in `inferTypes`): covariant positions union
(existing behavior), contravariant positions **intersect** (`&`, available in the type
model since M31 — the missing wiring). tsc mechanism note (verified 2026-07-09): the
intersection is the **signatureless conditional-`infer` path**
(`getTypeFromInference` → `getIntersectionType(contraCandidates)`); the signature/call-site
path's `getCommonSubtype` single-candidate selection must NOT be copied here.

Acceptance: the overlap fixture above → `r2` clean, `r1` errors (`string` ≠ `number`);
the disjoint case → `never` in both (unchanged); covariant same-name `infer` still
unions. Cross-check `tsc 6.0.3`. Extend `m25_conditional_types/`.

## Touch points

`infer` candidate resolution in the inference engine (`crates/typokat-check/src/check/infer.rs`),
`m25_conditional_types/` corpus, `docs/reference/divergences.md`.

<!-- Origin: 2026-07-07 divergence-ledger audit (verified vs tsc 6.0.3); corrects the
     stale "contravariant infer unions" claim. -->
