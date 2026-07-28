---
id: 83
title: Contextual generic signature relation
blocked-by: []
---

# 83 — Contextual generic signature relation

**Summary.** Model tsc's contextual instantiation when relating generic function-typed members;
the current alpha-alignment rule safely rejects four compatible inheritance members.

## Problem

Persistent generic signature binders and cache-safe alpha alignment are shipped, but equal-arity
generic-to-generic relation currently compares binders positionally under one universal alignment.
TypeScript also attempts a contextual instantiation of the source/target signatures when inference
from parameter occurrences can prove a subtype relation.

The strict-tsc 6.0.3 official witness is
`callSignatureAssignabilityInInheritance4.ts`: interface `I extends A` is clean, while typokat
emits four `TK2430` records at harness line 36 for members `a6`, `a11`, `a15`, and `a18`.
Member `a17` is the clean control and must remain clean. These are safe over-reports; the file's
four `TS2564` definite-assignment diagnostics are independently owned by backlog `47`.

Contextual instantiation is query-local. Its inferred substitutions depend on the exact signature
pair, variance position, constraints, overload member, and trial order; a verdict reached under
that environment must not enter the durable context-free relation cache. The in-flight cycle key
must still distinguish semantic binder environments so recursion remains order-independent.

## Approach / acceptance

Add a bounded contextual generic-signature trial beside the existing alpha-aligned and one-way
source-specialization paths. Infer candidate substitutions from the relevant parameter positions,
check constraints, then relate the instantiated shapes under a lexical binder environment. Reuse
the query transaction and contextual stack-key discipline; never mutate stored signatures or
publish a context-dependent verdict into the three-word durable cache.

Pin `a6`, `a11`, `a15`, and `a18` as strict-tsc-clean controls and `a17` as the non-regression
control. Add adversarial reverse-order, recursive, constrained, overload, and incompatible-nearby
cases proving no false negative, no cache hit across different instantiations, and identical
results in both relation orders.

## Touch points

`crates/typokat-relate/src/relate/relation/objects.rs`,
`crates/typokat-relate/src/relate/relation/mod.rs`, relation cache/context tests,
interface heritage validation, and the official-suite ratchet.

<!-- Origin: post-WU7 official-suite adjudication at a3fc116. -->
