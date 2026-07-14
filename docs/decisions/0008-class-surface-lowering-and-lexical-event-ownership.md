---
id: 0008
title: Cut class surface lowering over atomically with lexical event ownership
status: accepted
date: 2026-07-14
---

# 0008 — Cut class surface lowering over atomically with lexical event ownership

## Context

[ADR-0006](0006-immutable-class-instances-and-scc-publication.md) established immutable
`ClassInstance` identities, declaration-SCC publication, one-layer projection, typed exhaustion,
and transactional query writes. WU1a shipped the dormant representation and capability boundary,
but no production class consumer uses it yet. The remaining work is not safely separable into
independently runnable WU1b, WU1c, and WU1d states.

Class reservation, surface lowering, event ownership, SCC publication, projection planning, and
consumer dispatch are one protocol. Landing only part would require a legacy bridge, duplicate
diagnostic stores, partially populated application vectors, or a mode flag choosing between two
semantic paths. Each option makes declaration order, query order, or rollout state observable and
can publish a relation result for a surface whose meaning later changes.

The reviewed acceptance specification is fixture commit `889cc19`. Its flat and project corpora
pin public record ownership, diagnostic cardinality, opposite input/dependency order, generic
application failures, poison propagation, and projection exhaustion. Direct Rust tests must cover
the internal ordering, spans, budgets, capabilities, and zero-write properties that line markers
cannot observe.

This ADR refines ADR-0006's construction, event, and rollout protocol. ADR-0006's immutable type
representations, SCC publication model, one-layer projection, and typed query outcomes remain
binding. Where its staged WU1b/WU1c/WU1d landing language differs, this ADR supersedes that rollout
language without rewriting the accepted ADR.

## Decision

### One atomic WU1b-d production cutover

WU1b-d will land as one atomic production cutover. The cutover includes compilation-wide
reservation and scheduling, class surface lowering, lexical event preallocation, the checker-wide
`EventStore`, SCC publication, projection/query coordination, every class consumer, callable body
reuse, legacy-path deletion, and corpus registration. There is no dormant production path, dual
write, feature flag, compatibility shim, legacy fallback, or intermediate commit that compiles by
selecting old semantics.

Implementation may use internal review checkpoints, but none is a landing boundary. One production
worker owns the serialized Rust diff through all checkpoints; an independent reviewer evaluates the
whole candidate cutover; review fixes return to that same owner; and the leader commits only the
complete cutover. If the cutover cannot pass as a unit, rollback is the whole uncommitted production
diff to the last reviewed WU1a state. We do not keep a partly connected bridge.

### Compilation-wide reservation and scheduling

Before any surface is lowered, the checker reserves every source declaration that can participate
in a surface dependency: classes plus alias and interface templates, across all input modules.
Reservation allocates stable declaration identities, class/member/callable binders, type-parameter
frames, event tickets, and raw deferred type-syntax slots. Class, alias, and interface references
can therefore form edges across declaration and module boundaries before any target is published.

Only classes are SCC nodes. The identity walk traverses reserved alias and interface templates,
including class-to-alias-to-class, class-to-interface-property-to-class, and
interface-extends-class paths, to discover the class targets and class-to-class edges they contain.
The resulting finite class graph includes ordinary identity and distinguished heritage edges. Its
SCC condensation is processed dependency-first; non-heritage SCCs publish atomically; heritage
cycles poison their SCC; and a derived heritage edge observes only a completely published or
poisoned base.

Cross-module class edges and an input order opposite dependency order are required. Cyclic module
imports remain out of scope: this cutover does not use a class SCC to authorize or approximate a
cyclic module graph.

Every input retains two distinct ordinals:

- `original_module_ordinal` is assigned from the driver's stable original input order and is used
  in event ordering;
- a dependency scheduling slot is assigned by graph processing and is used only to decide build
  order.

Dependency discovery must never rewrite the original ordinal. An importer may appear before its
dependency in the stable input list while the dependency is still constructed first.

### Capability-isolated surface lowering

Construction is split into five narrow capabilities:

- `ClassSurfaceLowerer` coordinates one class draft and its preallocated members;
- `TypeSyntaxLowerer` resolves type syntax into immutable raw or deferred type nodes and creates a
  `ClassInstance` only for a syntactically valid, semantically available, complete ordered vector of
  real arguments, including a non-generic reference's complete empty vector;
- `SurfaceInitializerInferer` classifies only the reviewed initializer allowlist;
- `SurfaceTypeFactory` interns complete ordinary nodes and complete class applications; and
- `SurfaceEffects` fills only preallocated event tickets and pending obligations owned by surface
  syntax.

None receives `Pass`, an evaluator, a projection planner, a relater, overload/inference selection,
or an indirect callback that exposes those facilities. Surface construction cannot evaluate a
conditional, mapped type, `keyof`, alias instantiation, or deferred indexed access; project a class;
perform assignability; infer call type arguments; or select an overload. Raw deferred type syntax
remains deferred until a post-publication consumer demands it. A capability or test tripwire must
make an attempted construction-time query fail before any durable write.

`SurfaceTypeFactory` accepts a `ClassInstance` only when the source application is syntactically
valid, every argument is semantically available, and its ordered vector is complete and contains
real `TypeId`s. Every other case returns a typed arity, unavailable-argument, default, inference, or
poison/exhaustion outcome. It has no synthesized error, `unknown`, `never`, omitted-slot, or
partial-vector constructor. Source-authored `C<never>` is a legitimate complete real vector; the
prohibition is on inventing `never` as recovery.

### Representative initializer inference and whole-class poison

`SurfaceInitializerInferer` is an exhaustive explicit match over every current oxc `Expression`
variant. Only the reviewed pure allowlist returns `Inferred(TypeId)`. Every other explicit arm
returns `Unsupported`; no wildcard makes a new AST variant silently unsupported or inferred.
Compilation or a coverage test fails when oxc adds a variant until that variant receives an
explicit reviewed classification.

The public supported/unsupported fixtures are representative boundaries; the exact allowlist is a
direct Rust table and classifier gate:

- every unannotated `static` initializer is `Unsupported`, including a literal;
- numeric, string, boolean, and null literals are supported, with the existing mutable-versus-
  readonly widening rule;
- a parenthesized expression is supported only when its inner expression is recursively supported;
- the only supported unary form is unary negation applied directly to a numeric literal;
- the supported assertion forms are ordinary `expr as T` (`TSAsExpression`) and `<T>expr`
  (`TSTypeAssertion`) when the annotation lowers without exhaustion and the operand is recursively
  supported; non-null, `satisfies`, and instantiation expressions are not assertion allowlist forms;
- an array literal is supported only when dense, with no spread or elision, and every element is
  recursively supported (including the empty array);
- an object literal is supported only when every member is a static-key data property with an
  explicit value and that value is recursively supported; spread, computed keys, shorthand,
  methods, and accessors are unsupported;
- an instance `this.member` read is supported only when `member` is a lexically earlier field whose
  surface initializer was inferred successfully;
- an instance `this.member()` call is supported only when `member` is a lexically earlier method
  with one non-generic signature and the call has zero arguments; and
- an expression-bodied arrow is supported only when it is non-generic, every parameter is a simple
  explicitly typed identifier with no rest/default/destructuring, and its expression body is
  recursively supported.

Expanding this table requires a fixture and classifier review. The unsupported fixture is not an
exhaustive syntax list; the explicit oxc match and Rust table are the exhaustive gates.

Each unsupported unannotated class field owns exactly one preallocated origin record:

```text
id:      class/property-definition/initializer-inference
context: unannotated field initializer cannot be inferred during class surface construction
span:    the full initializer expression
ordinal: record ordinal 0 of the owning property-definition event
```

Construction visits every field even after the class is poisoned. `N` unsupported initializers
therefore produce exactly `N` origin records; two origins on one physical line remain two records in
the event multiset, ordered by their distinct lexical tickets and spans. The class's primary
internal poison cause is the first unsupported field in lexical order, while all `N` origin records
and all independently owned body records still emit. The class becomes initializer-poisoned before
same-pair identity, relation-cache lookup, `new` projection, call/construct or overload selection,
and inference can consume its surface.

Later read, write, static, structural source/target, same-pair, `new`, member call, overload,
assignment, argument, return, or inference demand returns typed initializer-poison exhaustion. It
does not replay the origin, add a second incomplete record, invent a `TK` diagnostic, or fabricate
an operand. An ordinary property, parameter, return, or alias reference to the poisoned class does
not itself create a record or poison the referring declaration.

The later body walk still visits syntax and preserves independently owned records such as `TK2304`
and the existing object-spread, array-spread, and computed-key child-slot incompletes. Poison is not
a body-skipping mechanism.

A heritage cycle owns one `class/class-heritage/cycle` record per participating heritage edge. An
acyclic derived heritage edge whose base is poisoned owns one
`class/class-heritage/poisoned-base` record at that derived `extends` site. Poison propagates
dependency-first through each derived heritage edge, including across modules, but never replays the
origin and never propagates through an ordinary non-heritage reference.

No fabricated recovery operand—including error, `unknown`, or synthesized `never`—enters a
`ClassInstance`, projection, relation, overload candidate, or inference candidate. The originating
initializer failure fills only its preallocated owner ticket once. A later demand that encounters
the existing class poison returns typed exhaustion and fills no record: zero replay and zero
additional event writes. Independent nested and body owners still fill normally. Poisoned and
exhausted paths perform no durable evaluator, projection, relation, overload, or inference
memo/cache write.

### Generic class descriptors and applications

Each source class type parameter stores a declaration descriptor:

```text
Absent | Ready(TypeId) | Unsupported(EventTicket)
```

For this cutover, **every source class default is `Unsupported`**, including a simple default such
as `B = string`. No source class default is lowered to `Ready`. `Ready` remains representable for a
future non-source or separately approved producer, but the cutover does not create one from source.

Each source default owns exactly one declaration record on its full default node:

```text
incomplete[annotation-lower/type-parameter-default/self]:
type-parameter default not lowered
```

Each source application that needs one or more unsupported defaults owns exactly one application
record, not one record per missing default. Its primary cause and declaration-event link are the
first `Unsupported` default in declaration order. It does not replay any declaration record:

- a type reference uses
  `annotation-lower/type-reference/class-default-argument`, context
  `class type-parameter default unavailable at application`, on the full reference;
- a `new` expression uses `expr-infer/new-expression/class-default-argument`, the same context, on
  the full `new` expression.

The source application fills its one preallocated use owner and returns typed exhaustion. It emits
no arity diagnostic, consults no fallback, and creates no `ClassInstance`. Later demands of that
existing exhaustion fill no record and replay neither declaration nor use. An explicit complete
vector does not consult the unsupported default and may create a normal instance.

Arity is checked syntactically before application construction:

- A bare, too-short, or too-long type reference to an exact-arity class emits one `TK2314` on the
  full reference, using renderer wording such as
  `Generic type 'ExactApplication<A, B>' requires 2 type argument(s)`.
- A type reference outside a defaulted min/max range emits one `TK2707` on the full reference,
  using wording such as
  `Generic type 'RangedApplication<A, B>' requires between 1 and 2 type arguments`.
- An explicit `new` with the wrong number of type arguments emits one `TK2558` on the exact
  `TSTypeParameterInstantiation.span`.

Wrong arity is the primary application cause. Every explicit nested type argument is nevertheless
visited and emits its independently owned record, such as `TK2304`; no unavailable child enters a
partial vector. Wrong arity does not consult defaults or inference. With valid arity, an unavailable
explicit child returns `UnavailableExplicitClassTypeArgument`, publishes no application, and later
demands cannot observe a fake partial vector.

A `new C()` whose arguments, constructor candidates, and defaults cannot produce a complete vector
owns one record on the full expression:

```text
incomplete[expr-infer/new-expression/class-type-argument-inference]:
class type arguments cannot be fully inferred
```

It does not construct `C<unknown>` or synthesize error/`never` recovery arguments. An unsupported
default takes precedence and uses the single source-application default-use record instead of the
inference record. An explicit complete vector in an open class frame is valid; a bare generic
self-reference receives `TK2314`; and a non-generic class reference creates a complete empty-vector
instance. Only syntactically valid, semantically available, complete ordered real vectors enter
interning; a source-authored `never` argument is real and remains valid.

### Lexical binder and event preallocation

Reservation follows lexical AST order and preallocates every class, member, callable, and event
identity before surface construction. A callable binder retains its full type-parameter binders,
constraints/default descriptors, receiver, ordered parameters, declared return, overload position,
implementation-hiding state, and constructor parameter-property ownership. Surface publication and
later body checking reuse that same retained callable content; the body pass does not lower the
public signature again.

All binders and metadata owned by a class—including callable binders and class type-parameter side
columns—freeze atomically with its declaration SCC. No class-owned binder becomes visible early,
and a later write to a frozen binder is rejected.

### Checker-wide event ownership

The cutover installs one checker-wide `EventStore`. Reservation issues opaque `EventTicket`s in
lexical order. Each ticket has exactly one semantic owner and may be filled once with zero or more
records. Final replay sorts records by the four-key order:

```text
(original_module_ordinal, source_start, event_ordinal, record_ordinal)
```

`event_ordinal` distinguishes events with the same start; `record_ordinal` preserves the internal
order of multiple records owned by one event. Completion order, SCC order, dependency slot, query
order, cache hits, and physical line sharing cannot change replay order.

There is no event deduplication, span-matched deletion, truncation, or post-hoc suppression.
Every speculative overload candidate writes only to its own local `CandidateEffects`, never
directly to the `EventStore`. Selection commits exactly one local set once: either the selected
winner's effects or the selected final-failure owner's effects. All non-owning candidates discard
their local effects. Exhaustion is not committed as a speculative candidate effect; it propagates
its preallocated demand/budget ticket to the coordinator, which fills that owner exactly once and
discards every speculative local set. Preallocated origin/application/budget tickets remain the
only public recovery records.

### Publication, projection, and consumer coordination

The class graph and heritage rules remain those of ADR-0006 as refined above. SCC publication
exposes either an immutable complete surface or a typed poison cause. It never exposes a partially
composed base, constructor, static surface, type-parameter descriptor, or callable binder.

After publication, one `SemanticQueryCoordinator` is the consumer boundary. Member/index/static
lookup, `keyof`, inheritance composition, contextual typing, destructuring, `new`, call/construct,
overload selection, inference, assignment/argument/return obligations, and relation enter class
projection only through this coordinator. No consumer calls a legacy class-object path directly.

The coordinator creates a query-local one-layer `ProjectionPlanner`. Each query admits at most 128
distinct complete `ClassInstance` ids; revisiting the same id is a regular cycle and consumes no
second slot. Attempting to admit the 129th returns typed
`Exhausted(ClassProjectionBudget)` and fills the owning
`incomplete[relation/class-projection-budget]` event. A memo hit still counts as an admitted id for
that query. The originating budget failure fills only that preallocated owner once; later consumers
of the existing exhausted outcome fill no record and do not replay it.

Projection/evaluation returns `Ready | Exhausted`; relation returns `Yes | No(ReasonChain) |
Exhausted`; candidate selection and inference preserve their corresponding typed exhausted forms.
If a demanded child exhausts, the parent remains exhausted even if a later sibling or overload
could otherwise decide the query. A definite mismatch or earlier winner reached before the frontier
remains decisive. In particular, an exhausted candidate before a later winner aborts selection;
an earlier winner avoids the exhausted candidate.

Planner evaluation/projection overlays, `CandidateEffects`, and pending evaluator/projection/
relation writes are query-local. A tainted query commits no durable memo/cache write. Successfully
completed, untainted work commits only at the coordinator. The final assignment, argument, or
return diagnostic boundary may conservatively convert exhaustion to its ordinary mismatch
diagnostic while the originating demand/budget owner fills its one incomplete record; an already
existing exhausted outcome contributes no later record. Internal semantic consumers may not fold
`Exhausted` into `No`.

### Deletion, hard stops, rollback, and direct gates

The atomic cutover deletes or disconnects all of the following before it can land:

- mutable reserved class-object publication and class-as-alias `Instantiation` fallback;
- duplicate class signature lowering and span-based diagnostic deletion/truncation;
- legacy class projection/member/new/call/inference entrypoints outside the coordinator;
- partial/default-filled/error/`unknown`/synthesized-`never` class application construction;
- class-local or pass-local diagnostic streams that bypass the checker-wide `EventStore`; and
- any feature flag, mode switch, dual write, dormant bridge, or compatibility fallback for WU1b-d.

Implementation stops for architecture review if construction needs `Pass`, evaluator, planner, or
relater access; if a consumer cannot preserve typed exhaustion; if a fake recovery operand is needed
to compile a path; if any class-owned binder must publish before its SCC; if an invalid application
can enter interning; if event ownership needs deduplication/deletion/truncation; or if old and new
semantics must coexist. The approved rollback is the whole uncommitted WU1b-d production cutover,
not a legacy runtime path.

The fixture commit `889cc19` must pass in both flat and project harnesses. Mandatory direct Rust
gates additionally prove:

- the exhaustive oxc expression classifier, exact allowlist table above—including all unannotated
  static initializers unsupported—and new-variant compile/coverage tripwire;
- exact initializer/default/application event ids, contexts, owner spans, lexical tickets,
  cardinality, same-line multiset order, and no replay;
- compilation-wide class/alias/interface-template reservation with class-only SCC nodes, alias/
  interface identity traversal to class targets, original module ordinal distinct from dependency
  slot, cross-module edges/opposite input-versus-dependency order, explicit cyclic-module-import
  rejection, dependency-first construction, and atomic binder freeze;
- capability isolation and zero construction-time evaluator/projector/relater calls;
- poison before identity/cache/`new`/call/overload/inference, all initializer origins and body child
  records visited, heritage-only propagation, and no fabricated recovery operand;
- syntactically valid, semantically available, complete-real-vector-only interning; invalid-arity
  primary precedence with every nested child record; unsupported-default first-declaration linkage
  and one use record per source application; source-authored `never`; self/open-frame rules; and no
  error/`unknown`/synthesized-`never`/partial instance;
- callable full-content retention, exactly-once signature lowering, body reuse, and source-order
  overload/parameter-property behavior;
- exact four-key event replay under shuffled completion, one owner, and absence of deduplication,
  span deletion, or truncation;
- one-layer projection, concrete application identity, exact 128/129 accounting, typed
  `Yes | No | Exhausted`, candidate exhaustion before a later winner, and mismatch/winner-before-
  frontier controls;
- origin/application/budget failure filling exactly its preallocated owner once; later demand of an
  existing poison/exhaustion filling zero; independent nested/body owners preserved; candidate-local
  effects committing exactly the winner or selected final-failure owner; and zero durable memo/cache
  writes for every poisoned, exhausted, provisional, or otherwise tainted recovery path; and
- every consumer routed through the coordinator, every legacy path deleted, and the old/new-path
  tripwire count exactly zero.

## Consequences

- There is one semantic state to review: before the cutover classes use the old path; after it every
  class surface and consumer uses the published immutable path.
- The production diff is larger and cannot be bisected into independently runnable WU1b/c/d
  commits. Internal review checkpoints and serialized ownership replace partial landings.
- Unsupported initializers, all source class defaults, unresolved constructor inference, and
  projection-budget exhaustion deliberately over-report or return typed incomplete outcomes instead
  of guessing. This preserves soundness at the cost of accepting fewer programs than tsc 6.0.3.
- Exact lexical ownership removes duplicate lowering and makes event order independent of dependency,
  SCC, demand, and cache order.
- All future class syntax and consumers must extend the exhaustive classifier, reservation graph,
  coordinator, typed-exhaustion matches, and direct gates together.

## Alternatives considered

### Land WU1b, WU1c, and WU1d independently

Rejected. Each intermediate needs dormant or dual semantics and can publish events or types through
the wrong path. Review checkpoints provide the desired decomposition without making partial states
production architecture.

### Keep the legacy path behind a feature flag or fallback

Rejected. A flag does not make mutable and immutable class identities compatible, and a fallback
turns unsupported or exhausted semantics into query-order-dependent success.

### Treat simple class defaults as ready

Rejected for this cutover. Splitting source defaults into easy and hard cases introduces a second
lowering policy before default ownership and application linkage are proven. All source defaults use
one explicit unsupported protocol until a later spec expands the allowlist.

### Recover with `unknown`, the error type, synthesized `never`, or a partial application

Rejected. Any fabricated operand can enter projection, relation, overload selection, inference, or
durable caches and create a false clean. Typed exhaustion carries the missing semantic fact to the
only boundary authorized to over-report.

### Append diagnostics and remove duplicates afterward

Rejected. Deduplication, span deletion, and truncation make the winning record depend on execution
order. Lexical tickets assign one owner before any semantic work begins.
