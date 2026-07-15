---
id: 0009
title: Publish ordered declaration groups and namespace surfaces atomically
status: accepted
date: 2026-07-15
---

# 0009 — Publish ordered declaration groups and namespace surfaces atomically

## Context

The binder already gives one name independent value, type, and namespace slots, but a repeated type
declaration currently replaces the previous `DeclId`. The checker then rediscovers the surviving
AST declaration and reserves or fills only that declaration. This cannot represent reopened
interfaces or namespaces, and extending it with a last-declaration search, fragment intersections,
or mutable completed objects would make the visible type and its diagnostics depend on declaration,
construction, or query order.

The WU0 corpus for backlog `43` fixes the TypeScript 6.0.3 oracle for repeated interfaces,
namespace reopening and visibility, qualified lookup, generic-header conflicts, recursion,
function/class augmentation, full class+interface(+namespace) merging, global augmentation, and the
relevant context diagnostics. Several results are intentionally non-obvious: an ambient namespace
exports members by default; private members do not cross namespace reopenings; identical overloads
retain the first/source-order surface where the oracle does; and even an invalid generic group keeps
class and interface members typed, including distinct parameter positions after an arity or name
mismatch.

[ADR-0006](0006-immutable-class-instances-and-scc-publication.md) and
[ADR-0008](0008-class-surface-lowering-and-lexical-event-ownership.md) already prohibit mutable
published class types, partial application vectors, construction-time semantic queries, and
completion-order diagnostics. Declaration merging must extend that protocol rather than introduce
a second publication mechanism. The checker remains a type checker, not an emitter: a standalone
namespace is needed as a named type container, but a standalone namespace runtime object is outside
the product boundary.

The ownership boundaries also need to be exact. `declare global` changes type-side name publication
and belongs to backlog `43`. `export as namespace` publishes an external module's export surface
under a global UMD alias. That external surface may be supplied by `export =` or by ordinary named
exports, so its general semantics belong to backlog `15`. The WU0 corpus keeps an `export =` form as
the immediate ownership witness without claiming that it is the only form. Enum/function
duplicate-kind diagnostics and the exact enum/function/namespace chimera require enum semantics and
belong to backlog `42`.

## Decision

### Lexical declarations and stable semantic groups

Every source declaration receives a unique lexical `DeclId` from one unified declaration identity
space. A repeated declaration never reuses or replaces another source declaration's identity. The
binder records the exact AST site for every `DeclId` and records each declaration occurrence's
event owner at that same lexical site. Lexical declaration identity is distinct from the checker
storage identity used by a value declaration and from type-group identity; none of those three ids
may be substituted for another merely because the current implementation uses a `DeclId`-indexed
table for value types.

In the final representation, the value slot names its dedicated value-storage identity, the type
slot of a `Symbol` names a stable `TypeGroupId`, and the namespace slot names a `NamespaceId`. A type
group owns an ordered vector of lexical declaration fragments and the exact `DeclId -> AST site`
map needed to lower each fragment once. The three slots remain independent; occupying the same name
is not by itself semantic composition. Legal composition is classified explicitly from the ordered
slot contents.

WU1a creates these identities, maps, and ordered groups as dormant metadata. It does not change
`Symbol.ty` or any production lookup/publication consumer. WU3 replaces the production type slot
and every consumer atomically. There is no transition adapter that selects a first, last, or
otherwise winning `DeclId` from a `TypeGroupId` for the legacy path.

The checker may use mutable private drafts while constructing a group, but the binder's source
identities and order are final before lowering starts. There is no lookup for "the declaration that
won," no scan for the last same-name AST node, and no reconstruction of source order from a symbol
slot.

### Namespace scope ownership and qualified lookup

Each merged namespace has one stable `NamespaceId`. It owns:

- one shared public scope containing exported members from every reopening in source order; and
- one distinct private block scope for each namespace declaration block.

A private block scope can see its own private declarations plus the namespace's public members and
ordinary lexical parents. It cannot see private declarations from another reopening. Members of an
ambient namespace are exported by default; an explicit export remains the rule in a non-ambient
namespace. Dotted and nested namespace syntax lowers to the same `NamespaceId`/scope topology as
the equivalent nested identifier declarations.

Qualified lookup resolves every namespace segment through shared public scopes only. It does not
enter a reopening's private scope, and a missing segment terminates that lookup: resolution never
falls back to a parent namespace, module, global, value, or type slot after the qualified path has
selected a namespace segment. The final leaf is then resolved in the requested declaration space.
This preserves the oracle distinction among missing namespace, missing exported member, type-only
root, and value-only leaf diagnostics.

### Type-only standalone namespaces and value augmentation

A standalone namespace publishes only its namespace/type-container surface. It does not create a
standalone value slot, function-like object, class-like static object, or any other runtime/value
receiver.

Exported namespace values may augment only an already existing function value or class
constructor/static draft admitted by the legal-merge classifier. They are lowered through the
existing function/class member machinery and join that draft before it freezes. Namespace type
members remain in the namespace public scope independently. A missing value owner is not repaired
by manufacturing a namespace value object.

### Standalone interface groups

A standalone interface `TypeGroupId` reserves one stable nominal declaration identity for the
whole group. This is declaration identity, not a change from structural interface assignability.
All ordered fragments lower without semantic queries into a checker-private, immutable source-order
recovery surface. The surface retains the complete generic frame, own members, index signatures,
call/construct signatures, overloads, heritage references, and every fragment needed for later
validation. Construction also creates typed pending heritage/conflict/relation obligations and
preallocates the exact lexical event owner for each outcome. Immediate syntax/header outcomes may
fill their existing owners, but no semantic validation is allowed to reshape the draft.

Recursive and mutually recursive interface groups reserve their identities first and form a finite
dependency graph. Every draft in one dependency SCC reaches its final immutable state before the
whole SCC is published atomically behind a final-state capability. Production lookup, projection,
evaluation, and relation reject a group that is not reachable through that capability; no member,
overload, heritage prefix, or event-completion order is observable early.

Only after publication may `SemanticQueryCoordinator` evaluate the typed heritage, conflict, and
relation obligations. An outcome fills its preallocated event owner and returns its typed semantic
result or exhaustion; it never adds, removes, replaces, or otherwise mutates the published recovery
surface. Tainted or exhausted checks follow the existing zero-durable-write rule. A rejected group
therefore remains a complete non-permissive typed surface, not a successful prefix, empty object,
or query-order-dependent reconstruction.

### Class+interface(+namespace) groups

For an admitted class+interface merge, the class remains the sole semantic identity owner. It keeps
its `ClassId` and the immutable `ClassInstance { class, args }` representation. Attached interface
fragments receive their lexical `DeclId`s and participate in the symbol's ordered `TypeGroupId`, but
they receive no independent object `TypeId`.

For this admitted merged-group case, the `args` vector is the complete ordered effective group
application/recovery frame defined below, rather than only the class declaration's syntactic type-
parameter list. Every class and interface binder maps to a position in that frame. `ClassId`
remains the sole declaration identity; the larger vector changes application identity, not class
declaration identity. This narrowly supersedes ADR-0006's class-declaration-parameter-order wording
for admitted merged groups and leaves ordinary class applications unchanged.

Interface instance fragments lower directly into the class instance draft before the existing
class declaration-SCC publication barrier. Exported namespace values lower into the class static
draft before that same draft freezes; namespace type members publish in the associated public
namespace scope. WU4 cuts this behavior over atomically: no production state may publish the class
first and attach interface or namespace members later.

Composition preserves the constructor and constructor accessibility, existing statics, retained
callable rows and overload/implementation visibility, parameter-property ownership, class type
parameters and applications, private/protected `declaring_class` metadata, and all other nominal
class state. Interface instance call/construct signatures remain instance-surface members; they do
not replace the class constructor value.

### Generic group headers and invalid-group recovery

A valid group has the same type-parameter name and arity at every supplied position. A constraint
or default supplied by only one declaration becomes the effective group header for that position.
When multiple declarations supply a constraint or default, the supplied forms must agree.
Conflicting names, arities, supplied constraints, or supplied defaults diagnose at every occurrence
required by the committed strict-tsc oracle.

Non-query header lowering produces a complete ordered effective group application/recovery frame
before member lowering. Each fragment binder maps to exactly one frame position. Matching positions
remain shared; a name or arity mismatch creates the distinct fragment-local positions demonstrated
by the WU0 corpus. Applications supply or recover one real `TypeId` for every position in that
frame, and substitution uses the binder-to-frame map for both class-owned and interface-owned
members. Constraints and defaults used for application diagnostics and default selection follow
the same first/source-order oracle recorded by the fixtures.

For an admitted merged-group header, this ADR explicitly supersedes ADR-0008's rule that every
source class default is deferred. The non-query surface lowerer may record `Ready(TypeId)` when an
effective default can be lowered with its construction capability. Otherwise it records typed
`Unsupported(EventTicket)` and uses ADR-0008's declaration/default-use event protocol: an
application that needs that default fills its one preallocated use owner, returns typed exhaustion,
and creates no application. An explicit complete vector need not consult the unsupported default.
Lowering a constraint or default may not invoke evaluation, projection, relation, inference, or any
other semantic query during construction.

Invalid-group recovery is semantic recovery, not successful validation, but it still publishes one
complete typed surface atomically. It must keep every class and interface member typed. It may not
insert `any`, the error type, `unknown`, synthesized `never`, omitted arguments, or a partial vector;
it may not discard a conflicting fragment; and it may not turn the group into an empty or
intersection/union approximation. If this complete oracle recovery cannot be represented, the
implementation stops for architecture review rather than substituting a permissive operand or
whole-group exhaustion.

For a class-owned admitted merged group, `ClassInstance` hashing/equality includes `ClassId` plus
every argument in the complete effective frame, including fragment-local recovery arguments. Its
projection substitutes through the same binder map. Two applications that differ only in a
fragment-local recovery argument are therefore different application identities and produce
separately keyed projections even when their class-header arguments are identical.

### Function+namespace value groups

An admitted function+namespace value group has one publication draft. Reservation retains every
ordinary or overload callable row in source order and every exported namespace value member that
augments the function. After all of those rows and members are complete, the checker interns one
immutable callable `ObjectType` and publishes that same type exactly once to the symbol and every
participating function declaration's value-storage slot. No bare `FunctionType` is separately
published as the group surface.

Function body completion reuses the reserved callable binders and records only body-owned effects.
It cannot replace or republish the callable object with a bare implementation `FunctionType`.
Ordinary, overloaded, ambient, and reverse-order groups all use this protocol; an illegal ordinary
reverse order still owns `TK2434` while retaining a non-permissive recovery surface.

### Ordered members, overloads, conflicts, and events

Member, heritage, call/construct, index, and overload fragments are processed in the exact source
order fixed by WU0. The first/source-order surface remains observable wherever TypeScript 6.0.3
retains it, including identical method, call, and construct overloads. Compatible later overloads
remain available in their oracle order. Property/method/modifier/index/heritage conflicts use the
oracle's diagnostic sites and retain its typed recovery surface; no generic "merge the shapes"
operation may replace these per-family rules.

Every declaration and member occurrence owns its checker-wide lexical event ticket. Immediate
conflicts and deferred validation outcomes fill those preallocated owners. Final replay uses the
existing `(original_module_ordinal, source_start, event_ordinal, record_ordinal)` order. There is no
diagnostic deduplication, last-fragment ownership, span deletion, or post-publication suppression.

### Global augmentation, UMD exports, and enum chimeras

WU1b reserves one compilation-global scope and typed augmentation records, but keeps both
disconnected from production root lookup and publication. No production consumer may observe a
global augmentation during that substrate stage. In WU5, backlog `43` atomically links and publishes
that one scope as the compilation global for every file in the serial project/type universe. Legal
augmentation blocks in external or ambient modules publish there in deterministic source/module
order; module-local declarations remain in their module scope and do not merge with the
compilation-global group merely because the names match. Cross-file and opposite-input-order
fixtures must prove the boundary. This reuses the ordinary binder, `TypeGroupId`, `NamespaceId`,
publication, and `Store`; there is no second ambient resolver or shared-prelude `Store`.

Backlog `43` owns `TK2669` when a global augmentation is outside an external or ambient module. The
context classifier and its exact lexical owner are direct gates, including legal external-module,
legal ambient-module, and illegal script contexts; WU1b records them dormantly and WU5 connects
their production behavior with the global scope in the same atomic cut.

Backlog `43` owns the context checks for `export as namespace` when the syntax is encountered:
`TK1314` outside an external module and `TK1315` outside a declaration file, where applicable.
Valid UMD publication is deferred to backlog `15`, because `export as namespace N` globally aliases
the external module export surface, whether that surface is formed by `export =` or ordinary named
exports. Backlog `43` must not claim support from local namespace/type lookup inside the declaration
file. The surface-inventory owner for the valid namespace-export form remains `15`. The current WU0
`export =` form is the immediate ownership witness; backlog `15` must add differential coverage for
both `export =` and named-export forms before claiming the UMD surface.

Backlog `42` owns enum/function `TK2567` sites and the exact three-way
enum+function+namespace legality/recovery matrix. Backlog `43` owns namespace placement `TK2434` and
must keep the function+namespace surface non-permissive even while the enum declaration remains an
explicit backlog-42 incomplete. No `43` implementation may guess the enum side or close the chimera
oracle.

### Cutover sequence

The production rollout is ordered as follows:

1. **WU1a — identity substrate.** Add unique lexical `DeclId`s, stable ordered `TypeGroupId`s,
   dedicated value-storage identity, exact AST-site maps, and declaration/event ownership as
   dormant metadata. Do not change `Symbol.ty` or any production lookup/publication consumer.
2. **WU1b — namespace binding substrate.** Add `NamespaceId`, one public plus per-reopening private
   scopes, export classification, legal-merge classification, plus a disconnected compilation-
   global scope and dormant augmentation/context records. Standalone namespace value lookup and
   production global lookup remain unchanged.
3. **WU2 — qualified reservation and lookup substrate.** Reserve namespace-contained groups,
   classify public namespace paths without fallback, and land exact missing-path diagnostics.
   Successful type-bearing leaves remain a substrate until WU3; no adapter selects one fragment
   for the legacy type slot.
4. **WU3 — atomic production type-slot/interface cutover.** Replace `Symbol.ty` and every type-space
   consumer with `TypeGroupId`, adapt single-fragment aliases/classes/interfaces to that contract,
   and replace last-declaration interface publication with immutable source-order recovery
   surfaces, dependency-SCC publication, pending obligations, and post-publication coordinator
   validation. The old and new type paths may not coexist in production. Until WU4, every admitted
   class+interface(+namespace) keep-pair returns an explicit typed unsupported/non-permissive
   outcome with its preallocated owner; it never reaches the old path or fabricates a type.
5. **WU4 — atomic keep-pair cutover.** Connect interface+namespace, function+namespace,
   class+namespace, and class+interface(+namespace) to the class/function drafts and publication
   barriers in one production cut, including the function callable-object group protocol. There is
   no attach-after-publication intermediate state.
6. **WU5 — ambient/global publication and ownership closure.** Atomically link/publish WU1b's
   compilation-global scope into production lookup, route ambient namespaces and `declare global`
   through it, implement `TK2669` plus the owned UMD context diagnostics, and finish the UMD/enum
   backlog and inventory reassignments.

WU1a/WU1b are representation and binding substrates and may land separately while no new semantic
consumer can observe them. WU3, WU4, and WU5 are atomic production cutovers. If any cannot pass as
one candidate, the whole uncommitted cutover returns to its preceding reviewed state; no winner
adapter, compatibility bridge, or partly linked global scope is retained.

### Direct invariant gates and hard stops

In addition to the WU0 conformance corpus, direct tests must prove:

- every source occurrence has a distinct `DeclId`, one exact AST site and event owner, while all
  legal same-name type fragments share one stable `TypeGroupId` in source order; lexical ids,
  value-storage ids, and type-group ids cannot alias, and WU1a leaves `Symbol.ty` plus all production
  consumers unchanged;
- a namespace has exactly one shared public scope and distinct private reopening scopes; ambient
  default export, private isolation, dotted/nested equivalence, and no-fallback qualified lookup;
- standalone namespaces have no value/static surface, while legal function/class augmentation
  freezes exported values exactly once with the existing draft;
- standalone recursive interface groups reserve one identity, freeze an immutable source-order
  recovery surface plus typed pending obligations, publish each dependency SCC atomically behind a
  final-state capability, and run heritage/conflict/relation obligations only afterward through
  `SemanticQueryCoordinator`; rejected or exhausted outcomes do not mutate that surface;
- a class+interface(+namespace) group has one `ClassId`/`ClassInstance` identity, no attached
  interface object id, and preserves constructor, statics, callable, private/protected, and poison
  metadata through both legal declaration orders;
- valid and invalid generic-header position maps, including effective sole-supplied constraints and
  defaults, conflicting supplied headers, name mismatch, arity mismatch, non-query `Ready` versus
  typed `Unsupported` default lowering, and real typed member recovery; two class applications that
  differ only in a fragment-local recovery argument have distinct `ClassInstance` and projection
  identities;
- function+namespace ordinary, overloaded, ambient, and reverse-order groups reserve all callable
  rows and namespace value members, intern one immutable callable `ObjectType`, publish it once to
  every declaration slot, and cannot be overwritten by later body completion with a bare
  `FunctionType`;
- exact first/source-order member and overload selection, conflict sites/cardinality, and lexical
  event replay under shuffled build and dependency order;
- WU1b's global scope/records remain disconnected from production lookup, while WU5 atomically
  links one compilation-global scope across files with isolated module locals and identical results
  under opposite input/dependency order;
- `TK1314`/`TK1315`, `TK2434`, `TK2669`, and the backlog-15/backlog-42 ownership boundaries without
  claiming valid UMD or enum semantics; the backlog-15 UMD plan differentially covers both
  `export =` and ordinary named-export external surfaces; and
- the WU3-to-WU4 class+interface interval returns explicit typed unsupported/non-permissive outcomes
  and never selects an old winner path; and
- zero construction-time semantic queries and zero partial/mutable publication or tainted durable
  query writes.

Implementation stops for architecture review if it requires last-declaration identity, one scope
shared by public and private namespace members, fragment intersection/union, mutation of a published
type or static surface, leakage between private and public namespace scopes, relation/evaluation/
projection during construction, a special global/UMD resolver or second `Store`, a fabricated or
partial recovery type, validation-before-interface-SCC-publication, a semantic outcome that mutates
a published recovery surface, publication of a bare function after its callable-object group,
production global lookup before WU5, coexistence of old and new publication paths, or broad
official-suite regex removal in place of structural syntax ownership. A false negative or source/
query/order-dependent surface is an immediate stop.

## Consequences

- Source identity, merge-group identity, namespace identity, class identity, and structural
  `TypeId` identity become explicit and cannot accidentally substitute for one another.
- Reopened namespaces and interfaces have deterministic public surfaces and diagnostics independent
  of binding, SCC, query, or file scheduling order.
- Class+interface merging reuses the existing class construction barrier, preserving nominal class
  behavior instead of inventing a competing merged object.
- Invalid generic groups require more bookkeeping because the checker retains fragment-local oracle
  positions and a complete rejected surface. The benefit is that diagnostics do not erase member
  types or create permissive recovery.
- Interface semantic validation becomes explicitly post-publication. The immutable recovery surface
  is stable while coordinator-owned obligation outcomes and lexical events record whether its uses
  are accepted, rejected, or exhausted.
- Standalone namespace values remain deliberately unsupported. Code that depends on namespace
  runtime objects stays outside typokat's type-only product boundary unless it augments a modeled
  function/class value.
- `declare global` can be validated before full `lib.d.ts` loading without introducing the frozen
  prelude/shared-store architecture owned by backlog `14`.
- Valid UMD publication and enum chimeras remain explicit active work with machine-valid owners;
  backlog `43` cannot silently absorb or close them.
- The staged rollout has three atomic production cutovers: type groups/interfaces, keep-pairs, and
  compilation-global publication. This makes those diffs harder to bisect, but it prevents a
  production state in which partial merged surfaces or a partly linked global scope are observable.

## Alternatives considered

### Let the symbol's type slot point to the last declaration

Rejected. It loses source identity and prior fragments, makes reverse order a different program,
and forces the checker to rediscover semantic ownership from an AST search.

### Give all namespace reopenings one scope

Rejected. It leaks non-exported members across blocks and makes qualified lookup capable of seeing
private declarations that TypeScript rejects.

### Merge fragment `TypeId`s with an intersection or union

Rejected. It cannot reproduce overload ordering, duplicate/conflict diagnostics, generic-header
recovery, index checks, or class nominal identity, and it encourages partial publication.

### Publish a class and attach interface or namespace members afterward

Rejected. A published class instance/static surface would change meaning after projection or
relation caches could observe it. Interface and namespace fragments must join the draft before the
existing SCC barrier.

### Give an attached interface its own object identity and combine it with the class

Rejected. It creates competing recursion and generic-application identities and can discard the
class constructor, static, retained-callable, or private/protected metadata. The class owns the
semantic identity.

### Recover invalid groups with `any`, error, `unknown`, synthesized `never`, exhaustion, or a
partial surface

Rejected. Each option either permits a false clean or erases the oracle's typed class/interface
members and distinct generic positions. Recovery remains complete, real-typed, and non-permissive.

### Model a standalone namespace as a value object

Rejected. That crosses the checker-only runtime boundary. Only an existing modeled function/class
value can receive exported namespace value members.

### Add separate ambient/global and UMD resolution paths

Rejected. They would duplicate binding and type-universe state, weaken cross-file determinism, and
make later `lib.d.ts` loading reconcile two semantic authorities.

### Remove the broad official-suite namespace regex when identifier namespaces land

Rejected. The bucket also contains external-module and regex-false-positive cases. It must be split
structurally, and every newly admitted file must have a supported surface or a precise active owner.
