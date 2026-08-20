---
id: 0022
title: Use one binder-authenticated declaration inventory
status: accepted
date: 2026-08-20
---

# 0022 — Use one binder-authenticated declaration inventory

## Context

The binder owns declaration identity, lexical scope, merge groups, and canonical fragment order.
The checker historically rediscovered alias, interface, class, and enum AST payloads with a second
statement walker that mirrored binder scope entry. Interface event reservation used another syntax
walk. These paths could disagree even when each was locally deterministic.

The disagreement became reachable in a script continuation. The binder attached a type declaration
inside a top-level block to the source module, while the checker walker started from a compilation
global and rejected the block because its parent did not match. The declaration group existed, but
reservation and publication never saw it. A later type demand reached a missing published terminal
and panicked.

A local fix to the walk root would remove that example, but it would retain parallel authorities
for payload discovery, owner occurrence order, scope reconstruction, and replay selection. The
active declaration-inventory sprint therefore built a behavior-neutral proof before changing any
semantic consumer.

That proof established the following on every production lifecycle route:

- one source manifest and one bound inventory are built per parsed target program;
- admitted rows preserve exact `DeclId`, `ScopeId`, `TypeGroupId`, source, fragment order, and
  selected `TypeGroup` membership;
- sparse replay joins only its authenticated affected source closure and user suffix without
  rebinding or scanning the complete frozen prefix;
- unsupported source placements are explicit rejections; and
- reservation, publication, class construction, and semantic checking remain unchanged.

## Decision

### Build one source manifest and consume it twice

Each parsed `Program` builds one `SourceDeclarationManifest` with the existing exhaustive binder
visitor. The manifest contains declaration occurrence rows, alias/interface/class/enum AST
payloads, source placement, and one flat interface occurrence array with per-interface ranges.

The route passes that same manifest to semantic binding. The binder uses its declaration rows to
preallocate source declaration identities, then drops the manifest reference. After binding, the
checker consumes and owns the manifest in one `BoundTypeDeclarationInventory` for that syntax
module and source.

Canonical production routes use manifest-aware binder entry points. Compatibility entry points may
build a transient manifest and delegate, but a route that later constructs an inventory must not
invoke them and must not walk the source again for declaration payloads.

### Keep binder identity and group order authoritative

The inventory joins a source type row through the binder's exact declaration site. An admitted row
must match the complete declaration and binding spans, declaration kind, syntax module, source,
attached lexical scope, and attached type group. Its `DeclId` must occur exactly once in that
group's canonical `TypeGroup::fragments`; that vector is the only fragment-order authority. A
stored fragment index is an authenticated locator and receipt field, not a second iteration order.

The inventory uses an offset-dense map over the joined `DeclId` interval for one module. It must
not size storage to the complete binder declaration table, retain another group-order vector, add
an exact-site index, or scan an unaffected frozen prefix.

Every binder-authenticated selected fragment must have exactly one manifest payload. Missing,
duplicate, mismatched, or selected-without-payload rows are construction errors. The source visitor
records an explicit placement for syntax that current binder policy does not admit. This includes
class expressions, unadmitted default-class candidates, declarations inside an unsupported ambient
external-module body, and declarations without a type group inside an issue-bearing invalid global
augmentation. The latter must match the exact global-augmentation site and its binder issue record;
a missing or clean record is not rejection evidence. A source-only rejection creates no type-group
terminal. Missing scope or group metadata on an ordinary placement remains a construction error;
it is not evidence of deliberate rejection.

### Keep the inventory out of the semantic type model

The inventory is an authenticated construction input, not a second type registry or publication
authority. `TypeDeclTable`, the existing reservation/fill pipeline, and one atomically published
type environment remain the semantic model. The binder, `LibraryBinderCheckpoint`, frozen library
provider, and replay plan remain AST-free.

Consumer cutover must proceed in existing phase order:

1. interface header, heritage, and member owners consume manifest occurrence ranges;
2. type reservation, fill, publication, and selected replay consume inventory rows in binder group
   order; and
3. class attachment and trusted-prelude validation consume the same inventory.

During cutover, the legacy checker walker remains the production authority for each consumer that
has not moved and also serves as the parity oracle. It may disagree solely on the proven
script-continuation top-level block-root defect. It becomes oracle-only after the final production
consumer moves, and is then removed once the parity gate is clean.

### Preserve route ownership and sparse replay boundaries

Fresh, complete-source, library-only, shared-provider, checkpoint, and sparse routes own manifests
beside their parsed programs only for the duration of the build. No manifest or inventory crosses
into a frozen runtime product.

Sparse replay reparses and inventories the authenticated affected library source closure plus user
programs. It joins those rows to retained prefix `DeclId` values and marks selection from the exact
affected `TypeGroup` owner set. Reparsed source membership does not itself select every row in that
source. Missing selected payloads fail the epoch; they never trigger a late walk, rebind, or full
prefix fallback inside the inventory.

Production selected reservation currently admits a group when either its `TypeGroup` owner or its
class owner is selected. WU2 proves only the `TypeGroup` side. WU3b must preserve and test the
existing class-owner alternative, or separately prove that every selected class necessarily
selects its containing `TypeGroup`, before inventory rows replace that consumer.

## Consequences

- Declaration payload discovery, binder identity, scope, merge order, and interface owner order
  now have one explicit join boundary.
- Local and control-flow declaration support can expand without adding another scope-mirroring AST
  walker.
- Route plumbing carries short-lived AST-bearing manifests until inventory construction. Frozen
  products and public semantic identities remain unchanged.
- Construction becomes fail-closed: a new binder fragment without a payload, or a payload attached
  to the wrong site, stops before semantic checking.
- Explicit rejection accounting is part of the inventory contract. A rejection requires a named
  source placement; an ordinary missing attachment remains binder corruption.
- The offset-dense representation may include gaps between type declarations in one module, but its
  cost is bounded by that module's joined declaration interval rather than the frozen global
  prefix.
- During cutover, the legacy walker remains authoritative for unmoved consumers while also proving
  parity. This temporary duplication must be removed after the final consumer moves.

## Acceptance and falsification

The decision must be revisited if any consumer cutover requires:

1. a second source walk or second exact-site/group-order index;
2. AST references in `Binder`, a checkpoint, provider, frozen runtime, or replay plan;
3. work proportional to an unaffected frozen prefix on shared or sparse routes;
4. semantic publication outside the existing checker pipeline;
5. source order replacing `TypeGroup::fragments` order;
6. a missing authenticated payload being downgraded to an ordinary rejection; or
7. a route building more than one manifest or inventory for the same target program; or
8. selected reservation drops the existing class-owner alternative without proving equivalent
   `TypeGroup` selection.

The permanent route receipts must retain exact manifest/inventory cardinality, admitted/rejected
row accounting, join attempts, dense-slot bounds, group identities, fragment order, and sparse
selected-`TypeGroup` membership. Class-owner selection retains a separate exact witness until its
equivalence is proven.

## Alternatives considered

### Fix only the script-continuation walker root

This is the smallest correction for the observed panic. It loses because scope and payload
discovery would still be reconstructed independently by the checker and interface owner walk, so a
different syntax or route could recreate the same class of omission.

### Store AST payloads in the binder or frozen provider

This makes declaration identity and payload lookup direct. It loses because binder and provider
products intentionally outlive parser arenas and remain AST-free. Adding AST lifetimes would also
change checkpoint and shared-base boundaries.

### Rewalk the AST after binding

A second visitor can join payloads to binder sites without changing binder APIs. It loses because
the route would retain two declaration traversals and could not prove once-per-source work or
shared ordering by construction.

### Add a semantic declaration registry or patch missing terminals late

Either option can hide an omitted declaration from the current pipeline. Both lose because they
create a second publication authority and make phase-order failures observable only at demand
time.

### Allocate one dense table over all binder declarations

This simplifies lookup. It loses on shared and sparse routes because it materializes or scans the
unaffected frozen prefix, contrary to ADR-0020's source-native sparse boundary.
