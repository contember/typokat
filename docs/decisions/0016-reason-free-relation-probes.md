---
id: 0016
title: Suppress the reason chain on relation probes whose caller discards it
status: proposed
date: 2026-07-25
---

# 0016 — Suppress the reason chain on relation probes whose caller discards it

## Context

The root [`CLAUDE.md`](../../CLAUDE.md) names as one of the four pillars that the relation engine
returns `Relation::No(ReasonChain)` — *"never a bare `bool`"* — explicitly **so that reporting runs
the same path** as checking. [`architecture.md`](../reference/architecture.md) §6.4 states the same
requirement: retrofitting a cause tree onto a boolean engine is a rewrite, so the engine builds one
on every failing path from the start.

The durable cache, however, is the three-word `(TypeId, TypeId, RelationKind) → bool` of §6.1. It
holds the *verdict*, not the reason. `Relater::relate` (`src/relate/relation/mod.rs`) reconciles the
two by short-circuiting a cached `true` and deliberately letting a cached `false` fall through to a
complete re-derivation of the failing subtree, purely to rebuild the chain the cache cannot store.
The comment above the cache lookup says so in as many words.

The consequence is that **a cached `false` is never a memo hit**. Any rule that keeps probing after
a failure becomes a branch point, because each failing probe re-walks its whole subtree instead of
returning the memo. Cost is `a^d`, where `d` is nesting depth and `a` the branch factor of the
enclosing OR rule. The dominant branch point is `relate_union_target` (`set_types.rs`); the same
shape appears in `relate_signature_sets`, `relate_object_to_function`,
`relate_construct_signature_sets`, and the non-object branch of `relate_source_members_to`
(`objects.rs`). Nothing bounds it: `src/relate/` has **no work budget at all** — no depth cap, no
node limit, only the assume-true cycle stack of §6.3, which addresses termination, not fan-out.

Two independent reproductions, each with a different shape and a different probe.

Nested Redux-style discriminated union (`{ payload, type }`, where `payload` sorts before the `type`
discriminant so every arm walks the whole payload before rejecting), nesting depth fixed at 9,
22-line files, only the union-arm count varying:

| arms | 2 | 3 | 4 | 5 |
|---|---|---|---|---|
| wall clock | 0.0032 s | 0.0181 s | 0.1781 s | 1.2259 s |

The fitted exponent is the nesting depth. Alternating recursive union members
(`Lk = { p: L(k-1) | R(k-1) }` / `Rk = { r: L(k-1) | R(k-1) }` against `Sk = { p: S(k-1); r: S(k-1) }`),
one failing assignment, two arms, depth varying:

| depth | 14 | 18 | 20 | 22 |
|---|---|---|---|---|
| file | 45 lines | 57 lines | 63 lines | 69 lines |
| wall clock | 0.01 s | 0.18 s | 0.74 s | **2.93 s** |

Exactly ×4 per two levels — `2^d`. In both reproductions an identical-shape control where the
relation *succeeds* is completely flat at the process floor across every depth and arm count, so the
exponent is the failure path, not the type graph. The same split in relation frames (2 arms,
depth 4 → 8, counted by `RelationSourceColdMeasure`):

| | frames | of those, rebuilds of a cached `false` |
|---|---|---|
| mismatched leaf, depth 4 | 77 | 63 |
| mismatched leaf, depth 8 | 1277 | 1251 |
| matching leaf, depth 4 | 9 | 0 |
| matching leaf, depth 8 | 17 | 0 |

98% of the failing run's frames re-derive a verdict the cache already holds. None of those reasons
reaches a diagnostic: every one of the OR-probe call sites listed above **discards** its children's
reasons by construction — `relate_union_target` returns a flat `NoUnionMember`, `relate_signature_sets`
and `relate_object_to_function` return bare leaves. The work buys nothing.

This is not adversarial input. A four-arm nested discriminated union with one wrong leaf type — an
ordinary Redux action shape — costs 1.14 s at ten levels (a 24-line, 1.8 KB file) and 16.8 s at
twelve (28 lines, 2.2 KB).

## Decision

We will make §6.4's reporting mode a **real mode**: thread a `want_reason: bool` through
`Relater::relate`, and when a lookup finds `cached == Some(false)` and `!want_reason`, return a
shared leaf `Relation::No` immediately instead of re-deriving the subtree.

`want_reason` is granted only to callers that actually consume a child's reason. The top-level
query passes `true`. Every OR-probe call site that already discards its children's reasons passes
`false`. `relate_source_members_to`'s `last_child` (`objects.rs`) keeps `true`, because it is the
one probe loop that renders a failing candidate's own reason.

This changes **nothing** about which verdicts are cached or how they are derived. The `false` is
already in the cache and is already documented as always genuine: the assume-true rule of §6.3 can
only ever manufacture a spurious `true`, never a spurious `false`. The provisional-`Yes` discipline
of §6.3 is untouched. Only the reason chain is suppressed, and only where the caller throws it away.

Diagnostics must be byte-identical for every existing test and fixture. If any message changes, the
implementation stops and reports rather than updating the expectation.

## Consequences

- **The pillar is amended, not abandoned.** `Relation` still never returns a bare `bool`, and every
  reason a user sees is still built by the same engine on the same path. What is now true is
  narrower: *a `No` returned in reason-free mode cannot explain itself.* Any future caller that
  wants to inspect a nested reason must opt in by passing `want_reason: true`. Adding a new OR probe
  and defaulting it to `false` will silently flatten whatever it used to report.
- **One known existing case must be audited, not assumed.** `relate_source_members_to`'s
  `last_child` (`objects.rs`) is the only probe loop found that keeps a failing candidate's reason;
  it must keep `want_reason: true` or explicitly accept a leaf. Every other caller has to be checked
  for the same pattern before the change lands — a missed one is a silently degraded diagnostic, not
  a test failure, unless it is pinned.
- **The cache becomes authoritative for `false` in a way it is not today.** Today a cached `false`
  is always re-derived, so its explanation is always freshly built in the current context. After
  this, a cached `false` in reason-free mode is answered from the cache alone. This is sound only
  because a `false` never rests on an assumption; it is worth restating whenever §6.3 is touched.
- **The reporting spine is unchanged and still re-derives.** A frame with `want_reason: true` that
  hits a cached `false` still falls through. That path is bounded by the single chain the diagnostic
  renders, not by the OR fan-out, which is what makes it affordable.
- **This is not a work budget.** The dominant exponential goes away; `src/relate/` still has no
  depth cap and no node limit. Do not read this ADR as "relation work is now bounded".
- **Documentation follow-on.** Architecture §6.1/§6.4 and, if its wording proves too absolute,
  [`invariants.md`](../reference/invariants.md) §1 need amending to describe the two modes. That
  edit lands with the implementation, not with this ADR.
- Safety net: the perf guard and the four reporting pins in
  `src/relate/relation/failing_relation_scaling_spec.rs` are committed before the fix. The guard
  fails today; the pins pass today and must keep passing byte for byte.

## Alternatives considered

**Cache the `ReasonChain` alongside the bool.** The most conservative option: the cache answers both
questions, one code path survives, and the pillar needs no amendment at all. It loses on cost. §6.1
calls the relation cache possibly the single largest perf element of the whole checker, and its
cheapness comes from being three `u32`s; making the value a heap-allocated cause tree undoes that.
§6.2 notes narrowing creates swarms of short-lived types, so the number of distinct failing pairs is
large and volatile — we would pay unbounded memory to retain explanations that, by the measurement
above, are discarded 98% of the time before anyone reads them.

**Two passes: run reason-free, then re-run the top-level failing query with reasons on** (the shape
of tsc's `reportErrors` flag). Attractive because it confines the mode switch to one entry point.
It loses because the second pass *is* today's behaviour: with reasons on, cached `false`s fall
through again, so the reporting pass reproduces the same exponential on the same input. Making pass
two cheap requires bounding it, and bounding it changes messages. It also needs the two passes to
share a cache without the reporting pass promoting anything the first pass did not, which is more
§6.3 surface area to get wrong than threading one flag.

**Do nothing and accept the exponential.** Rejected on the evidence: the triggering shape is an
ordinary nested discriminated union with one wrong leaf, not a crafted input, and it costs seconds
on a 24-line file. A checker that is fast on correct code and exponential on incorrect code is
useless in the loop where it is actually used.
