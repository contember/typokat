---
id: 0030
title: Treat the WU6 typokat oracle as a regression ratchet
status: accepted
date: 2026-08-29
---

# 0030 — Treat the WU6 typokat oracle as a regression ratchet

## Context

WU6 originally required the project-local session runner to reproduce every normalized row in the
committed official-suite scoreboard exactly. The first live 32-project panel rejected thirteen
rows. Every row exposed a runner bug: the runner replaced its precise incomplete surface identities
with the class token `unresolved`, although only four scoreboard rows have class-only unresolved
authority. One of those rows also contained real checker progress. The project-local route removed
the old `TK2339` false positive from `indexersInClassType.ts`; the pinned TypeScript-Go exact-flags
oracle is clean on the same source.

The complete named diff was:

- `privateProtectedMembersAreNotAccessibleDestructuring.ts`
- `privateStaticMemberAccessibility.ts`
- `genericSetterInClassTypeJsDoc.ts`
- `indexersInClassType.ts` — precise incomplete IDs collapsed and one `15:2339` occurrence removed
- `classWithBaseClassButNoConstructor.ts`
- `classWithNoConstructorOrBaseClass.ts`
- `derivedClassIncludesInheritedMembers.ts`
- `derivedClassOverridesIndexersWithAssignmentCompatibility.ts`
- `derivedClassOverridesProtectedMembers.ts`
- `derivedClassOverridesProtectedMembers2.ts`
- `derivedClassOverridesProtectedMembers3.ts`
- `derivedClassOverridesProtectedMembers4.ts`
- `derivedClassOverridesPublicMembers.ts`

Exact reproduction would require the target lifecycle to restore a known false positive. Replacing
the scoreboard with output produced by the candidate would instead make the candidate its own
oracle before cutover. Both choices conflict with the official suite's existing role as an
immutable regression ratchet.

## Decision

WU6 authenticates the committed official-suite scoreboard bytes, but applies a conservative
benchmark-specific projection of its ratchet instead of treating every old typokat output as
required behavior.

For an in-scope or unsupported scoreboard row, the candidate must retain the frozen prior typokat
diagnostic multiset, including multiplicity. A separately authenticated accepted-progress multiset
may name occurrences that are allowed to disappear. The initial multiset contains exactly one
occurrence: `conformance/classes/members/classTypes/indexersInClassType.ts` at `15:2339`. The
contract freezes that occurrence before the next live run and the pinned TypeScript-Go exact-flags
row independently supports it. No other occurrence may disappear, and no new occurrence is
accepted. An in-scope row may not become incomplete. An unsupported row must retain the frozen
incomplete multiset, including multiplicity; additional precise incomplete identities remain an
explicit progress signal rather than disappearing from evidence.

This projection is deliberately stricter than official-suite `--check`. The ordinary ratchet may
accept newly matched diagnostics and an `OOS:unsupported` to `IN` transition as progress after
comparison with its TypeScript baseline. The benchmark accepts neither from a live candidate.
Those changes require independent review and a frozen authority update before timing.

The 42 parse-error and four unresolved rows remain class-only authorities. They normalize exactly
to `parse-error` or `unresolved` and carry no unauthenticated diagnostic or surface detail. All
other unsupported rows retain sorted precise incomplete surface identities, including
multiplicity. The runner may not collapse them to a class token.

Every timed sample for one tool must still produce byte-equivalent normalized semantic rows. Raw
rows and the named accepted-progress projection remain in the panel evidence so an independent
reviewer can recompute the verdict. TypeScript-Go continues to reproduce its separate pinned
exact-flags oracle exactly; this decision does not turn the 687-frame panel into a cross-tool
semantic-equality claim.

## Consequences

- WU6 can accept one pre-authenticated false-positive removal without weakening false-negative
  detection.
- A candidate cannot hide a dropped matched diagnostic or incomplete surface behind a new baseline.
- Candidate output never becomes its own pre-cutover oracle.
- Benchmark-specific normalization must know the four class-only unresolved case identities; all
  ordinary incomplete outcomes retain their stable surface IDs.
- A future reviewed authority change may widen the accepted-progress multiset. The benchmark does
  not infer that authority from a live run.

## Alternatives considered

**Require exact reproduction of the old typokat output.** Rejected because it makes a known false
positive a target-model requirement.

**Rebaseline the committed scoreboard from the project-local runner before cutover.** Rejected
because the runner under test would produce its own authority and the ordinary production route
has not switched yet.

**Compare all typokat rows directly with TypeScript-Go.** Rejected because typokat has documented
coverage gaps and the pinned TypeScript-Go version has legitimate semantic differences from the
TypeScript 6.0.3 truth used elsewhere in the project.
