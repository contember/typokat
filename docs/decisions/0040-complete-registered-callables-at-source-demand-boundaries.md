---
id: 0040
title: Complete registered callables at source-demand boundaries
status: accepted
date: 2026-09-08
---

# 0040 — Complete registered callables at source-demand boundaries

## Context

The full and partial inference fixers repeatedly substitute constraints through their own derived
fallbacks. This introduces depth exhaustion and loses recovered argument/result diagnostics.
Freezing every fallback is also wrong: a still-invalid reverse-dependent candidate needs its fallback
updated after its dependency completes. Original candidates, active seeds and completed recoveries
must remain distinct.

Canonical TypeIds do not determine the necessary dependency order. Opposite union orders can share
one ID; interface substitution can erase unused arguments; transparent and retained aliases differ.
Even an occurrence-owned lowering record is insufficient: identical signatures have different
construction histories after a lexical header, forward call or early callable relation. Constructed
returns can create array constituents before constraints; a plain copy need not construct either.

The user explicitly included this construction-order boundary in the as-const sprint rather than
restoring old missed errors. This decision supersedes the narrower proposed ADR-0034. It does not
accept its old tracer as a complete implementation or import all of the deferred binder programme.

## Decision

### Registration and immutable identity

Represent an authenticated, unconstructed callable explicitly in the TypeId model. A registered
reference contains a registration-domain identity, a callable registration identity and a complete
ordered outer-argument vector. The selected local source profile has an empty outer vector.
Registration is not a ready FunctionType and must not be encoded as an unrelated alias.

The registration owns complete AST-free recipes and authenticated declaration, source, scope,
callable and binder keys. Reuse the existing declaration inventory and reservation traversal.
Reserve named identities once; physical ID allocation is not semantic construction order.

Registration safety checks the complete recipe/application closure, including unused fields,
resolved endpoints, binding alignment, arity, published dependencies and poison. It allocates no
construction ranks. Safety is not a claim that deferred semantic header validation has passed.
Copies, identity and supported unknown-target transfer require this preflight, but not shape.
Rendering and inspection cannot construct a signature. Pre-ready successes never populate ordinary
durable completed-callable relation caches, including through an enclosing relation.

Include registered-reference application edges in hash/equality, substitution, projection and the
reference census. Domain/freeze/fork validation must not reuse an unvalidated borrowed capability.
An application never contains a partial or fabricated argument vector.

### Closed producer and consumer selection

Before registration choose exactly `Selected(CompleteProducerUsePlan)`, `OutsideProof` or
`InvalidAuthenticatedInput`. Selection is whole-owner and query-free. A selected failure never
triggers eager fallback. An unselected owner retains its existing route, not a new unsupported notice.

Resolve the source reference inventory once against current borrowed binder facts, joined to exact
manifest identities and consumed root/scope receipts. C0/C1a are evidence, not a current lookup basis.
A narrow read-only receipt accessor is permitted; a second Binder, semantic source walk or general
lookup framework is not required by this contract.

Close every selected value over all future uses and finite immutable const-copy destinations.
Every use must have a supported operation recipe. A future write, escape, unresolved/unclassified
use, unclosed copy, overloaded/union callee or unsupported relation target rejects the whole owner
before registration. There is no generic-transport catch-all or readiness assumption based on a
later source position. Selected references cannot first request construction inside overload trials,
contextual callback rewalks or inspection. Singleton source calls prepare before candidate trials.

The required source profile is:

| Dimension | Included |
|---|---|
| Function producer | Local top-level, single-declaration, non-overloaded `declare function`; explicit return; no implementation body, receiver or outer application; explicitly typed ordinary parameters |
| Recipes | Owned parameters, primitives/literals, tuples/arrays/readonly, static records, unions/intersections, bare bound chains, ready local interfaces, retained references and transparent aliases including static tuple-index projection |
| Returns/defaults | Constructed tuple returns, absent defaults, closed ready defaults and validated earlier-binder defaults; invalid forward defaults retain their diagnostics |
| Uses | Singleton calls, finite immutable copies, unused copies, supported unknown-target assignments and source relations to closed non-generic callable targets |
| Initial relation targets | `() => unknown` and closed tuples of unknown leaves; ordinary result-member checks do not escape registered values |
| Call inputs | Ready declared data, literals and supported literal assertions; no selected registered value passed as a value argument |

Required full/partial class cases use existing complete query-free class retention and immutable
constructor descriptors. This decision does not make classes lazy or query their construction.

### Source-owned field construction

One source construction owner holds private parameter/constituent creation receipts, bound/default/
parameter/return slots, continuation frames, held header effects and a descriptor terminal.
Unstarted, active and completed slots are distinct. Only an actual source operation holds the
operation-scoped execution permit; a pure query cannot create one.

Field operations include acquiring signature binders, completing a bound/default, completing
parameter types or the declared return, validating a header bound, and finishing the descriptor.
They arise from actual semantic reads, not caller-kind switches or a fixed finish-all-fields schedule.

| Source operation | Required execution |
|---|---|
| Lexical header | Complete/validate bounds in declaration order before each parameter's lexical creation receipt; service bound-chain demands; complete remaining fields and adopt held work |
| Singleton call | Discover the reference without forcing shape; acquire binders, bounds/defaults, parameters, then return and complete descriptor before native candidate instantiation |
| Annotated callable assignment | After initializer checking, decide the pure relation at this lexical checkpoint on retained operands; retain the decision for later reporting |
| Copy/unknown transfer | Safety preflight only; no shape-field construction |

The early assignment checkpoint is essential. Queuing an obligation and first executing its relation
after the declaration header is too late. Later reporting consumes the retained semantic decision;
it must not execute the relation again. Preserve source spans, grouping and independent RHS effects.

Logical first-construction identities belong to the source owner, not numeric Store IDs. Canonical
arrays reuse their construction identity across bounds and returns. Alias actual-argument construction
and semantic erasure are separate operations. Freeze surviving union order and constraint-use
witnesses before descriptor publication; later inference never reads the mutable construction ledger.
Use existing constructors/reductions, not a private union evaluator or source-use simulator.

### Pre-ready query views and bounds

Use a read-only callable view separate from FunctionType: either a ready descriptor or a registered
application with an immutable availability snapshot. Each field is missing, complete or failed.
Bound presence is separate from availability: absent, present TypeId, or a registered bound source.
Missing availability must never mean unconstrained.

Registered-owned parameter lookup precedes the Store constraint column, including when that
parameter occurs inside a completed array, tuple or alias. Keep selected constraints private and
snapshot-owned until complete publication; incremental Store writes cannot leak newer availability
into an older query snapshot.

The relation's actual reads drive requests: generic dispatch acquires binders; specialization retains
lazy bound sources; parameter-shape access and return access obtain their fields; specialization
validation and constraint comparison demand bounds when needed. There is no invented erasure owner.
After preflight, a valid unknown-target success may avoid unnecessary bound specialization. A relation
that succeeds using completed parameter/return fields need not finish unused bounds or the descriptor.
Never publish a partial FunctionType to satisfy this interface.

### Transactions, snapshots and effects

Pure attempts can return one ordered `NeedsSource(application, field operation)` alongside existing
native-identity demands. Do not sort or batch source requests using native-demand scheduling.
Rollback the complete receiving query journal before releasing borrows and servicing source work.
Discard local relation contexts, evaluator/projection work and pending plans on that attempt.
Retry only the pure operation over retained operands and the new snapshot, not an expression,
argument, annotation, callback or effectful retry closure.

Publication creates a new persistent immutable availability snapshot. Old snapshots retain old
missing/complete/failure answers. Query context includes registration domain and snapshot identity;
a new invalidation token without persistent old lookup semantics is insufficient. Completing all
required fields interns one immutable FunctionType. Typed construction failure publishes no descriptor.
Ordinary diagnosed header recovery remains explicit and keeps its lexical reporting obligation.

Header construction captures detached source-owned effects. A forward consumer can complete work,
but the lexical header later adopts its held effects once, without relowering. Source finalization
accounts for every held batch, including failures. Existing EventStore ordering remains authoritative.
Do not suppress/deduplicate committed records, snapshot all of Pass or rewind named identities.

### Immutable occurrence transport and native completion

Completed signature metadata retains ordered constraint/default uses by owned parameter identity.
Include this call-observable order in function identity. Clone/preserve it through ordinary rewriting;
remap it with the exact binder bijection on specialization/freshening; remove it on erasure and clear
constraint uses after diagnosed circular-constraint clearing. Never regenerate it from canonical IDs.
Binder-only metadata adds no class-SCC roots. Retained class/synthetic descriptors obey the same
complete constructor contract, without a production empty-record default for missing information.

After publication, the existing full/partial inference owners use explicit continuation frames with
original candidates, active seeds and completed `(result, violation)` cells. Dependencies complete
before original-candidate validation. Completed reads return identical TypeIds without new queries
or allocation. Recursive reads use the original seed, not a derived fallback.

Recursive defaults additionally use `PreparingDefault`: an active earlier binder supplies its seed;
an already-completed binder supplies its terminal value. A nested provisional default answer is not
the outer owner's Done result. Declaration index alone never proves completion. Source-construction
cycles and inference/default seed recursion are different states and failure policies.

Preserve full no-candidate unknown seeding, partial absence, fresh-exemption differences, violations,
native P/R inputs and typed genuine exhaustion. All frames/results remain attempt-local; retry starts
from original candidates after rollback. No outer map-equality fallback iteration remains.

## Acceptance and integration

Static source inspection covers 37 retained rows in 22 fixtures: 17 basic and 20 construction-risk
rows, comprising 35 depth mismatches and two separately lost errors. This is not an executed selector
or a repair receipt. The corpus also retains separate seed, occurrence-order and reverse-recovery
controls. All their valid reads and required downstream errors remain acceptance obligations.

1. Commit leader-authored lifecycle/admission/source specs before implementation. Include rejection
   of future writes, escapes, overload/union uses and unclosed copies, not just positive examples.
2. Implement and independently verify model, immutable views/snapshots, preflight, field-demand
   transactions and metadata transport without public source cutover.
3. Connect source registration, header/assignment/call checkpoints and native completion. Prove the
   header/forward and constructed-return call/relation pairs through the real CheckerSession route,
   with no relation replay at reporting, no effect duplication and no inspection-driven construction.
4. Close all 37 rows and additional full/partial/default controls. Retain accepted R1c/publication
   semantics, M2 transfer ownership, differential/repro/official gates and unchanged-tree final gates.

Source integration waits for M2 acceptance wherever files/ownership overlap. No parallel production
edits across that boundary. No failed selected request falls back to legacy construction. Keep the
public route unchanged until the complete execution proof is accepted; a model-only proof is not
feature closure.

## Consequences and alternatives

This adds explicit callable availability and changes selected assignment semantic timing. It preserves
immutable published meanings and lexical reporting order. General lazy bodies/values, arbitrary
registered-value transport, class rescheduling and all of C1b–C4 remain outside this decision.
The construction boundary needed by these cases is no longer deferred from backlog 44 to backlog 120.

A checker-local pending operand is smaller only for a restricted experiment; carrying it through
ordinary TypeId APIs either reconstructs too early or spreads a second operand representation.
An explicit model-visible registration is the consistent full-scope choice. Eager prediction,
query once-cells, physical allocation ranks and caller-kind switches do not satisfy the witnesses.

Confidence is medium until execution proof. Reject the cutover if actual source continuations cannot
produce the required order exactly once without replay or inspection-controlled construction. Do not
turn that failure into waived diagnostics, partial descriptors or new runtime unsupported admission.
