---
id: 05
title: Call / method / construct signatures in object & interface types
blocked-by: []
---

# 05 — Call / method / construct signatures in object & interface types

**Summary.** Deferred feature, candidate for its own milestone. A steady source of false negatives in
the in-scope diff against the official suite.

## Problem

A method-shorthand or call-/construct-signature *member* in an object type or interface
(`{ f(x: number): void }`, `interface T { f(x): void }`) is not modeled: typokat lowers the type as
if the member weren't there, so it collapses to `{}`.

```ts
interface T { f(x: number): void; }
declare var t: T;
const n: number = t.f;   // typokat: TK2339 "Property 'f' does not exist on type '{}'"
t = () => 1;             // tsc: TS2322 (fn not assignable to T); typokat diverges
```

Evidence: on `conformance/.../assignmentCompatWithCallSignatures2.ts` tsc's baseline has **12**
errors; typokat reports **4**, and renders the target interface `T` as `{}`. It recurs across
`assignmentCompatWithCallSignatures*`, `assignmentCompatWithConstructSignatures*`, and
`covariantCallbacks.ts`.

This is **not** the same as class methods (M11, supported) — it's method/call/construct signatures as
**members of object-type literals and interfaces**, a distinct feature not covered by M0–M22.

## Approach / acceptance

Model call / method-shorthand / construct signatures as members during object- and interface-type
lowering, so they participate in member access and assignability. Acceptance: the interface renders
with its members (not `{}`), and the call-signature conformance files above converge toward tsc's
error counts. Secondary: verify whether `fn → {}` is itself a real typokat bug when triaging (a
function appearing non-assignable to a wrongly-empty `{}` target).

## Touch points

Type lowering for object-type literals and interfaces (`lower_annotation` / object-member lowering);
function/call/construct-signature members in the type store + relation engine.

<!-- Origin: official-suite finding F1. -->
