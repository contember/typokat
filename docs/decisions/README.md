# decisions (ADR)

One file per significant architectural/product decision: `NNNN-<slug>.md`
(monotonic, never reused). Copy [`_template.md`](_template.md).

**Immutable.** Once a decision is Accepted, don't rewrite it — to change course,
write a *new* ADR and set the old one's status to `Superseded by NNNN`.
Use that status only when the old decision is superseded wholesale. A new ADR that narrowly
supersedes one named boundary states that scope textually; both ADRs remain Accepted and all
unaffected rules in the older ADR stay binding.

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
- [`0006`](0006-immutable-class-instances-and-scc-publication.md) — immutable class applications with SCC publication, bounded normalization, and typed exhaustion — accepted (2026-07-13)
- [`0007`](0007-bundler-resolution-via-oxc-resolver.md) — Bundler is the 1.0 resolution profile, delegated to `oxc_resolver` — accepted (2026-07-13)
- [`0008`](0008-class-surface-lowering-and-lexical-event-ownership.md) — class surface lowering and lexical event ownership cut over atomically — accepted (2026-07-14)
- [`0009`](0009-ordered-declaration-groups-and-namespace-publication.md) — ordered declaration groups and namespace surfaces publish atomically — accepted (2026-07-15)
- [`0010`](0010-publish-instantiated-standalone-namespace-values.md) — instantiated standalone namespace values publish as immutable structural objects — accepted (2026-07-16)
- [`0011`](0011-freeze-pinned-default-library-base.md) — freeze the pinned TypeScript 6.0.3 default library as a shared semantic base — partially superseded by 0023; pinned profile and 0018 census contracts retained (2026-07-16)
- [`0012`](0012-ship-the-canonical-default-library-snapshot.md) — ship the canonical default-library semantic snapshot as ADR-0011's startup realization — superseded by 0017 (2026-07-22)
- [`0013`](0013-replay-private-library-collision-closures.md) — replay affected owners in private collision universes — superseded by 0023 (2026-07-23)
- [`0014`](0014-authenticate-private-replay-prefixes-at-separate-boundaries.md) — authenticate private replay prefixes at separate boundaries — superseded by 0023 (2026-07-23)
- [`0015`](0015-make-private-collisions-snapshot-native.md) — make private collisions snapshot-native publication epochs with sparse binder-prefix replacement — superseded by 0017 (2026-07-23)
- [`0016`](0016-reason-free-relation-probes.md) — suppress the reason chain on relation probes whose caller discards it — accepted (2026-07-25; implemented by `665c911`)
- [`0017`](0017-compile-the-default-library-from-source.md) — compile the default library from source in every process — partially superseded by 0023; source, profile, and no-artifact contracts retained (2026-07-26)
- [`0018`](0018-pin-library-owned-records-as-a-named-census.md) — retain no library-owned record at runtime; pin the full set as a named `(code, site)` suite census — accepted (2026-07-27)
- [`0019`](0019-split-the-checker-into-layered-workspace-crates.md) — split the checker into a root facade over ten acyclic workspace layers — accepted (2026-07-28)
- [`0020`](0020-build-source-native-sparse-collision-epochs.md) — build source-native sparse collision epochs over the frozen library base — superseded by 0023 (2026-07-28)
- [`0021`](0021-use-complete-source-compilation-for-standalone-cli-checks.md) — use complete-source compilation for standalone CLI checks — partially superseded by 0023; parsing and report-order contracts retained (2026-08-05)
- [`0022`](0022-use-one-binder-authenticated-declaration-inventory.md) — use one binder-authenticated declaration inventory — accepted (2026-08-20)
- [`0023`](0023-own-default-library-semantics-per-project.md) — own default-library semantics per project — accepted (2026-08-22)
- [`0024`](0024-represent-recursive-aliases-as-immutable-applications.md) — represent recursive aliases as immutable applications — accepted (2026-08-26)
- [`0025`](0025-rebaseline-library-records-after-immutable-aliases.md) — rebaseline the named library-record census after immutable recursive aliases — superseded by 0026 (2026-08-27)
- [`0026`](0026-retain-library-outcomes-after-alias-evaluation.md) — retain named library outcomes after complete recursive-alias evaluation — accepted (2026-08-27)
- [`0027`](0027-produce-library-census-from-the-project-publisher.md) — produce the library census from the project-local production publisher — superseded by 0028 (2026-08-27)
- [`0028`](0028-rebaseline-the-production-owned-library-census.md) — rebaseline the production-owned library census — accepted (2026-08-28)
- [`0029`](0029-gate-the-library-record-census-behind-an-opt-in-feature.md) — gate the library record census behind an opt-in feature — accepted (2026-08-28)
- [`0030`](0030-treat-the-wu6-typokat-oracle-as-a-regression-ratchet.md) — treat the WU6 typokat oracle as a regression ratchet — accepted (2026-08-29)
- [`0031`](0031-preserve-literal-widening-in-immutable-type-identity.md) — preserve literal widening in immutable type identity — accepted (2026-09-06)
- [`0032`](0032-own-semantic-assignment-work-by-source-and-query-environment.md) — own semantic assignment work by source and query environment — accepted (2026-09-06)
- [`0033`](0033-separate-contextual-row-views-from-symbolic-signatures.md) — separate contextual row views from symbolic signatures — accepted (2026-09-06)
- [`0034`](0034-own-callable-construction-before-publishing-signatures.md) — own callable construction before publishing signatures — proposed (2026-09-06)
- [`0035`](0035-finalize-literal-types-at-publication-owners.md) — finalize complete literal identities at role-specific publication boundaries — accepted (2026-09-06)
- [`0036`](0036-bound-contextual-class-publication-migration.md) — narrowly supersede ADR-0035's class migration closure scope; retain bounded field const support and defer general contextual class preparation — accepted (2026-09-06)
