---
id: 65
title: Multi-argument inference unions candidates instead of fixing-then-checking (silent FN)
---

# 65 — Multi-argument candidate union drops per-argument errors

**Summary.** When one type parameter is inferred from several arguments,
`infer_type_arguments`/`fix_candidates` (`src/check/infer.rs`) **unions** the candidates
and checks each argument against the union, instead of fixing `T` and then checking each
argument against the fixed type — so a genuinely-incompatible argument is silently
accepted. `declare function pair<T>(x: T, y: T): void; pair(1, "s")` is clean in typokat
but tsc reports `TS2345 '"s"' not assignable to '1'`; same for `f2<T>(a: T, b: T);
f2(1, "x")` and — since b57 — `f<T>(a: T[], b: T); f(tuple, "x")` (the tuple arg now
infers `number` for `T`, unioning with the scalar, exactly like an array arg does). A
silent false-negative family in ordinary generic calls. Leader- and review-verified
2026-07-07 (the b57 WU5 review built a pre-fix worktree binary to confirm the gap is
pre-existing and general — array-literal and pure-scalar variants drop the error
independently of b57).

## Problem

tsc's inference fixes each type parameter to a single type (the common supertype /
first-candidate per its priority rules) and then checks every argument against the fixed
parameter; a mismatched argument fails `TS2345`. typokat's union-of-candidates makes the
parameter as wide as the arguments demand, so every argument trivially satisfies it. The
one fixture that touches this (`m10_inference/inference_multi.ts`) currently pins the
typokat behavior as "ok" and the divergence is **not** in the `tests/cases/README.md`
ledger.

## Approach / acceptance

Adopt tsc's fix-then-check discipline for multi-source parameters: pick the fixed type
per tsc's candidate-priority rules (supertype of the covariant candidates, respecting
literal vs widened), then run each argument's `TK2345` check against the fixed type.
Object-literal candidates keep the fresh-literal reshaping exemption (existing M24 rule).
Corpus: same-`T` scalar pairs (literal + widened), tuple/array-plus-scalar mixes (the b57
surface), contravariant-position candidates (must still intersect), cases that must STAY
clean (genuinely-common-typed args). Cross-check tsc 6.0.3 --strict; update
`inference_multi.ts` and delete the README divergence gap.

## Touch points

`src/check/infer.rs` (`fix_candidates` / `infer_type_arguments` candidate resolution),
`m10_inference/inference_multi.ts`, `tests/cases/README.md`.

<!-- Origin: cross-cutting review 2026-07-07 (was backlog 63 item j, reclassified from
     doc-ledger to silent-FN after the b57 WU5 review confirmed dropped TS2345). -->
