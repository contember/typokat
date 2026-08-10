---
id: 76
title: Lazy declaration and value-type resolution
blocked-by: [./46-return-path-analysis.md, ./48-no-implicit-any.md]
---

# 76 — Lazy declaration and value-type resolution

**Summary.** Replace the conservative `unknown` return used by an unannotated
forward function call with a demand-driven declaration-type query that resolves
function returns and referenced value declarations exactly, including cycle
diagnostics, without changing source-order checking or leaking provisional types.

## Problem

The shipped declaration-hoisting sprint reserves the stable callable surface —
parameters, generic constraints, explicit returns, and overload signatures — before
executable statements. An
unannotated body cannot be inferred safely at that point with the current sequential
`DeclTypes` fill:

- `void` is unsound (`const x: void = f(); function f() { return 1; }` becomes
  falsely clean);
- the error type is any-like and suppresses result obligations;
- `unknown` is sound but over-reports valid result-consuming forward calls;
- checking bodies early changes diagnostic/obligation timing and still fails when a
  return expression references a later inferred value declaration.
- a hoisted unannotated `var` has the same missing value-type problem, while using
  `unknown` would be permissive as an assignment target.

The object-binding review pinned the cross-file form on 2026-08-10. With a consumer script sorted
before `var crossProjectLeaf = "ready"`, typokat exits clean and drops the `TS2322` wrong-type
witness from tsc 6.0.3. The equivalent flat object `var { crossProjectLeaf } = ...` reports
`TK2454` on both the valid and invalid reads and still drops `TK2322`; placing the declaration path first produces
the expected single `TK2322`. Reversing the supplied root list does not change either result because
the project route canonicalizes path order. The parked acceptance witnesses are
`tests/cases/b48_object_binding_publication/project_consumer_first/` and
`tests/cases/b48_object_binding_publication/project_declaration_first/`.

TypeScript separates declaration visibility from type availability. It resolves a
signature return on demand, tracks `Unresolved → Resolving → Resolved/Errored`, and
reports the implicit-any cycle family (TS7022/TS7023) before using a recovery type.
typokat needs the equivalent model before it can claim exact inferred-return hoisting.

Class accessors and static methods are the same declaration-type demand, not a separate callability
bug. The official-suite witnesses are exact: the static getters in
`classPropertyAsPrivate.ts`, `classPropertyAsProtected.ts`, and
`classPropertyIsPublicByDefault.ts` return `null` but are published as `void`, producing a safe
`TK2349`; `accessorsAreNotContextuallyTyped.ts` publishes an arrow-returning getter as `void` and
produces the same `TK2349`; and `typeOfThisInStaticMembers.ts` publishes static `bar()` as `void`,
causing `TK2351` on `new t(...)`/`new t2(...)`. Backlog `49` owns the eventual strict-null
`TK2721` for calling a null getter; this item owns recovering each inferred declaration type.

Attached namespace class/static cycles are the same demand problem. When an exported class static
initializer refers back to its namespace root, publication currently records
`decl/class-declaration/namespace-payload-static-cycle` and withholds the complete root. This item
owns resolving that cycle without partial class or namespace publication.

## Approach / acceptance

Add a declaration-plan layer keyed by `DeclId` for function declarations and the value
declarations their return expressions can demand. Reserve stable generic ids,
constraints, parameters, and explicit returns once; resolve inferred declaration types
through a cycle-guarded query; publish only terminal function `TypeId`s and overload
objects. A provisional result must never enter relation/flow caches or phase-2
obligations.

Keep normal statement/body checking source ordered and exactly once. Type-query mode
must not evaluate parameter defaults, duplicate diagnostics/obligations, or mutate
narrowing state. Coordinate return aggregation with backlog
[`46`](./46-return-path-analysis.md) and TS7022/TS7023 emission with backlog
[`48`](./48-no-implicit-any.md).

Corpus first, cross-checked with `tsc 6.0.3 --strict`: forward unannotated ordinary and
generic returns, later inferred variables referenced from function returns, direct and
mutual return cycles, mixed value/function cycles, annotated cycle breakers, overload
implementations, reordered declarations, and repeated-query/cache-order probes.
Acceptance requires exact verdicts without conservative `unknown` over-reports, no
duplicate/reordered body diagnostics, and no query-order-dependent false negative.
It also requires the parked cross-file object-binding witnesses to agree in both path orders. The
fix must resolve the general declaration-type demand; do not add object-binding-only pre-inference.

Add the five official files above as getter/static-method demand controls. The arrow getter must be
callable, both `this`-returning static methods must preserve their constructor surfaces, and the
null getters must expose `null` to backlog `49` rather than `void`. Keep the existing backlog `46`
return aggregation and backlog `48` cycle-diagnostic dependencies; do not special-case call/new
sites around an unavailable declaration type.

## Touch points

Value declaration plans beside `DeclTypes`, identifier/call type resolution, function
return aggregation, generic signature metadata, overload publication, diagnostics for
TK7022/TK7023, flow/relation cache boundaries, conformance fixtures, and the official
suite scoreboard.

<!-- Origin: sprint-2026-07-11 WU1 stop gate; TypeScript 6.0.3 getReturnTypeOfSignature audit. -->
