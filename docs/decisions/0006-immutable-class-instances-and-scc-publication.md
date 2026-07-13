---
id: 0006
title: Represent class applications as immutable nodes behind an SCC publication barrier
status: accepted
date: 2026-07-13
---

# 0006 — Represent class applications as immutable nodes behind an SCC publication barrier

## Context

The class path currently reserves an object `TypeId`, fills that row in place, and temporarily
represents a reference to a generic class that is still filling as the general lazy
`Instantiation` node. That terminates direct recursion, but it gives one node two unrelated jobs:
alias evaluation and class-instance projection. A class application nested under another object,
array, tuple, callback, union, intersection, conditional, mapped type, or indexed access can then be
observed before the class surface it needs is complete, or can reach the evaluator/relation path as
though it were a lazy alias. Mutual references make the result declaration-order-sensitive.

The semantic-duplication sprint's WU1 was going to reserve class method and constructor signatures
once so body checking could reuse them. Extending that idea to mutable reserved function rows would
make a `TypeId` change meaning after interning. That conflicts with the binding type-store and
relation-cache invariants: ordinary structural nodes are hash-consed, their identity is complete at
intern time, and a cached relation verdict must remain valid for the lifetime of its operand ids.
Span-based deletion of diagnostics emitted by duplicate lowering has the same temporal problem on
the reporting side: it relies on which pass happened to emit first rather than assigning each
source occurrence one owner.

The replacement must handle regular recursion (`Box<T> -> Box<T>`), mutual recursion in either
declaration order, and non-regular recursion (`Box<T> -> Box<T[]>`). The last case rules out eagerly
building a transitive graph of concrete applications: `Box<string>`, `Box<string[]>`,
`Box<string[][]>`, and so on is infinite. Generic class instances remain structural for relation,
while private/protected property metadata continues to carry nominal declaration identity.

## Decision

### Immutable class-application representation

We will represent every class type application, including a non-generic class with an empty
argument list, as an immutable, hash-consed `ClassInstance` node:

```text
ClassInstance { class: ClassId, args: [TypeId; declaration parameter order] }
```

Its complete interner key is the class declaration identity plus the ordered applied arguments.
Arguments are the effective values after the existing arity/default/recovery policy; a missing or
invalid source argument is diagnosed before its recovery `TypeId` enters this complete key.
The representation is formally distinct: a new `TypeTag::ClassInstance`, a
`ClassInstanceType` payload and its own Store side table/dedup discriminant. Hashing, equality and
child-walking helpers may share plumbing with `InstantiationType`, but the payload/tag/semantic
dispatch may not. The alias evaluator must never interpret a class instance as
`substitute(alias, args)`, and relation must not apply the conservative identical-only rule used by
an unevaluated alias instantiation.

`ClassInstance` is an application identity, not its member object. Its object projection is derived
by the checker as described below. Two applications with structurally compatible projections still
relate structurally even when their application ids differ. Private/protected members keep their
existing `declaring_class` metadata in the projection, so nominal compatibility is unchanged.

All ordinary nodes produced for a class surface or projection—objects, functions, arrays, tuples,
unions, intersections, conditionals, mapped types, and their metadata—remain
immutable and use their normal structural hash-consing. Function rows in particular are interned
only after binders, constraints/defaults, receiver, parameters, and return type are complete.
Neither class recursion nor signature reuse authorizes a reserve-and-mutate function row. Existing
reserve/fill mechanisms outside the class-application path are not generalized by this decision.

Indexed access needs one new immutable deferred form because the current store has no general
indexed-access tag. WU1 will add `TypeTag::DeferredIndexedAccess` with payload
`{ object: TypeId, index: TypeId }`. The ordered pair is its complete structural hash/equality key;
substitution rewrites both children; rendering prints `Object[Index]`; every structural walker visits
both; and the evaluator returns a typed demand outcome after resolving at most one outer layer when
both operands are ready. An unevaluated node relates only to the identical node (or to its
query-local evaluated result after demand), and an exhausted evaluation never degrades into an
ordinary mismatch or cacheable relation verdict. This is distinct from the existing
`Instantiation`, `Mapped`, and `Keyof` tags rather than an overloading of any of them.

### Checker-side declaration construction graph

Class construction will be mutable checker state, not mutable type-store payload. The finite graph
is keyed by class declaration identity. A node owns the class's open type-parameter frame, draft
instance surface, constructor/overload surface, static surface, heritage plan and class metadata.
All class identities are allocated before graph construction, so forward references can create
immutable application nodes without reading a partially built target surface.

Surface construction runs in an explicit **no-evaluation/no-relation mode**. It may resolve names,
allocate persistent binders, create class applications, and intern complete ordinary composite
nodes. It must not:

- project a class application to members;
- evaluate a conditional, mapped type, deferred indexed access, `keyof`, or lazy alias merely to
  finish a class surface; or
- ask the relation engine about constraints, defaults, overload compatibility, overrides, or
  assignability.

Checks that require evaluation or relation become typed pending obligations attached to the source
event that requested them. Immediate name/access/arity failures become events too. The same lowered
parameter and method surfaces are retained for constructor parameter properties, overloads, the
class value side, and later body checking; those consumers do not lower the annotation again.

Graph edges are extracted by a non-evaluating identity walk over every draft root: instance/static
properties and both index signatures; object call/construct signatures; function binder
constraints/defaults, receiver, parameters and return; array element; tuple elements/rest; readonly
operand; union/intersection members; all conditional operands; alias-instantiation base/arguments;
mapped key/value; template holes; `keyof` operand; both deferred-indexed-access operands; and class-
instance arguments. Encountering a `ClassInstance` also adds an edge to its target declaration.
Constructor overloads, parameter-property types and static surfaces are roots of the same walk.
Leaf intrinsics/literals/type parameters/infer and mapped placeholders add no edge.

An `extends` clause adds a distinguished **heritage edge** even when the base contributes no
ordinary identity child. The graph is decomposed into SCCs, then its condensation DAG is processed
dependency-first (for an edge `Derived -> Base`, `Base` publishes first). Every draft in an SCC must
be complete before any open instance surface, constructor/static metadata, frozen parameter
metadata or pending obligation from that SCC is visible. Non-heritage mutual recursion publishes the
whole SCC atomically. A heritage edge inside one SCC is a cyclic-base error: that edge contributes
no partial members, the SCC is marked projection-poisoned, and its source event records
`incomplete[class/class-heritage/cycle]`. Any later projection demand conservatively rejects with
that incomplete reason and cannot populate a relation cache.

Poison then propagates dependency-first **only across heritage edges** in the condensation DAG. If
an acyclic `Derived -> Base` heritage edge targets a poisoned SCC, the derived SCC becomes poisoned,
publishes no partial instance/constructor/static surface, and the derived `extends` source event owns
`incomplete[class/class-heritage/poisoned-base]`. The next derived SCC observes that poison in turn,
so one-level and multi-level chains cannot launder partial base members into a published class. Each
edge owns its own incomplete record; it does not replay the origin cycle event. An ordinary
non-heritage identity edge to a poisoned class does **not** poison the referring SCC: that declaration
may publish an immutable reference, while a later demand to project the poisoned application returns
the typed exhausted/poisoned outcome. For an acyclic heritage edge to a published base, composition
runs only after complete base publication and before the derived SCC publishes.

Class type-parameter constraints/defaults are draft data until SCC completion. Circular constraints
are diagnosed/cleared first; the final descriptors and the Store constraint side-column entries for
those `TypeParamId`s are then frozen as part of publication. An interner setter must reject a later
write to a frozen id. Projection, method defaults and relation therefore never read class-parameter
metadata that can still change.

The SCC is over declarations, not concrete applications. That makes the graph finite even for
non-regular recursion: the open surface of `Box<T>` contains the immutable application
`Box<T[]>`, but graph construction does not recursively construct `Box<T[]>`, `Box<T[][]>`, and its
infinite successors.

Publication is enforced by capability, not convention. Graph nodes have
`Pending -> Building -> Built -> Published` (or `Poisoned`) states. Surface lowering receives a
`ClassSurfaceBuilder` capability that exposes name resolution and complete-node interning but has no
evaluator, projector or relater entrypoint. Completing condensation processing creates a
`PublishedClasses` capability over the immutable final-state registry; each entry is `Published` or
`Poisoned` with its owned cause. Class projection and any evaluator/relation path that may encounter
a `ClassInstance` require that capability and reject `Pending/Building/Built`; `Poisoned` returns
typed `Exhausted(ClassHeritagePoison)` and never exposes a partial surface. No state falls back to an
empty object or the error type. Test-only tripwires count attempts to evaluate, project or relate
during surface construction and assert zero; direct negative tests prove every public entrypoint
rejects a forged/pre-publication request without a cache write.

### One-layer demand normalization

After a declaration SCC is published, demanding the outer shape of a `ClassInstance` substitutes
its applied arguments into that declaration's open surface exactly once. The result is an ordinary,
immutable, structurally interned projection. A pass-local `ClassProjectionMemo` maps the complete
`ClassInstance` `TypeId` to that projection `TypeId`; entries are installed only after publication,
never change, and may persist for the rest of the checker pass. Nested class applications remain
`ClassInstance` nodes with substituted arguments; one demand does not project them recursively.

For example, projecting `Box<string>` where the open `next` member is `Box<T[]>` produces an object
whose `next` type is `Box<string[]>`. Only a later member/relation demand on that node projects the
next layer. Regular and mutual back-edges therefore terminate through repeated application ids, and
non-regular recursion advances only as far as the program actually observes.

Evaluation remains demand-driven at its existing boundary. If selecting a projected member later
requires a deferred conditional, mapped type, deferred indexed access, or lazy alias to expose one
outer shape, the owning evaluator performs that operation then; class surface publication itself
does not pre-evaluate it. Each normalization step returns after exposing one outer layer.

One-layer projection does not by itself bound a whole-type relation: a class with only
`next: Box<T[]>` produces infinitely many distinct applications. Every public projection demand
therefore carries an explicit `ClassProjectionBudget`; relation planning starts with
`MAX_CLASS_PROJECTION_EXPANSIONS = 128`. The unit is one distinct `ClassInstance TypeId` admitted to
the query plan, including a projection-memo hit, so prior query order cannot buy more logical depth.
Revisiting the same application id is a regular cycle and consumes no second unit. The key is always
the complete application id (class plus arguments), never the declaration `ClassId` alone.

If a demand reaches the 129th distinct application, it returns
`Exhausted(ClassProjectionBudget)` and records
`incomplete[relation/class-projection-budget]`. Exhaustion is a semantic outcome, not a synonym for
false. Projection/evaluation APIs use `DemandOutcome<T> = Ready(T) | Exhausted(Exhaustion)`, and
relation uses `RelationOutcome = Yes | No(ReasonChain) | Exhausted(Exhaustion)`. Every caller must
match all variants; no `is_yes()` helper may silently fold `Exhausted` into `No`.

Taint propagates through every parent evaluator/projection/relation frame. Once a demanded child
returns `Exhausted`, the enclosing semantic outcome remains `Exhausted` even if a different sibling
later supplies a definite mismatch; a tainted query commits zero evaluator, projection-result or
relation-cache writes. If relation proves `No` before demanding an exhausted frontier, its outcome is
an ordinary `No`; merely carrying an unreachable frontier in the query plan does not change the
verdict. This is the same soundness class as evaluator exhaustion and provisional recursive relation
assumptions.

The final assignment/argument obligation boundary may deliberately convert `Exhausted` into a
conservative `No` plus the incomplete record. Other semantic callers preserve the
distinction. A conditional-extends evaluation returns its original deferred conditional node and
does not select either branch. Inference records an exhausted inference attempt and contributes no
candidate/substitution from it. Overload selection treats an exhausted candidate as
`CandidateSelection::Exhausted`: if reached before a winner it aborts selection rather than skipping
to a later overload that might violate source-order semantics. Member/index/`keyof`/call/construct
shape consumers likewise return typed exhaustion to their diagnostic/recovery boundary instead of
fabricating an empty, error or mismatching shape.

### Explicit projection/relation protocol and cache discipline

`Relater` remains read-only over `Store`; it never receives or hides `&mut Interner`. Before each
relation query that can reach a class application, an explicit `ProjectionPlanner` phase owns
`&mut Interner`, `&PublishedClasses`, read access to the pass-local projection memo and a fresh
128-unit budget. It walks the query roots through the same identity children relation may inspect,
materializes missing one-layer projections as immutable Store nodes, and performs any demanded
outer evaluation of deferred indexed access, conditional, mapped, `keyof`, or alias-instantiation
nodes. Every successful evaluation result is also an immutable Store node. None enters a durable
evaluator/projection memo yet; the planner consumes itself to return a query-local planned
transaction:

```text
PlannedRelation {
  plan: ProjectionPlan {
    class_projection_overlay: ClassInstance TypeId -> projection TypeId,
    evaluation_overlay: deferred/evaluable TypeId -> evaluated TypeId,
    frontier: TypeId -> Exhaustion,
  },
  pending_evaluator_writes,
  pending_projection_memo_writes,
  planning_tainted,
}
```

Consuming the planner ends its mutable interner borrow before `Relater` is constructed. In keeping
with the current API, `Relater` receives immutable `&Store` and `WellKnown`, owns its
`RelationCache` and cycle stack, and additionally borrows `&PublishedClasses` plus this query's
`&ProjectionPlan`. Before identity, cache or cycle handling at every relation frame, it follows the
query-local evaluation and class-projection overlays to concrete operand ids. The planner forbids
self mappings and records a finite visited chain; absence from an overlay means the node remains
deferred under its existing identical-only rule. Reaching `frontier` returns
`RelationOutcome::Exhausted` with that typed cause. Relation therefore observes evaluated
conditional/mapped/`keyof`/alias/deferred-indexed results without either a mutable interner or a
premature durable evaluator-memo write.

Each public `is_assignable`/subtype operation creates a `RelationTransaction` inside that `Relater`;
relation inserts go to the transaction, while reads use only the relater-owned durable cache. The
relation outcome carries the query taint back to the checker coordinator, which either promotes all
three pending write sets—evaluator memo, projection memo, and relater-owned relation cache—or
discards all three. A planner-discovered exhausted frontier sets `planning_tainted` and therefore
forbids promotion even when relation proves an ordinary `No` without reaching that frontier. No
external cache is introduced.

The pass-local projection memo persists deterministic application-to-projection ids across queries,
but a later query must still admit each id into its own budgeted plan. Both normalization overlays,
the frontier and exhaustion taint are query-local and never persist. Newly interned but uncommitted
projection or evaluated nodes are harmless arena growth; no consumer can discover them without the
plan or a committed memo entry. Non-relation member/index/`keyof` demands use the same planner
transaction, overlay and budget, omitting only the relation-cache write set.

After outer normalization, the existing relation cache/cycle key remains the three Store ids/words
`(normalized source TypeId, normalized target TypeId, RelationKind)`. The active cycle stack is keyed
by the concrete normalized operand pair and relation kind; neither the planner nor relation uses a
declaration-only cycle key. A nested regular/mutual application therefore normalizes back to the
same concrete pair, while `Box<T[]>` continues to distinct pairs until a mismatch or the budget.

Cache writes are transactional per relation query. Completed relation/evaluator entries and new
projection-memo entries accumulate in local write buffers and are promoted only when planning and
relation finish without projection exhaustion, evaluator exhaustion, binder alignment context, or
an in-flight ancestor dependency. `RelationOutcome::Exhausted` and `planning_tainted` both discard
all buffers, guaranteeing zero durable cache writes from an exhausted plan/query. Previously
committed complete verdicts and projections remain valid reads because their Store operands are
immutable and context-free. The ADR-0005 aligned-binder bypass and the existing provisional-cycle
rule remain unchanged.

### Ordered diagnostic events

Class construction and body checking will report through an ordered event stream. Every source
event receives the total-order key
`(module_ordinal, source_start, event_ordinal, record_ordinal)`: module order is the driver's stable
input order; `source_start` is the owning AST occurrence; `event_ordinal` is allocated by lexical AST
traversal to distinguish semantic events sharing a start; and `record_ordinal` preserves multiple
records owned by one event. Immediate surface errors, deferred relation/evaluation outcomes and later
body errors fill their preallocated keys; final replay is one lexicographic sort, independent of SCC
traversal, projection demand or cache hits.

An occurrence is lowered and emitted once. There is no diagnostic deletion by matching spans and no
re-lowering solely to reproduce a message. This preserves constructor parameter-property ownership,
overload source order and implementation hiding, static class-parameter diagnostic cardinality, and
the ordering between signature and body records.

WU1 replaces the current nine-record raw-order characterization with this exact six-record vector
for `class_diagnostics_preserve_current_raw_vector_order`: earlier assignment `TK2322`; generic
default `TK2344`; unresolved parameter `TK2304`; unresolved return `TK2304`; method-body assignment
`TK2322`; later assignment `TK2322`, each at its existing source span. The duplicated default/
parameter/return records disappear rather than being filtered, and the test is renamed to describe
source-event order. The five distinct static `TK2302` occurrences remain in lexical source order.

## Consequences

- Recursive and mutually recursive class applications have immutable identities and cannot expose
  partial member rows. Declaration order and prior demand/relation order cease to affect their
  visible surface.
- Non-regular recursive applications terminate through one-layer demand plus a 128-expansion query
  budget. Accepted downside: an otherwise compatible shape deeper than the budget over-reports and
  records incomplete rather than guessing true. The core preserves `Exhausted`; only an explicit
  diagnostic boundary may turn it into a conservative failed check.
- Heritage poison cannot leak partial members through derived chains, while an ordinary property or
  signature reference to a poisoned class does not unnecessarily poison its declaring SCC.
- WU1 is an **XL architectural rollout**, not a local signature container extraction. It is staged
  representation/capabilities -> graph/publication/events -> projection/relation -> consumer reuse
  and enablement, with an independent gate between stages.
- Substitution, rendering, evaluator structural walkers, member/index lookup, `new`, inheritance,
  constructor overloads, contextual/destructuring paths and relation must recognize
  `ClassInstance`/`DeferredIndexedAccess` while preserving their own cycle, binder, budget and cache
  policies.
- Ordinary `TypeId` structural hash-consing remains the default. The special application node is
  immutable and hash-consed by its complete key; its projected object/function shapes return to the
  ordinary interner and can deduplicate normally.
- Projection planning adds immutable arena growth even for an exhausted query, but its class and
  evaluation overlays remain query-local and publish no memo/cache entry. This is bounded per query
  and can be measured separately from relation work.
- Diagnostics that currently depend on eager duplicate lowering must move to explicit event
  ownership. This is more bookkeeping, but it removes span suppression and makes exact
  source-order/cardinality testable.

## Alternatives considered

### Reserve and mutate function rows

Rejected. A function `TypeId` would be interned before its full structural key was known and would
change meaning after relation/evaluator code could observe it. Updating dedup buckets or invalidating
every dependent cache cannot restore the invariant cheaply; literal mutable function rows are not
an acceptable shortcut.

### Keep mutable reserved object rows for class instances

Rejected for the application path. It exposes partial state across recursive demands and makes a
relation verdict depend on fill timing. Mutable checker drafts are allowed; published interned class
applications and ordinary projections are not mutable.

### Reuse the existing lazy `Instantiation` semantics unchanged

Rejected. Alias instantiation is evaluator-owned and may remain deferred/identical-only. A class
application needs member projection and structural relation after its declaration surface is
published. Sharing physical storage is harmless; sharing semantic policy is not.

### Let `Relater` intern projections on demand

Rejected. It would hide `&mut Interner` behind the relation engine, break the current immutable
Store phase boundary, and make cache/cycle code observe arena mutation mid-query. The explicit
planner materializes immutable operands first and hands relation a bounded read-only plan.

### Eagerly expand every reachable concrete class application

Rejected. It does unnecessary work for regular recursion and does not terminate for
`Box<T[]>`. One-layer demand plus an explicit expansion budget is the finite boundary.

### Publish declarations independently and retry missing members later

Rejected. Retrying turns declaration order and query order into semantics, permits provisional
relation results to enter caches, and makes diagnostic cardinality scheduling-dependent. SCC
publication provides one explicit completeness boundary.
