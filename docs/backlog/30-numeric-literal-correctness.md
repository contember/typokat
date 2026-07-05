---
id: 30
title: Numeric literal correctness (negative literals lower to any; JS-exact stringification)
---

# 30 — Numeric literal correctness

**Summary.** Two related numeric-literal gaps, both unsound, found by the M27
adversarial review:

1. **Negative numeric literal types lower to the error/`any` type** — `const b: -1 = 5`
   passes silently, and `-1` is assignable to `{ a: 1 }` (probe `p27`/`p31`,
   review_m27). A bare-annotation, base-level (M0/M1) lowering gap: the unary-minus
   type expression isn't handled, so every type mentioning a negative number is
   fully permissive. Blast radius includes M27 template holes (`` `v${-1}` ``
   accepts anything — the M22 error-hole degradation being fed a spurious error).
2. **`number_to_string` is not JS `String(n)`** — plain decimal formatting, so
   `` `${1e21}` `` constructs `"1000000000000000000000"` where tsc's type is
   `"1e+21"` — typokat accepts the non-canonical form tsc rejects (an
   under-report, mislabeled "conservative" until the M27 close corrected the docs).

## Approach / acceptance

(1) Lower `-<numeric literal>` in type position to a proper negative literal type
(oxc: TSLiteralType with unary expression); corpus: annotations, unions,
template holes, extends checks. (2) Implement JS `String(n)` semantics
(exponential at ≥1e21 and <1e-6, shortest round-trip formatting — document any
residual divergence as over-report by rejecting, never accepting, non-canonical
forms). Cross-check tsc 6.0.3. Fixture corpus first, per dev-method.

## Touch points

`src/check/checker/annotations.rs` (literal type lowering), `src/types/repr.rs`
(`number_to_string`), m27 corpus extension (template holes with negative/large
numbers).

<!-- Origin: M27 adversarial review FN-1/FN-2 (2026-07-05). -->
