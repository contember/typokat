---
id: 73
title: Unsupported AST surface and silent-skip audit
blocked-by: []
---

# 73 — Unsupported AST surface and silent-skip audit

**Summary.** Make AST traversal completeness executable: every expression, statement,
declaration, and type-node form is either checked, deliberately out of scope, or surfaced as
unsupported — never silently discarded through a wildcard/`None`/error-type fallback.

## Problem

Targeted reviews keep finding ordinary child nodes that the checker never visits. The 2026-07-10
review found binary-result inference, template-literal interpolations, array spreads, and
iteration checks (concrete fixes owned by [`71`](./71-expression-inference-fn-tail.md)). Similar
gaps can exist in `infer_expr`, statement/binder dispatch, annotation lowering, declaration
collection, and helpers that skip a child while returning `None` or the internal error type.

A finite list of known cases is not a completeness argument. The error type is valid for cascade
suppression after a reported failure, but it must not turn an unvisited in-scope construct into a
clean verdict.

## Approach / acceptance

Inventory the `oxc` AST variants consumed by the binder/checker and classify every form in a
checked-in, machine-validated surface manifest:

- **supported** — all semantically relevant children are bound and checked, with a focused
  witness;
- **delegated/OOS** — excluded by the diagnostic scope boundary with an explicit reason;
- **unsupported-IN** — not implemented yet, produces a stable unsupported notice and links to a
  live backlog owner; it cannot contribute a permissive value to a clean verdict.

Audit wildcard arms and permissive returns in expression inference, statement traversal,
declaration/member binding and checking, type-annotation lowering, binder prepasses, and flow-graph
construction. Add a validation test that fails when an `oxc` upgrade changes the pinned variant
inventory, when a variant becomes unclassified, or when a classified form loses its witness/owner.
Add adversarial nested-child fixtures so assignments/errors inside templates, spreads, computed
positions, member initializers, loop headers, annotations, and other containers cannot disappear.
Backlog `71` remains the implementation owner for its known binary/template/spread/iteration
families; this item owns the systematic inventory, enforcement mechanism, and graduation of newly
found gaps.

Acceptance: all four dispatch surfaces have complete classifications; no in-scope wildcard path
can silently return `None`/error without first recording a diagnostic or unsupported notice; an
independent review adds fresh nesting probes and finds no unclassified silent child. The
real-project summary in `72` consumes the same unsupported identities. Until this inventory is
complete for every surface reached by a project, a zero-diagnostic run must explicitly say that a
clean verdict is not trustworthy rather than presenting it as success.

## Touch points

`src/binder/`, `src/check/checker/expr.rs`, `src/check/checker/statements.rs`, declaration and
annotation lowering, a checked-in AST-surface manifest/validator, diagnostics/reporting, and
focused conformance fixtures.

<!-- Origin: post-sprint MVP-readiness audit; generalizes WU4 traversal byproducts, 2026-07-10. -->
