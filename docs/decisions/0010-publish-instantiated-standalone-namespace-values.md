---
id: 0010
title: Publish instantiated standalone namespace values as immutable structural objects
status: accepted
date: 2026-07-16
---

# 0010 — Publish instantiated standalone namespace values as immutable structural objects

## Context

The pinned TS 6.0.3 ES5 proof validates type-side namespaces and reopenings but remains NO-GO on
`Intl.Collator()`: the `Intl` type container exists, while no standalone namespace value receiver
does. ADR-0009 deliberately made standalone namespaces type-container-only. Full ES5 model readiness
now requires reversing that one boundary without introducing a special resolver, partial mutable
surface, runtime semantics, or a second publication mechanism.

This ADR narrowly supersedes only ADR-0009's standalone type-container-only boundary. All unaffected
ADR-0009 rules remain binding: ordered declarations, public/private reopening scopes, existing
function/class value owners, query-free construction, atomic immutable publication, typed
unavailability, lexical event order, and qualified-path no-fallback behavior.

## Decision

Each standalone `NamespaceId` whose merged declaration group is **instantiated** owns one stable
`ValueStorageId`. Every reopening shares that storage. Distinct namespaces retain distinct storage
identity even if hash-consing deduplicates equal structural `TypeId`s. A non-instantiated namespace
has no value storage and remains a type container.

The checker builds one complete structural `ObjectType` from every public exported value member
across the namespace's reopenings. Private members remain fragment-local and never enter it. Nested
instantiated namespaces publish bottom-up; the parent property references the child's already-final
object. Group-level instantiation is the binder's existing transitive classification, not a
per-fragment choice.

Construction is query-free and reaches exactly one terminal state before ordinary expression
checking:

- `Ready { storage, TypeId }`, published atomically once; or
- typed `Unavailable`, retaining exact preallocated diagnostic/incomplete owners and publishing no
  prefix, empty, error, `any`, `unknown`, `never`, or otherwise fabricated object.

An unavailable required member or nested child makes the complete namespace value unavailable.
Containment cycles or unsupported value forms likewise stop with typed unavailability. No
evaluation, projection, relation, inference, or semantic query may run during construction, and a
published object is never extended by a later reopening.

The namespace symbol's ordinary value slot points to the namespace-owned storage. Root reads,
aliases, member access, calls, missing-member diagnostics, and assignment checks then use ordinary
value/object machinery; there is no qualified-leaf-only path. Property mutability follows the TS
6.0.3 oracle exactly: exported `const` is readonly; exported `let`, `var`, function, class, and
nested instantiated namespace properties are mutable. Unsupported `using`, import, enum, or other
forms remain unavailable until their exact owner permits them. The namespace binding itself is not
a writable variable.

Function+namespace and class+namespace groups keep their existing function/class `ValueStorageId`
and ADR-0009 draft/freeze protocol; no competing namespace storage is created. This decision models
only the declared value shape for type checking. It adds no JavaScript initialization, runtime
object, transformation, emit, module loader, or shared prelude store, and it does not expand
`declare global` value publication owned by backlog 82.

## Consequences

- `Intl` and other instantiated standalone namespaces become ordinary first-class checker values
  with stable provenance and immutable structural shape.
- Parent namespace availability depends on every required nested/value member, so unsupported
  shapes conservatively withhold the whole value rather than expose a permissive prefix.
- `PropertyType.readonly` becomes the exact namespace-member mutability contract and remains part
  of structural identity.
- Namespace storage identity and structural type identity remain separate; reopening and input
  order cannot change either publication result or event order.
- ADR-0009's no-special-resolver, no-attach-after-publication, and existing-owner rules continue to
  constrain implementation.

## Alternatives considered

### Resolve only qualified leaves

Best when namespace roots are permanently excluded as first-class values and only a narrow `.d.ts`
escape hatch is needed. Rejected because it duplicates ordinary value/member resolution, has no
honest root identity, and cannot generalize to aliases, nested chains, or assignment/mutability.

### Keep the safe over-report permanently

Best when full ES5/library readiness is removed from the product scope. It is cheap and
conservative, but it leaves backlog 14's accepted start gate permanently closed.

The selected design wins the weighted axes that matter here: ES5/model coverage (30%), soundness
and order independence (30%), reuse of existing architecture (20%), reversibility/extensibility
(10%), and delivery cost (10%). It clears the gate through ordinary machinery while retaining the
strongest publication boundary; the alternatives optimize cost or conservatism by giving up the
approved capability.

## Falsifiability and exit

Before production, spec and direct gates must prove one storage/type across reopenings and opposite
orders; distinct storages for equal-shape namespaces; ambient/export/private visibility; first-class
root reads and calls; nested/dotted bottom-up publication; non-instantiated no-value behavior;
`const` versus `let`/`var`/function/class/nested mutability; missing/private diagnostics; preservation
of function/class owners; complete parent unavailability; and zero construction queries or partial
publication.

Ordinary value machinery is not presumed safe merely because the namespace has a value slot. The
corpus must pin root assignment/update forms `N = ...`, `N++`, `++N`, and `N--` to `TS2631` parity
or one precise sound non-43 owner; calling the root `N()` must reach `TS2349` (backlog 19 if still
deferred), and `new N()` must reach `TS2351` (backlog 75 if still deferred).

Unavailable causes require an exact ledger: type query → 52; inferred initializer/function return
→ 76; enum → 42; import/import-equals → 15; duplicate → 18; TDZ/use-before-declaration → 47; and
class/static dependency cycles must be implemented or owned concretely (76 by default). Each cause
withholds the whole parent value and leaves zero broad backlog-43 fallback. A pure type-only
namespace must allocate no storage or empty object; root alias/read/call/`new`/member value access
must match `TS2708` or a documented sound owner/divergence.

A two-file `check_project` gate runs forward and reverse input order and requires identical storage
identity, structural type, publication state, and diagnostics. Event replay must retain the exact
`(original_module_ordinal, source_start, event_ordinal, record_ordinal)` order with no deduplication,
truncation, suppression, or completion-order drift.

The pinned proof must remove the sole backlog-43 incomplete, change `deep.Intl.value` from `TK2304`
to the expected semantic `TK2322`, and preserve every non-43 result explicitly. If exact behavior
requires a second resolver, query-time construction, mutable published objects, guessed mutability,
or produces a false negative/order dependency, stop, retain the safe incomplete, and leave backlog
14 blocked. No compatibility shim or partial fallback is accepted.
