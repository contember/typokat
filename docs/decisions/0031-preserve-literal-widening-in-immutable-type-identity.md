---
id: 0031
title: Preserve literal widening in immutable type identity
status: accepted
date: 2026-09-06
---

# 0031 — Preserve literal widening in immutable type identity

## Context

The as-const sprint exposed information loss across declaration publication, object members,
contextual parameters, and generic inference. A boolean attached to an entire expression cannot
represent a union containing both ordinary and non-widening literals. Recovering that information
from source syntax after inference misses published values and changes inference constraints too
late. Ordinary namespace constants and readonly class fields must also retain the distinction.

The bounded type-store experiment established distinct immutable IDs, selected union algebra,
canonical enum ownership, and substitution transport. It did not establish a compatible checker
cutover. Two independent feasibility reviews identified producer compatibility as the missing
boundary, rather than a defect disproving this representation.

## Decision

Represent an ordinary widening literal as an immutable hash-consed `WideningLiteral` node whose
operand is its canonical regular literal or enum-member ID. Literal annotations and non-widening
const-context leaves use regular IDs. The wrapper creates no additional enum registry owner and
admits no unavailable member or recovery operand. Existing object-literal excess-property
freshness remains a separate policy.

Storage, substitution, projection, semantic demand, and inference candidate collection preserve
complete mode-bearing IDs. Same-value unions obey `regular | widening = regular`, independently
of input order. Different-value contributions retain their individual modes. Actual widening
maps only widening contributions to their primitive or enum base. Display hides the wrapper.

Relation-only normalization compares regular identities after coordinator publication/poison
preflight and current overlay/frontier handling, before identity/cache/cycle success. It does not
rewrite shared evaluation overlays or stored types. Equality/narrowing predicates compare regular
values while retaining original mode-bearing result IDs. Exhaustion promotes no durable cache work.

Migrate ordinary producers and shared widening/inference consumers coherently, including namespace
constants and readonly class fields. Do not reinterpret unmigrated regular IDs by origin, introduce
a transitional flag, or repair inferred bindings afterward in the call checker. Inference applies
its widening policy before validating final dependent constraints.

Const ownership remains syntax-directed. Only literal-owned object members become readonly and
literal-owned arrays become readonly tuples. An identifier or member reference does not acquire a
deep readonly transformation because its containing literal is const-qualified.

## Consequences

The complete immutable type identity carries widening behavior through existing type consumers.
This expands the producer and exhaustive-consumer migration beyond the small upstream const-context
syntax change; the sprint must report that missing-foundation cost separately.

Adoption requires model, relation/query, enum, frozen-store, display, inference, and source-level
tests, plus independent full-diff review and the required differential gates. This decision records
the implementation direction, not a claim that the experiment or sprint has shipped. Execution and
receipts remain in the [active sprint](../sprints/sprint-2026-08-17-as-const.md).

## Alternatives considered

- **Occurrence-owned composite provenance graph:** can preserve modes without changing `TypeId`,
  but needs a parallel semantic graph through members, substitution, callables, and memoization.
  The existing type store provides a smaller authoritative identity boundary.
- **Whole-result provenance and syntax reconstruction:** cannot represent mixed contributions or
  preserve modes through published/contextual values. Independent review found both dropped errors
  and false positives in that approach.
- **Regularize in shared semantic evaluation:** erases evidence before inference candidate fixing.
  Relation equivalence and actual widening must remain separate operations.
