---
id: 75
title: Diagnostic scope and deferred-feature disposition tail
blocked-by: []
---

# 75 — Diagnostic scope and deferred-feature disposition tail

**Summary.** Close the gap between the declared Tier S/A/B scope and the executable 1.0
manifest: each remaining family is implemented with a witness or explicitly moved out of scope
with a documented, sound boundary.

## Problem

The scope map names semantic families and exact diagnostics that do not yet have a dedicated
owner. That lets the manifest validate links while silently omitting part of the claimed surface.
The currently unowned tail includes:

- Tier S: weak-type/no-common-properties (`TK2559`), multiple-property and suggestion code
  fidelity (`TK2739`/`TK2740`, `TK2551`), value/type-space misuse (`TK2693`), static-side extends
  (`TK2417`), and implements compatibility (`TK2420`);
- Tier A: unknown receivers (`TK2571`) and non-callable-vs-non-constructable parity
  (`TK2348`/`TK2351`; `TK2349` stays owned by `19`);
- Tier B: indexed access/index-signature compatibility (`TK2536`/`TK2411`), implicit `this`
  (`TK2683`), accessor compatibility (`TK2379`/`TK2380`), reachability
  (`TK7027`), delete-operand checking (`TK2790`), decorators, and computed/symbol properties;
- deferred model shapes without an exact dedicated diagnostic: type-parameter defaults, optional
  tuple elements, and generic/deferred `T[K]` outside mapped templates;
- silent/deferred parity families already recorded in `divergences.md` but lacking a stable owner:
  fresh-literal constraint-side excess checks, unequal-raw-arity and generic-base override checks,
  visibility/kind heritage checks (`TS2415`/`TS2425`/`TS2426`), private-base construction
  (`TS2675`), mapped key remapping/template keys and non-literal key sources, unsupported utility
  aliases/`intrinsic` declarations that degrade to the error type,
  generic `keyof`/intersection key domains, call arguments dropped from `arg_types` (backlog
  `63g`), and any other deferred entry found by the required parity census.

Template-expression traversal and spread expressions are not duplicated here: their concrete
silent-skip owner is [`71`](./71-expression-inference-fn-tail.md), enforced systematically by
[`73`](./73-unsupported-surface-audit.md). Iterability belongs to `71`; optional
methods/accessors belong to `49`; enums/namespaces/`this` parameters belong to `42`/`43`/`70`.

## Approach / acceptance

Turn `scope.md` into a canonical family inventory with stable ids and require every in-scope
family id to map to an incomplete owner or a shipped witness in `completion-1.0.toml`. Work this
tail family-by-family, spec first. For each family, either:

1. implement it and cross-check a positive/negative corpus against `tsc 6.0.3 --strict`; or
2. make an explicit scope change to OOS, documenting why it is orthogonal to the type model and
   how unresolved use stays sound/non-permissive.

Merely adding a divergence entry is not enough to remove an in-scope family. Moving a family OOS
must update the canonical scope inventory, divergence/user-facing limitations where applicable,
and the manifest in the same change. Close this item only when it owns no remaining family and the
scope-to-manifest validator proves full disposition coverage.

**Census infrastructure shipped (2026-07-10 completeness-accounting sprint, WU6 — do not redo).**
The one-time divergence census this item demanded is done and machine-enforced: every entry in
`divergences.md` carries a validated inline marker (stable id, `under`/`over`/`cosmetic`
direction, scope disposition, owner, witness; `tests/divergences.rs` rejects unmarked rows,
duplicates, dead links, and every ownerless under-report), and manifest `deps` are cross-checked
against owner `blocked-by` frontmatter with an explicit `deps_exception` mechanism
(`tests/manifest.rs`) — the `14`/`70` drift class is now a failing test. Manifest criterion
`C-deferred-divergence-census` is complete. **What remains here is the semantic tail itself**:
implementing or explicitly re-scoping the families listed above, each already owned and
witnessed in the structured ledger.

## Touch points

`docs/reference/scope.md`, `docs/backlog/completion-1.0.toml`, `tests/manifest.rs`, focused
conformance corpora, checker/binder/type-store paths selected per family, and diagnostics.

<!-- Origin: post-sprint scope-to-manifest completeness audit, 2026-07-10. -->
