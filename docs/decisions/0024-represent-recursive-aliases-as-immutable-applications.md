---
id: 0024
title: Represent recursive aliases as immutable applications
status: accepted
date: 2026-08-26
---

# 0024 — Represent recursive aliases as immutable applications

## Context

WU4 of the project-local library-universe sprint must publish the real `IDBCursor` surface. That
surface depends on `IDBValidKey`, whose definition recurs through an array. The same lowering
boundary must handle legal recursion through tuples, functions, objects, mutual aliases, and
generic applications without replacing their meaning with an error type.

The store invariant says that a `TypeId` has one stable structural meaning and that structural
equality is integer equality. Reserving an incomplete `Array`, `Tuple`, or `Function` row and
filling it later would change the transitive meaning of that identity after parent types, free
parameter summaries, evaluator memos, or relation results could already refer to it. This is the
mutable-row design rejected for class applications by
[ADR-0006](0006-immutable-class-instances-and-scc-publication.md).

Marking the complete recursive SCC unavailable is sound but cannot preserve the required
`IDBCursor` behavior. Error, `any`, `unknown`, `never`, empty-object, and permissive opaque
placeholders are not sound in every relation polarity.

## Decision

We will model a recursive type alias with an immutable declaration identity and immutable lazy
applications:

- A project allocates one project-local `AliasId` for each recursive alias declaration.
- An immutable `AliasTemplate` type is keyed by that complete `AliasId`. Its name is provenance and
  diagnostic presentation, not nominal assignability identity.
- Every reference or application is a regular immutable `Instantiation` whose base is the alias
  template and whose sorted arguments contain the complete substitution.
- The alias body and parameters live in a checker-owned draft during construction and in an
  immutable published-alias registry after publication. They do not live in a mutable store row.
- The checker allocates all alias identities in an alias SCC before lowering any body. It publishes
  the complete SCC atomically as `Ready`, or publishes the complete SCC as typed `Unavailable`.
  Draft state is never queryable.
- `SemanticQueryCoordinator` unfolds one outer alias application through a query-local overlay
  before identity, durable cache lookup, or relation-cycle handling. Recursive applications inside
  the substituted body remain lazy.
- Alias assignability is equi-recursive and structural. `AliasId` never creates nominal
  compatibility. The existing provisional relation-cycle rule may close regular recursive pairs,
  but assumption-dependent outcomes enter no durable cache.
- Non-regular recursion is bounded by a per-query distinct-application budget. Exhaustion
  over-reports through a typed outcome; it never guesses success or substitutes a recovery type.
- A recursive SCC is legal only when every alias-reference cycle crosses a productive structural
  boundary. Union, intersection, parentheses, and changes confined to recursive type arguments do
  not make an otherwise direct cycle productive.

The general source checker and the project-local declaration-product lowerer will consume the same
recursive-alias plan and published semantic capability. Product lowering derives guarded edges from
authenticated type recipes and resolved group references, not from names or the legacy replay
graph.

## Consequences

- Ordinary arrays, tuples, functions, unions, and objects remain immutable and normally
  hash-consed.
- A recursive alias application has stable shallow identity `(template, arguments)` and does not
  require eager infinite expansion.
- Query context identity must include the published alias registry. Readiness and poison preflight
  must run before same-ID success.
- Substitution composes through instantiation arguments. It treats an alias template as an opaque
  declaration leaf and never traverses checker draft state.
- Diagnostics render the alias name and arguments, with a cycle guard for explicit expanded-body
  rendering. A non-generic application does not render empty angle brackets.
- WU4 must prove cold/warm and forward/reverse relation order, absence of provisional cache writes,
  bounded non-regular recursion, and exact `IDBValidKey`/`IDBCursor` behavior.
- Unsupported syntax or failed construction makes the complete publication SCC unavailable. No
  partial alias body or fabricated member surface becomes Ready.

## Alternatives considered

### Reserve Array, Tuple, and Function shells

This is superficially smaller because relation code already understands the final constructors.
It loses because a reserved constructor lacks its complete structural key, and filling it changes
the transitive meaning of every parent that already references it. Each additional guarding
constructor would also need a separate mutable reservation mechanism.

### Mutate a reserved Instantiation shell

This reuses the existing tag but still changes the shell's base after allocation. Keeping the body
in a checker-owned registry preserves the same reuse without mutating a store identity.

### Use cyclic declaration recipes

Declared recipes currently reject cycles and do not represent every required structural boundary.
Turning them into an equi-recursive evaluator would be a wider second recursive type system.

### Publish an opaque alias or mark the complete SCC unavailable

An identical-only opaque alias and whole-SCC unavailability are conservative fallbacks, but neither
provides the required structural TypeScript behavior for `IDBValidKey`. A permissive opaque or error
fallback could drop diagnostics and is forbidden.
