---
id: 75
title: Diagnostic scope and deferred-feature disposition tail
blocked-by: []
---

# 75 — Diagnostic scope and deferred-feature disposition tail

**Summary.** Close the gap between the declared Tier S/A/B scope and the executable 1.0
manifest: each remaining family is implemented with a witness or explicitly moved out of scope
with a documented, sound boundary.

The pinned TS 6.0.3 ES5 readiness gate assigns 173 explicit annotation incompletes here:
polymorphic `this` 164, `intrinsic` 5, `symbol` 3, and `bigint` 1. This is independent
1.0 work and does not prevent backlog `14` from starting. The exact
sites and counts are enforced by
[`readiness.toml`](../../tests/fixtures/lib-es5-6.0.3/readiness.toml).

## Problem

The scope map names semantic families and exact diagnostics that do not yet have a dedicated
owner. That lets the manifest validate links while silently omitting part of the claimed surface.
The currently unowned tail includes:

- Tier S: weak-type/no-common-properties (`TK2559`), multiple-property and suggestion code
  fidelity (`TK2739`/`TK2740`, `TK2551`), value/type-space misuse (`TK2693`), static-side extends
  (`TK2417`), and implements compatibility (`TK2420`);
- Tier A: unknown receivers (`TK2571`) and non-callable-vs-non-constructable parity
  (`TK2348`/`TK2351`; `TK2349` stays owned by `19`), including `TK2348` code fidelity when a
  construct-only object is called;
- Tier B: indexed access/index-signature compatibility (`TK2536`/`TK2411`), implicit `this`
  (`TK2683`), accessor compatibility (`TK2379`/`TK2380`), reachability
  (`TK7027`), delete-operand checking (`TK2790`), decorators, and computed/symbol properties;
- deferred model shapes without an exact dedicated diagnostic: type-parameter defaults, optional
  tuple elements, and generic/deferred `T[K]` outside mapped templates;
- assertion source/target overlap validation (`TK2352`);
- exception/catch flow, async/generator expression semantics, bigint/symbol/unique and
  literal-type forms, private expressions, and class static blocks, auto-accessors, and index
  signatures; polymorphic `this` type annotations/qualified `this` type names (distinct from the
  shipped explicit receiver slot and contextual `ThisType<T>` marker);
- silent/deferred parity families already recorded in `divergences.md` but lacking a stable owner:
  fresh-literal constraint-side excess checks, unequal-raw-arity and generic-base override checks,
  visibility/kind heritage checks (`TS2415`/`TS2425`/`TS2426`), private-base construction
  (`TS2675`), mapped key remapping/template keys and non-literal key sources, unsupported utility
  aliases/`intrinsic` declarations that degrade to the error type,
  generic `keyof`/intersection key domains, call arguments dropped from `arg_types` (backlog
  `63g`), and any other deferred entry found by the required parity census.

The constraint tail also owns two under-report records left after backlog `67` shipped:
fresh-literal inference candidates exempt from constraint-side excess checks, and the
canonical Omit/deferred-`keyof` constraint check whose completeness across every
concretization path is not yet proved. They remain structured divergence entries until
implemented or explicitly re-scoped.

Certified well-known-symbol publication exposes one additional generic-interface heritage
family in the full-library census. `HTMLCollectionOf<T>` now contributes one surplus `TK2430`,
and `NodeListOf<T>` contributes four: its three pre-existing `forEach`/`entries`/`values` rows plus
the newly represented `[Symbol.iterator]` row. All five fail at the same `ArrayIterator<T>`
relation to `ArrayIterator<Element>` or `ArrayIterator<Node>` through the overloaded
`IteratorObject.reduce` callback. Strict tsc 6.0.3 with the exact `es2025.full` library set is
clean. This is general method-variance work, not a library-provenance special case; a fix must
retain a negative computed-member heritage control. The associated same-site `TK2344` text change
at `lib.es5.d.ts:1873:155` is truthful rendering of ArrayBuffer's published
`[Symbol.toStringTag]`, not a removed diagnostic.

Template-expression traversal, elisions, object/call spreads, tagged templates, and iteration
targets are not duplicated here: their concrete silent-skip owner is [`71`](./71-expression-inference-fn-tail.md),
and the shipped surface inventory enforces their accounting. Iterability belongs to `71`; optional
methods/accessors belong to `49`; enums belong to `42`; namespace type/value publication is shipped,
while ambient external modules remain `15`; explicit `this`
parameters are shipped.

The post-WU7 official adjudication adds exact witnesses without reopening shipped tuple labels or
local generic class heritage:

- Local plain-identifier generic class heritage is shipped. The official run correctly drops 42
  stale `class/class-heritage/type-arguments` outcomes: 3 in
  `classWithBaseClassButNoConstructor.ts`, 1 in `privateNamesConstructorChain-2.ts`, 29 in
  `subtypesOfTypeParameterWithConstraints.ts`, and 9 in
  `subtypesOfTypeParameterWithConstraints4.ts`. `derivedClassTransitivity3.ts` and
  `derivedGenericClassWithAny.ts` move into scope without a new false positive, while
  `objectTypesIdentityWithPrivates2.ts` is a clean positive. The `object` keyword type is shipped;
  none of these belongs to this item.
- `numericIndexerConstrainsPropertyDeclarations.ts`,
  `stringIndexerConstrainsPropertyDeclarations.ts`, and both
  `subtypesOfTypeParameterWithConstraints{,4}.ts` retain
  `class/class-index-signature/self`; class index signatures remain this item's boundary.
- `contextualTypeWithTuple.ts` records `interface/heritage/topology` for an interface extending a
  tuple alias and therefore moves `IN` → `OOS:unsupported`.
  `arityAndOrderCompatibility01.ts` makes the same state transition through the implicit
  standard-library `Array` and is owned by backlog `14`, not by generic class heritage.
- `partiallyNamedTuples{,2}.ts` prove labels are transparent while conditional/mapped rest
  containers that are not provably array-like retain
  `annotation-lower/tuple-rest-element/non-array`; `partiallyNamedTuples3.ts` remains owned by
  backlog `71` solely for spread-call traversal. In `partiallyNamedTuples2.ts`, `Iterable` is a
  backlog `14` library dependency, while `null!` remains backlog `49`'s expression boundary.
- `dependentDestructuredVariables.ts` exposes the existing selected-key listener over-report after
  its event tuple labels lower; the multi-key `Events[K]` callback still conservatively sees the
  whole tuple union.
- `assertion_compatibility.ts` pins the hidden false-clean `TS2352` family for both `as` and
  angle-bracket assertions. The asserted type may remain the expression result, but publication
  must not skip source/target overlap validation.

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

For the witnesses above, acceptance is exact: class-index and interface-topology cases remain
non-permissive until implemented; tuple-label controls never regain a named-member incomplete;
rest proof failures emit only their own stable identity; selected-key callbacks match strict tsc
without duplicate `TK2345`; and both assertion forms emit `TK2352` without suppressing independent
child diagnostics. Calling a construct-only object reports `TK2348`, not the generic `TK2349`, while
`new` remains accepted. Closing the generic iterator heritage family removes the five surplus
census rows through a general relation rule while an incompatible `[Symbol.iterator]` override
still reports `TK2430`.

**Census infrastructure shipped (2026-07-10 completeness-accounting sprint, WU6 — do not redo).**
The one-time divergence census this item demanded is done and machine-enforced: every entry in
`divergences.md` carries a validated inline marker (stable id, `under`/`over`/`cosmetic`
direction, scope disposition, owner, witness; `tests/divergences.rs` rejects unmarked rows,
duplicates, dead links, and every ownerless under-report), and manifest `deps` are cross-checked
against owner `blocked-by` frontmatter with an explicit `deps_exception` mechanism
(`tests/manifest.rs`) — dependency drift between a criterion and its owner's `blocked-by` list is
now a failing test. Manifest criterion
`C-deferred-divergence-census` is complete. **What remains here is the semantic tail itself**:
implementing or explicitly re-scoping the families listed above, each already owned and
witnessed in the structured ledger.

## Touch points

`docs/reference/scope.md`, `docs/backlog/completion-1.0.toml`, `tests/manifest.rs`, focused
conformance corpora, `crates/typokat-check/src/check/checker/annotations/`, checker/binder/type-store paths selected per
family, and `crates/typokat-diagnostics/src/diagnostics/`.

<!-- Origin: post-sprint scope-to-manifest completeness audit, 2026-07-10. -->
