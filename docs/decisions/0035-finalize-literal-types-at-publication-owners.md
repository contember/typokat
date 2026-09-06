---
id: 0035
title: Finalize literal types at their publication owners
status: accepted
date: 2026-09-06
---

# 0035 — Finalize literal types at their publication owners

## Context

Repeated contextual-literal repairs preserved values but changed their later widening behavior.
Shared predicates meant different things to member, return and class producers: retain the current
mode, make a literal non-widening, or leave a referenced container unchanged. Callers selected mode
conversion separately, allowing half of a semantic decision to be applied.

Strict TypeScript 6.0.3 also distinguishes publication boundaries. Under a `boolean | number`
element context, `[flag ? true : 1]` publishes regular `true | 1`, while `[true, 1]` publishes
`true | number`. An inferred callable returning the same conditional publishes widening
`true | 1`, so an immediate exact assignment succeeds but its mutable copy becomes
`boolean | number`. Contextual finalization neither distributes over source unions nor commutes
with combining separately produced elements.

The immutable representation in [0031](0031-preserve-literal-widening-in-immutable-type-identity.md)
already carries the required information. The defect is ownership and timing, not missing state.

## Decision

Keep literal policy in a checker-local `literal_publication` owner. Its operation takes the complete
source identity, effective contextual type when applicable, and explicit publication role. It returns
the completed `TypeId`, not a public preservation boolean or action that callers must apply later.
Use existing shallow literal/union regularization and wrapper-only widening. Do not move checker
policy into the type store or create a second provenance graph.

### Publication roles and boundaries

- **Mutable members and array/tuple elements:** decide contextual literal-kind matching on the
  complete expression result. If it matches, regularize; otherwise widen then regularize. Finalize
  each expression before member publication or element-type collection. Matching is not an
  assignability check: an incompatible source literal remains itself and is rejected by its owner.
- **Inferred callable return:** collect complete IDs from all value returns already visited by the
  existing walker, combine once, then apply the inferred-return operation. Apply contextual literal
  finalization to supported single-unit results; preserve non-unit results and their modes. An
  expression body uses the same operation on its complete expression result.
- **Mode-retaining declaration:** `const` and ordinary readonly-field publication retain the source
  mode where required. Readonly does not imply a non-widening literal.
- **Const-owned expression:** syntax-directed construction owns readonly properties/tuples and
  regularizes its admitted leaves. This does not recursively transform referenced containers.

Explicit annotations retain their existing validation and publication authority. They are not a
contextual hint and do not enter the inferred-return policy. Ordinary function and arrow bodies use
the same inferred-return timing; do not add an arrow-only source-collection compatibility path.
Keep existing `Declared`, `Inferred` and satisfies reporting ownership distinct where their effects
differ, even when they use the same type operation.

Broad same-kind target alternatives absorb their literal alternatives for matching purposes:
`"a" | string` behaves as `string` in either order. A broad numeric arm must not veto a matching
boolean contribution of a composite source. Do not replace symbolic contextual parameters by their
bounds or add constraint-order discovery. Retain identity distinctions of existing supported contexts.

### Capability and readiness

Context projection, admission, readiness and expression execution remain outside finalization.
The operation constructs immutable nodes from already available inputs; it cannot query relations,
demand declarations, adopt effects or walk syntax. Unavailable context and exhaustion are not an
absent context or a successful identity result. Preserve existing typed failure paths; a new required
query/readiness seam is a blocker, not permission for permissive fallback.

`SurfaceTypeFactory` may forward the same checker-local operation through its private interner.
Do not expose the interner or give class construction a `Pass`, evaluator or relation capability.
Class primitive producers must retain ordinary literal IDs until the role is applied; a broad
intrinsic produced earlier cannot be repaired at the endpoint. Existing class expression admission
remains unchanged, including its narrower supported arrow bodies.

### Scope and acceptance

Change the timing of existing value-return aggregation, not return discovery: bare-return
contributions, fallthrough, reachability, throw/never exits and missing-return diagnostics remain
with backlog 46. No source-construction, general lazy-declaration or new inference pipeline follows.

Acceptance combines direct mode/identity tests with source publication, projection, mutable copy,
and valid/invalid writes. Cover grouping, role differences, same-value collapse, mixed regular and
widening contributions in both orders, broad contexts, primitive references, container references,
class publication and explicit annotations. Preserve exactly-once defaults, selected/rejected
candidate effects and existing diagnostic owners. Extend existing regression tooling rather than
creating another harness; independent review and classified differential gates remain required.

## Consequences and alternatives

This consolidates policy while preserving different semantic publication roles. All affected
consumers migrate coherently; no small helper-only fix is accepted. A finished identity is sufficient
for this operation, but does not replace the wider expression/effect protocol.

A staged expression product carrying both source and published identities would be justified if a
retained defect required independently consumed results to cross this boundary together. The current
defects do not establish that need. Whole-result flags, blanket memberwise decisions, universal
regularization, and an arrow-only legacy split each contradict observed source behavior.

This decision approves the contract, not a candidate or sprint completion. Implementation and gate
receipts belong to the [active sprint](../sprints/sprint-2026-08-17-as-const.md).
