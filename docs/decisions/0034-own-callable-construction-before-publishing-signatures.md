---
id: 0034
title: Own callable construction before publishing signatures
status: superseded by 0040
date: 2026-09-06
---

# 0034 — Own callable construction before publishing signatures

## Context

This proposal is deferred to [backlog 120](../backlog/120-semantic-constraint-construction-order.md).
It is not an implementation prerequisite for bounded backlog-44 delivery.

Cyclic generic fallback depends on semantic instantiation order. Canonical structural types erase
some of that order. The accepted intersection experiment preserves ordered immutable expressions,
but TypeScript 6.0.3 union order also depends on when constituent identities are created and reused.
Written operand order, binder ordinal, and the first textual call each fail source witnesses in the
[as-const sprint](../sprints/sprint-2026-08-17-as-const.md).

Copying a function value need not construct its signature. A callable relation can construct it.
The current eager publication boundary loses this distinction. A query-chosen once-cell would make
published meaning depend on inspection order. Whole-body replay would duplicate semantic effects.
Even a declared return can create constituents before constraints; declaration-header validation
can demand another bound early. The first proof must exclude those unproved scheduling dependencies.

## Proposed decision

Separate an immutable registered callable reference from a completed immutable `FunctionType`.
Registration authenticates declaration, source, scope, binder, and callable-owner identities through
the existing declaration inventory. It retains complete AST-free recipes and application identity.
It allocates no semantic construction ranks and does not masquerade as a ready signature.

Signature-independent copy and assignment to `unknown` may transport a safe registered reference.
Identity and top-target success still require the complete registered dependency closure to be
valid and unpoisoned. Registration safety is distinct from shape readiness. Neither preflight nor
display constructs signatures, creates ranks, or grants a source-execution permit.

Only the active source owner completes construction. Its operation-scoped permit cannot be created
by query inspection. A shape-dependent pure query returns one ordered `NeedsSource` request for a
complete application, discards tentative query writes, and releases its borrows. The source owner
then completes the requested work; only the pure query retries on retained operands and the new
publication snapshot. Expressions, annotations, arguments, and effectful retry closures are not
re-executed. Existing lexical tickets continue to own reporting, not semantic construction order.

Construction has private parameter-creation slots, per-bound completion receipts, a return slot,
one descriptor terminal, and a header effect/adoption receipt. Completed work is shared; active
work is not completed. Failure publishes no descriptor. Exactly one owner adopts held effects.
No whole-`Pass` snapshot, mutation of an already published descriptor, or rollback of named IDs is
permitted. Old publication snapshots retain old availability; new snapshots do not replace rows
visible to old queries. Registration-domain and publication-snapshot identities participate in
query context. Pre-ready successes do not enter ordinary durable callable relation caches.

For the first proof, lexical header processing completes each parameter's bound before recording
that parameter's creation. A reference records creation of its parameter without completing that
parameter's bound. Forward signature demand first records all parameters in declaration order,
then completes the same bound receipts and direct return. A later lexical visit adopts the held
header work without lowering it again. This schedule is justified only for the grammar below.

The source owner records relative first-creation ranks for own parameter identities and canonical
mutable arrays of those identities. Array identities are reused across bound slots. These ranks
are private construction state, not mutable type metadata, source-origin flags, or physical arena
indices. Freeze surviving union constituents in rank order into retained immutable expressions.
Complete constraint witnesses and all call-observable descriptor fields participate in identity
before publication. Later projection and inference cannot consult construction ranks.

Reuse the existing declared-expression, materialization, and full/partial inference owners for
retained union/array operations. There is no private union evaluator or replacement inference
machine. Ordinary single-property recursive bounds remain structural types; legal inference
backedges remain distinct from source-construction cycles and resource exhaustion.

### First proof boundary

One source unit; local top-level non-overloaded `declare function` producers with no bodies,
receivers, value parameters, defaults, outer binders, `const` parameters, or variance modifiers.
Authenticate the whole signature before semantic construction. Partition its own parameters by
resolved bound shape, not by names or position:

```text
G       = string | number | boolean | bigint | symbol | null | undefined | unknown | void | never
L-bound = absent | G | { static-key: L }
S-bound = L[] | Union(L, L, ...) | Union(L[], L[], ...)
Return  = own parameter | G
```

The record has exactly one required mutable property. Every bound reference resolves to a leaf
parameter `L`, never a selector `S`; self/mutual record recursion is allowed. Unions have at least
two written operands of one family. Parentheses and finite nested same-family unions preserve
ordered child traversal and sharing. No alias, tuple, intersection, mixed union, bare-parameter
bound, constructed return, or implicit constraint expansion enlarges this proof.

Pre-completion consumers are finite unannotated const-copy chains, const assignment to `unknown`,
uncontextualized zero-argument calls without explicit type arguments, and const callable assignment
to `() => unknown`. Completed call results use ordinary member and assignment checks. These are
proof obligations, not new public unsupported forms or a fixture-selected alternate checker.

Admission is query-free and whole-owner: selected complete plan, outside proof, or invalid
authenticated input. Selection does not lower part of a declaration. A selected failure is not a
canonical fallback trigger. Source and product checks use the same resolved-identity predicate;
a signature recipe alone does not prove declaration ownership or absence of overloads/body.

### Execution gate

This ADR is proposed, not implementation authorization. Before acceptance, specify how the real
source route authenticates both producer and pre-completion consumer eligibility using existing
inventory/traversal owners. A new semantic prescan, fixture flag, alternate checker entry, or
successful-query classifier does not satisfy that requirement. Per-declaration producer eligibility
alone cannot justify silently mishandling a later consumer outside the proof.

The same gate must identify the exact registered-reference representation, publication-snapshot
owner, source-permit handoff, and shared union/array operation seams. Split the proof into model/
query lifecycle and real source execution only after these interfaces are coherent. No source
cutover follows from the accepted intersection experiment or passing oracle programs alone.

## Consequences and acceptance

This changes callable availability, not the immutability of published types. ADR-0005's complete
generic descriptors, ADR-0024's immutable applications, ADR-0008's class/effect boundaries, and
ADR-0032's source-owned work remain binding. Class construction receives no new query or `Pass`
capability. Reference census, hash/equality, raw/derived transport, substitution with own-binder
shielding, and freeze/fork must cover every field of the new reference and snapshot domains.

Direct tests must prove poison/missing closure before identity/top success; no durable promotion
after `NeedsSource`, poison, or exhaustion; one completion/adoption per bound and header; stable
held descriptors and old snapshots; and inspection-first/cold-warm order independence. Use complete
application/context keys with separate active/completed state. The proof limits are 128 parameters,
4,096 unique recipe nodes, and 16,384 ordered edges per signature, with iterative traversal and
typed, owned exhaustion at limit-plus-one. These are experimental limits, not a public policy.
An unrelated clean source must remain clean after each failure.

Source tests distinguish declaration-first, forward-call, copy, unknown-target, and callable-target
construction, plus array order/reuse/duplicates, alias calls, reordered binders, and direct return
parameters. Constructed returns and selector-bound chains remain explicit negative admission
controls and unchanged broader acceptance requirements. Arrows/body demand, general lazy fields,
aliases, overload speculation, members, libraries, and cross-file routes require separate proofs.

Confidence is medium for this bounded design. Reject it if admitted source operations cannot
produce the oracle order through the existing semantic owners exactly once, or if route selection
requires a second checker. Keep the frozen experimental candidate and public route unchanged until
that question is resolved; a failed proof does not justify weakening source fixtures or scoreboards.

## Alternatives considered

- **Retain only instantiation metadata:** best when construction order is already fixed at
  publication. This works for the intersection experiment but cannot supply the missing union order.
- **Migrate general source/signature laziness immediately:** best if an established scheduler already
  owns all affected fields and effects. Here its missing return/bound-demand contracts make the
  larger cutover harder to validate and reverse than the bounded proof.
- **Defer the construction dependency:** best when feature delivery can proceed without exact cyclic
  fallback. It preserves architecture options, but cannot close this sprint's committed witnesses.
- **Bounded source-owned proof:** preferred for testing registration, construction, and publication
  without claiming that the excluded scheduling surface is implemented.
