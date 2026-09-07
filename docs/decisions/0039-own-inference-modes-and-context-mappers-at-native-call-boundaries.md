---
id: 0039
title: Own inference modes and context mappers at native call boundaries
status: accepted
date: 2026-09-07
---

# 0039 — Own inference modes and context mappers at native call boundaries

## Context

The rejected R1c implementation preserved single scalar candidates but changed the inputs of outer
multi-candidate calls. Collecting contextual-parameter observations through effects and memo values
did not supply the missing inference phases. Restoring raw callback returns then discarded legitimate
context-sensitive nested-call results. More shell-specific repairs would preserve the ownership split.

Strict TypeScript 6.0.3 distinguishes a direct literal callback return from a context-sensitive
`id`/factory call. It also distinguishes independent annotated callback inputs from omitted parameters.
The final contribution list cannot reconstruct which evidence existed before fixing. A static fixed
bit, one substituted contextual type, and the original return alone are therefore insufficient.

## Decision and owners

Replace the unaccepted R1c observation/memo/composition machinery. Keep immutable literal modes from
[ADR-0031](0031-preserve-literal-widening-in-immutable-type-identity.md) and the publication owners of
[ADR-0035](0035-finalize-literal-types-at-publication-owners.md). This decision changes inference input
preparation, not those publication contracts or the query/relation ownership model.

- `checker/calls.rs` owns independent-input preparation, shell summaries and immutable ordinary and
  expected-result maps. It retains the selected candidate's contextual inputs separately from its
  fully instantiated applicability parameters.
- `infer/mod.rs` owns phase-correct per-binder mode plans, contribution preparation and selection.
- Existing calls/initializer/return routes consume their appropriate contextual view and forward the
  outer inference environment to nested generic calls. There is no observation protocol in effects.

### Evidence and mode preparation

1. Retain the original signature and explicit preview/final-call/class-application request purposes.
   Preserve each candidate's original target, occurrence identity, order and multiplicity.
2. Summarize the existing argument inputs as parameter demands, independent typed callable evidence,
   or deferred/opaque inputs. Admit independent evidence before resolving demands. Read already-walked
   raw types or authenticated parameter declaration slots; do not recheck bodies/defaults or lower
   annotations again. Use original binder IDs, never an inverse map of effective types.
3. Infer expected-result-only bindings **R** independently of ordinary-argument precedence. Build
   ordinary preview bindings **P** from available independent argument inputs, not a direct-arrow
   prefix. Missing evidence is absent, not a manufactured binding to `unknown`.
4. Freeze demanded binders' mode evidence before contextual contributions arrive. Final selection
   still retains later occurrences, including the rest-callback `never`; they cannot retrospectively
   change that frozen mode plan. Unfixed binders finish mode evidence before final selection.
5. Prepare contributions once, before unioning/selection: preserve existing optional-fallback
   precedence, regularize under primitive constraints, otherwise widen W contributions iff all mode
   evidence is top-level and the binder is fixed or not top-level in the retained return. Other
   contributions keep their complete IDs. Regular literals never broaden; `never` remains `never`.

Top-level is binder identity or membership through modeled union/intersection paths, not descent
through properties, tuples, arrays or callables. Unresolved conditionals remain Legacy/opaque:
typokat's raw branch T is not TypeScript's distinct substitution node. Do not build a conditional
type graph inside this classifier. Opaque shells/evidence use Legacy, never guessed fixing or a new
unsupported diagnostic. Keep existing constraint/default completion and coordinator-owned relations.

### Bounded independent callback-input rule

Traverse finite arrow/function/method, static object, array/tuple, parentheses and conditional/logical
value shells in the existing argument-preparation loop, not their bodies. An unannotated supplied
parameter requests context; an annotated/independently typed parameter supplies evidence; an omitted
parameter supplies neither. Combine value-arm evidence before resolving their demands.

Classify an original target per binder as output-only, callback-input-only, mixed or opaque. Only
independent evidence from callback-input-only targets is an upper bound. When selection reaches it,
retain an existing ordinary candidate if the semantic coordinator proves it assignable to that bound.
With no ordinary candidate or incompatible bounds, use baseline selection rather than a new solver.
Mixed/opaque polarity is Legacy. Context-produced parameters are not independent upper bounds.

This intentionally includes a bounded part of backlog 122. It does not introduce general variance,
priorities, post-fix candidate exclusion, suffix scheduling/freezing or mutable lazy fixing mappers.

### Contextual views and nested calls

Keep original callable signature, parameter view, return target and an immutable **P/R environment**:

- Parameter contexts substitute through P. Callback literal/return targets use original types plus R,
  including symbolic leaves in object/array/tuple returns. Primitive bounds shape through existing
  constraint queries, not a preview-value exception. Explicit annotations retain authority.
- Pass bare symbolic return contexts through native expression/return inputs; do not discard them
  solely because they are TypeParams. `PreparedInitializerContext.original_target` keeps its meaning.
- Nested generic expected-result inference maps its target through outer P for ordinary expected
  evidence. Its independently inferred R uses outer R with outer P fallback/composition. Shield inner
  binders; preserve explicit `unknown` versus absence. No raw-return restoration or post-call repair.
- Selected-candidate committed checking reuses the original contextual-input product, not its fully
  substituted applicability return. Explicit type arguments use full substitution; constructs retain
  real returns; ClassApplication keeps baseline policy.

Install the immutable environment with scoped save/restore at native argument/callable boundaries.
Forward it through existing initializer and block-return routes. Optional expected-result preparation
must roll back on failure and publish no partial R. Existing native retry/query savepoints remain the
transaction owners; failure cannot become ready recovery through the new contextual environment.

### Memo and effects

P/R change expression semantics. Add their immutable, ordered binding content to `WalkEnvironment`,
covering raw and contextual memo keys. Equality must compare content, not an unchecked fingerprint,
allocation address or attempt ID. Equal environments can share; different outer mapping authority
cannot share merely because an immediate contextual TypeId matches. Missing context remains distinct
where necessary. Cloning an Arc replaces Copy where needed; this is explicit context, not a cursor graph.

Keep memo values, dependency validation, net-write adoption, budgets and region lifetime at their
baseline owners. No inference facts, tickets or diagnostics enter memo values. Preserve raw-batch
supersession, suppressed-record behavior, lexical tickets and retaining/nonretaining modes.

## Removal and acceptance

Remove only authenticated agent-owned R1c observers, effect facts/drains, cursor projection, pending
memo insertion, raw-function barriers and composition repairs. Preserve accepted publication changes
and unrelated work. Replace facts-only tests explicitly with native mode-plan, original-binder,
environment/rollback and owner tests; retain existing diagnostic and memo read/write assertions.

| Retired mechanism test | Required replacement proof |
|---|---|
| Consumption emission/replay and raw-function barrier | Frozen pre-context mode plans and function/method shell demands |
| Original cursor and recursive fact drain | Original T/U identity, shielding, scoped environment restoration and unchanged record ownership |
| Observing memo separation | P/R content equality, dependency invalidation, net-write adoption and failure/retry isolation |
| Return restoration and rest special exits | Literal versus nested-call/factory/wrapped returns; ordered `[W, never]` preparation |

The prior 34 R1c markers and the replacement's 19 strict-tsc markers remain required. Preserve
conditional `inference_candidate_positions.ts:31`, body `contextual_parameter_fixing.ts:26`, B42
`structural_transport.ts:11`, and generic-policy lines 8/19. Prove `[W, never]` remains ordered but is
prepared under pre-context mode evidence. Pin P/R priority, shielding and literal/nested-call contrasts.

Measure cache counts against the authenticated publication baseline. Its evidence proves scaling PASS,
not exact 6/20/72 counts. Restore old measurement vectors where sharing is actually restored; explain
and separately verify any environment-induced separation. Scaling bounds never change. Run differential,
owner, publication, repro and official gates once on a stable candidate, then independent review.

## Alternatives and consequences

Another shell observer or raw-return patch cannot repair the phase split. Deferring all inference
work leaves demonstrated errors in the publication baseline. This replacement adds an explicit native
context environment and bounded upper-bound handling instead of general fixing parity. The contract
is accepted, not its unimplemented performance or compatibility. General variance/scheduling and
tuple-rest collection remain backlog 122/69; class publication, Store, binder and query engines stay out.
