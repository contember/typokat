---
id: 66
title: protected↔protected override compatibility (TK2416)
blocked-by: [./63-review-parity-tail.md]
---

# 66 — protected↔protected override compatibility (TK2416)

**Summary.** Track C silent-FN. Override-compatibility (`TK2416`) is checked
**public↔public only**; an incompatible `protected`-over-`protected` override is
skipped, so a genuine tsc `TS2416` is **dropped**.

## Problem

Verified vs `tsc 6.0.3 --strict`:

```ts
class Base { protected m(x: string): void {} }
class Derived extends Base { protected m(x: number): void {} }
```

tsc reports `TS2416` on `Derived.m` ("Property 'm' … is not assignable to the same
property in base type 'Base'."); typokat is **clean** — a dropped error.

The b06 override check deliberately restricted itself to public↔public because the
nominal relation would otherwise also reject a *legal* protected redeclaration (a
compatible protected override — same-signature — must stay clean; the nominal
foreign-declaration guard rejects it structurally). Documented in
[`../reference/divergences.md`](../reference/divergences.md) (Classes) as a declared
false negative.

## Approach / acceptance

**Acceptance spec (ready):** the disabled focused
[`tests/cases/b66_protected_override_compat/`](../../tests/cases/b66_protected_override_compat/)
corpus pins incompatible methods/fields, compatible and covariant controls, visibility
boundaries, and the nested protected-lineage stop gate.

The 2026-07-11 implementation attempt proved this item is blocked by backlog `63(d)`.
Admitting protected pairs to the existing override queue correctly reports primitive
mismatches, but nested types whose derived class legally redeclares a protected member are
rejected as unrelated nominal origins. The global relation currently sees exact
`declaring_class` identity but not base→derived lineage. The
`nested_protected_lineage.ts` control must stay clean before this corpus can be enabled.

Run the existing `TK2416` variance query (method bivariant / field strict, keyed on
the base member's declaration kind — already implemented for public) on
`protected`↔`protected` pairs, comparing **signatures structurally** and bypassing the
nominal same-declaration guard *for the override-compat check specifically*. Fix `63(d)`
with lineage-aware protected-origin handling without weakening private identity or
accepting unrelated protected classes; then reuse that policy here.

Acceptance: the fixture above → `TK2416`; a *legal* compatible protected override
(`protected m(x: string)` over `protected m(x: string)`, or a covariant-return
narrowing) stays **clean**; no regression on the public↔public b06 corpus. Extend
`b06_class_completeness/` (or a focused `b66_*` dir per the bug-fix corpus
convention). Cross-check `tsc 6.0.3`.

Out of scope (still deferred, separate): `TS2415` (visibility narrowing /
private-member redeclaration) and `TS2417` (static-side override).

## Touch points

The override-compat check in the class-completeness path (`crates/typokat-check/src/check/checker/…`), the
nominal relation guard (`src/relate/…`), `b06_class_completeness/` corpus,
`tests/conformance.rs`, `docs/reference/divergences.md`.

<!-- Origin: 2026-07-07 divergence-ledger audit (verified vs tsc 6.0.3). Was a
     declared FN in the b06 scope note, unfiled until the audit. -->
