# decisions (ADR)

One file per significant architectural/product decision: `NNNN-<slug>.md`
(monotonic, never reused). Copy [`_template.md`](_template.md).

**Immutable.** Once a decision is Accepted, don't rewrite it — to change course,
write a *new* ADR and set the old one's status to `Superseded by NNNN`.

Write one when the choice (a) constrains future work, (b) rejected a real
alternative, or (c) someone will later ask "why did we do it this way?". Otherwise
a commit message suffices.

## Log

<!-- newest last; one line each: NNNN — title — status (date) -->
- [`0001`](0001-type-level-vm-is-a-deferred-evaluator-optimization.md) — the bytecode VM is a deferred evaluator optimization, not a planned pillar — accepted (2026-06-25)
- [`0002`](0002-de-bruijn-scoped-to-infer-binders.md) — de Bruijn indices scoped to `infer` binders; declaration type params stay named ids — accepted (2026-07-04)
- [`0003`](0003-backlog-38-minimal-prelude-go.md) — GO on the backlog `38` minimal ambient prelude; scheduled as a later spec-first sprint — accepted (2026-07-10)
- [`0004`](0004-prelude-value-type-handoff.md) — preserve prelude value types through the canonical checker pipeline — accepted (2026-07-11)
- [`0005`](0005-persist-generic-signature-binders.md) — persist generic signature binders in function types — accepted (2026-07-12)
- [`0007`](0007-bundler-resolution-via-oxc-resolver.md) — Bundler is the 1.0 resolution profile, delegated to `oxc_resolver` — accepted (2026-07-13)
