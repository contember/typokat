---
id: 0005
title: Persist generic signature binders in function types
status: accepted
date: 2026-07-12
---

# 0005 — Persist generic signature binders in function types

## Context

Generic free functions currently publish their type parameters through a pass-local
`TypeId -> Vec<TypeParamId>` map. Class, interface, object call, and object construct
signature lowering cannot use that metadata reliably. More importantly, substituting an
outer type parameter re-interns a changed `FunctionType` from its positional parameters
and return only. A generic member such as `Box<T>.map<U>` therefore loses `U` after
`Box<number>` is instantiated.

The map also leaves genericity out of a function type's structural key. The relation
engine receives only two `TypeId`s and must keep its durable three-word cache sound; a
verdict cannot depend on mutable checker-pass metadata or an alignment environment that
the cache key cannot identify. The type-store invariant still applies: types are
hash-consed, equality is `TypeId` equality, and declaration type parameters retain their
named, unique `TypeParamId` representation from ADR-0002. A whole-model de Bruijn
migration is not justified by generic signatures.

## Decision

We will make generic binders an owned, persistent part of `FunctionType`. A function
will carry an ordered list of generic-parameter descriptors. Each descriptor contains
its existing unique `TypeParamId` plus optional constraint and default `TypeId`s. Source
names remain on `TypeParamType` for display only. Function, method, call-signature, and
construct-signature lowering will all build this same representation; free generic
functions will migrate from the pass-local map to it.

The function structural key and hash will include binder count, ordered ids, constraints,
and defaults before positional parameters and return. Names are excluded. Thus an exact
signature identity includes every call-observable binder field, while two independently
declared alpha-equivalent signatures need not hash-cons today: their distinct named ids
remain distinct, as ADR-0002 requires. The descriptor fields are necessary even though an
id distinguishes declarations: outer substitution can change a binder's constraint or
default without changing that inner binder id.

The existing store-side constraint column remains the apparent-type source while a
declaration body is checked. A persistent signature descriptor is the source for external
call instantiation and relation, so it survives after the checker pass that lowered the
declaration. Invalid/circular constraints retain M24 behavior: they diagnose during
lowering and are not made relation-permissive through a descriptor.

There are two distinct substitution operations:

1. **Outer substitution** preserves the inner binder list, removes its ids from the
   incoming substitution map, and rewrites free outer references in parameters, return,
   constraints, and defaults. This is the `Box<T>.map<U>` path.
2. **Signature instantiation** consumes the selected signature's own binders, checks
   explicit arguments or infers them with the existing inference engine, applies defaults
   by declaration order, validates constraints, and returns a non-generic callable
   surface. It must not leave a consumed binder attached to that call candidate.

Generic-function relation derives binder alignment solely from the persistent source and
target signatures. For two generic signatures, it aligns corresponding binder ids by
position, checks compatible constraints under that alignment, then relates parameters and
return through the aligned types. Defaults affect omitted call arguments, not the
generic-to-generic relation rule. Generic-to-specific compatibility derives its temporary
instantiation only from the two signatures and uses the existing inference machinery where
candidate collection is needed; it does not create a second inference engine.

The alignment is local relation state, never side metadata. Any nested comparison whose
answer depends on an aligned or temporarily instantiated binder bypasses the durable
three-word relation cache. Only a completed outer `(source TypeId, target TypeId,
RelationKind)` verdict is cacheable, because the full binder descriptors make that verdict
deterministic. The existing provisional-cycle rule remains stricter: an answer dependent
on an in-flight ancestor assumption is never committed. This prevents both alpha-alignment
context and recursive assumptions from poisoning a later relation query.

Function display will render the persistent list using parameter names from their
`TypeParamType`, for example `<T extends Bound = Default>(value: T) => T`; ids remain
internal. A substituted outer signature renders its retained inner binders and substituted
constraint/default normally.

## Consequences

Calls, overload lists, object/interface members, class instance/static members, and
construct signatures have one durable source of generic metadata. Outer generic
instantiation cannot silently erase a method binder, and free generic functions no longer
depend on an exact-template lookup in a mutable pass map.

The interner, structural key, equality tests, substitution code, signature lowering, call
candidate construction, overload compatibility, relation, and renderer must change in
lockstep. Hash/equality parity and outer-substitution preservation require focused tests.
The relation implementation has a narrow uncached path for binder-dependent subqueries;
it must never broaden into a cache keyed only by unaligned child `TypeId`s.

## Alternatives considered

### Keep or extend the pass-local side map

Rejected. It loses `U` when a substituted function receives a new `TypeId`, makes type
identity incomplete, and would require relation to consult mutable hidden metadata.

### Migrate all declaration parameters to de Bruijn indices

Rejected. It conflicts with ADR-0002's named-unique declaration-parameter invariant and
would broaden this signature feature into a whole-model migration. Alpha equivalence is
handled by deterministic relation alignment instead.

### Cache aligned child relations under the existing three-word key

Rejected. A relation such as `T` against `string` can mean different things under two
alignments. Omitting that context would permit order-dependent false cleans. Binder-aware
child comparisons stay uncached; only the full persistent-signature pair may use the
durable cache.
