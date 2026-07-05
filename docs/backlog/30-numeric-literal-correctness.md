---
id: 30
title: JS-exact number stringification (String(n) dtoa)
---

# 30 — JS-exact number stringification

**Summary.** `number_to_string` (`src/types/repr.rs`) uses plain decimal formatting,
not JS `String(n)`: `` `${1e21}` `` constructs `"1000000000000000000000"` where tsc's
type is `"1e+21"` — typokat ACCEPTS the non-canonical form tsc rejects (an
under-report). The same routine validates `${number}` pattern segments, so large
magnitudes accept digit strings tsc rejects. (The negative-literal half of this item
shipped in the soundness-warmups sprint, 2026-07-05 — `-<literal>` lowers correctly
in both positions, `-0`≡`0`.)

## Approach / acceptance

Implement JS `String(n)` semantics: exponential form at ≥1e21 and <1e-6, shortest
round-trip formatting (Ryū/Grisu-style — a small dtoa crate is acceptable if the
no-new-deps posture allows; otherwise implement the exponential-threshold subset and
REJECT ambiguous forms — over-report, never accept a non-canonical string). Corpus:
template-hole construction and `${number}` pattern matching at the thresholds;
cross-check tsc 6.0.3.

## Touch points

`src/types/repr.rs` (`number_to_string`), m27 pattern-matching relation arm,
m27 corpus extension.

<!-- Origin: M27 review FN-2; re-scoped after the warm-ups sprint shipped the
     negative-literal half (2026-07-05). -->
