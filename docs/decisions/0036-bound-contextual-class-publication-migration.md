---
id: 0036
title: Bound the contextual class publication migration
status: accepted
date: 2026-09-06
---

# 0036 — Bound the contextual class publication migration

## Context

[ADR-0035](0035-finalize-literal-types-at-publication-owners.md) requires effective contextual inputs
at literal publication boundaries. The Pass-owned checker can prepare those inputs through its
existing semantic coordinator. Query-free class construction cannot use that capability: ordinary
class and alias query registries are staged after class drafts finish.

Preparing external interface contexts earlier would solve some cases, but not a field whose
context depends on its own class. This source is clean in strict TypeScript 6.0.3, the pre-feature
checker and three frozen feature candidates:

```ts
class ContextOwner {
  tag = 1;
  member = { key: "alpha" } satisfies {
    key: ContextOwner extends { tag: number } ? string : "alpha";
  };
}
let copy = new ContextOwner().member.key;
copy = "beta";
```

Changing the true branch to `"alpha" | "beta"` makes TypeScript reject the final write, while all
four checker versions remain clean. This is an existing contextual-inference gap, not evidence of
correct self-dependent construction. Reading a partial class or changing an already published field
would violate the class publication contract. Blanket rejection would instead make the broad
control newly nonclean.

The user approved narrowing the class migration rather than making a new class construction
schedule a prerequisite for bounded `as const` delivery.

## Decision

This decision narrowly supersedes ADR-0035's requirement to complete contextual class-initializer
publication migration in the current milestone. All other ADR-0035 rules remain binding.

- Complete role-aware publication and effective-context preparation on Pass-owned paths, including
  all existing inferred callable-body consumers. No arrow-only legacy branch follows.
- Keep literal-owned `as const` support in instance fields, its reference boundaries, ordinary
  readonly versus non-widening distinctions, explicit annotations and existing ready-context
  acceptance. Class changes still require independent review and unchanged-tree regression gates.
- Defer general contextual class-initializer preparation to
  [123](../backlog/123-contextual-class-initializer-publication.md). Retained behavior in that region
  is legacy, incomplete contextual semantics, not proof of ADR-0035 readiness.
- Do not add blanket unsupported-context rejection, a class query callback, partial surfaces,
  early publication, post-publication repair or a second evaluator to finish this sprint.
- Do not disguise a raw legacy class context as a successfully prepared Pass context. This scope
  exception does not change the Pass readiness/failure contract or permit newly dropped diagnostics.

Preserve existing positive controls in the active corpus and missing-error witnesses under the
new backlog owner. Do not remove assertions, rebaseline scoreboards wholesale, or accept the current
unreviewed class cutover merely because general class preparation is deferred.

## Consequences and alternatives

Bounded `as const` can finish without claiming complete contextual class inference. Backlog 123
remains release-blocking because its narrow self-dependent witness silently loses a TypeScript
error. Backlog 119 remains a behavior-preserving maintenance extraction; it does not authorize
semantic rescheduling.

External-context preparation is useful when that limited coverage is itself scheduled, but it is
not a closure solution for self-dependent contexts. Full class-dependent preparation is appropriate
when the construction schedule is the intended work; it needs a separate design. Explicit rejection
would be appropriate only under a separately approved compatibility reduction, which this decision
does not grant.

Revisit this boundary when a reviewed class construction contract can supply effective contexts
without violating complete immutable publication. This decision accepts scope, not implementation
or sprint completion.
