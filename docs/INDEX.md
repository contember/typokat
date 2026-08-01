# typokat docs — index

The map of everything under `docs/`. Read [`CLAUDE.md`](CLAUDE.md) for the rules.
When sources disagree, precedence is: invariants/hard-rules → active sprint →
decisions → reference → archive.

## Folders

- [`reference/`](reference/README.md) — how the system works now (architecture, dev method, invariants, scope map, `tsc` divergences).
- [`ideas/`](ideas/README.md) — proposals, no commitment.
- [`decisions/`](decisions/README.md) — ADRs (the *why*), immutable.
- [`backlog/`](backlog/README.md) — decided work, not yet scheduled (the roadmap).
- [`sprints/`](sprints/README.md) — active thematic work-plans.
- [`archive/`](archive/README.md) — shipped sprints + reference-worthy records.

> Three code-adjacent docs stay **outside** `docs/`: [`tests/cases/README.md`](../tests/cases/README.md)
> (how to write/read the conformance fixtures — marker conventions, **not** the divergence ledger,
> which lives in [`reference/divergences.md`](reference/divergences.md)),
> [`tooling/official-suite/README.md`](../tooling/official-suite/README.md) (the official-suite
> harness) and [`tooling/differential/README.md`](../tooling/differential/README.md) (the randomized
> differential harness). The public [`README.md`](../README.md) and the hard-rules
> [`CLAUDE.md`](../CLAUDE.md) live at the repo root.

## Active sprints

<!-- list the sprint files currently in sprints/ ; empty between sprints -->
- [`sprint-2026-07-25-checker-scaling.md`](sprints/sprint-2026-07-25-checker-scaling.md) —
  active: remove the five quadratic/exponential terms that lose `modules` (665×), `generics` (17.7×)
  and `flow` (3.9×) to native TypeScript 7, and leave guards so the class cannot land silently again.
- [`sprint-2026-08-02-default-library-cutover-closure.md`](sprints/sprint-2026-08-02-default-library-cutover-closure.md) —
  active backlog `14` closure sprint: bounded freeze-boundary decision, the remaining nine
  conformance differences, atomic production cutover, cross-tool gates, and authoritative
  full-library performance evidence.
- [`sprint-2026-07-12-real-project-preview.md`](sprints/sprint-2026-07-12-real-project-preview.md) —
  paused at WU0's zero-threshold witness gate after public candidate screening.
- [`sprint-2026-07-16-namespace-binder-refactor.md`](sprints/sprint-2026-07-16-namespace-binder-refactor.md) —
  planned behavior-preserving cleanup, not started; gated on the full-lib performance sprint
  closing, refs re-verified 2026-07-22.

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **Full default-library performance cutover is active** —
  [`ADR-0011`](decisions/0011-freeze-pinned-default-library-base.md) accepts an exact embedded
  TypeScript 6.0.3 ES2025 full-host profile, one AST-free frozen library base with private deltas,
  and a same-pipeline private universe for global collisions;
  [`ADR-0013`](decisions/0013-replay-private-library-collision-closures.md) seeds that universe in a
  fresh private run and replays the affected-owner closure, and
  [`ADR-0014`](decisions/0014-authenticate-private-replay-prefixes-at-separate-boundaries.md)
  reaches a library-only continuable binder checkpoint before any user row exists.
  [`ADR-0017`](decisions/0017-compile-the-default-library-from-source.md) **retires the shipped
  snapshot**: the library is compiled from its 82 vendored sources in every process, which
  supersedes `ADR-0012` and `ADR-0015` wholesale and narrowly supersedes 0013's decode-seeding and
  0014's digest authentication. The measurement behind it: typokat's cold parse+bind+check of the
  pinned library was at parity with native TypeScript 7 and the rest of the old 1.85 s was work that
  existed only to fill the archive. That in-process figure (277 ms against 289 ms) is **retracted as
  a comparison** — it was not end-to-end. The production-shaped CLI reads **260 ms against the
  comparator's 289.6 ms, 1.12–1.14×**; WU8 is the authoritative gate. The earlier
  [`archived feasibility sprint`](archive/sprint-2026-07-16-full-lib-loading.md) removed the
  first substitution and rendering barriers.
  [`ADR-0020`](decisions/0020-build-source-native-sparse-collision-epochs.md) resolves the remaining
  collision-path conflict: a private epoch may share proven-unaffected immutable rows from that
  source-compiled base, while every affected meaning, identity, event, cache, and suffix stays
  private and republishes through the ordinary checker.
  [`ADR-0018`](decisions/0018-pin-library-owned-records-as-a-named-census.md) settles what happens
  to the 875 records the library reports against itself: no process retains one, and the suite pins
  the complete set as a named `(code, site)` multiset
  (`tests/fixtures/library-owned-records.txt`), which narrows ADR-0011's "preserved exactly" to the
  pinned suite rather than the published base. The
  [`active sprint`](sprints/sprint-2026-08-02-default-library-cutover-closure.md) owns the bounded
  freeze-boundary decision, production Stage-1 cutover, collision/fanout correctness, and the
  cross-tool gate against native TypeScript 7. The
  [`superseded 2026-07-21 sprint`](archive/sprint-2026-07-21-full-lib-performance-cutover.md)
  remains the incomplete run and measurement record. Stage 1 is not yet shipped and
  `crates/typokat-check/src/prelude.ts` remains production at HEAD until the atomic cutover.
- **Namespaces/declaration merging shipped 2026-07-16** (archived:
  [`archive/sprint-2026-07-15-namespaces-declaration-merging.md`](archive/sprint-2026-07-15-namespaces-declaration-merging.md)) —
  ordered groups, qualified types, keep-pairs, legal global type publication, and immutable
  standalone instantiated namespace values are complete. The pinned ES5 proof is GO and backlog
  `14` passed its namespace model prerequisite and is active under the performance-cutover sprint;
  `15`, `63`, `76`, and `82` retain their explicit non-namespace tails.
- **Semantic duplication/layering shipped 2026-07-14** (archived:
  [`archive/sprint-2026-07-13-semantic-duplication-layering.md`](archive/sprint-2026-07-13-semantic-duplication-layering.md)) —
  immutable complete `ClassInstance` applications, atomic declaration-SCC publication, retained
  callable rows, lexical four-key `EventStore` ownership, and the sole transactional
  `SemanticQueryCoordinator` are now the production architecture. Neutral parser/parameter/prelude/
  selector seams were shared without speculative-effect reuse. The conditional-`infer`
  optimization failed its `TypeId`/diagnostic-identity equivalence gate and did not ship; 10k/100k
  mapped/constraint measurements selected no further optimization or backlog item. Independent
  reviews and the zero-regression official-suite ratchet passed.
- **Bundler resolution is the 1.0 profile** —
  [`ADR-0007`](decisions/0007-bundler-resolution-via-oxc-resolver.md) delegates physical
  package/filesystem/tsconfig resolution to `oxc_resolver`; typokat retains project enumeration,
  module-graph and import/export semantics, `.d.ts` checking, diagnostics, and determinism.
  NodeNext and other host profiles are deferred rather than approximated.
- **Pre-lib hardening shipped 2026-07-13** (archived:
  [`archive/sprint-2026-07-12-pre-lib-hardening.md`](archive/sprint-2026-07-12-pre-lib-hardening.md)) —
  aliased construction (`22`), evaluator cycles (`56`), callable-object `ReturnType` inference
  (`77`), explicit `this`/`ThisType<T>` (`70`), and the `13` DEFER/no-VM decision are complete.
  The final official-suite ratchet now uses one shallow exact-revision fetch into a marked
  full-blob Git cache; the 874-test corpus check passed with zero regressions.
- **Rewrite/hotpath hardening shipped 2026-07-13** (archived:
  [`archive/sprint-2026-07-13-rewrite-hotpath-hardening.md`](archive/sprint-2026-07-13-rewrite-hotpath-hardening.md)) —
  `InferRewrite`, the former constraint evaluator, and `MappedRewrite` shipped as separate
  iterative walkers with distinct cycle/memo/exhaustion policies. The semantic-query cutover later
  replaced the private constraint evaluator with coordinator demand. WU8 also corrects
  mapped generic constraint/default metadata; its 10,005-deep public-evaluator witness,
  independent reviews, and zero-regression official-suite ratchet all passed.
- **Generic methods (`41`) shipped 2026-07-12** (archived:
  [`archive/sprint-2026-07-12-generic-methods.md`](archive/sprint-2026-07-12-generic-methods.md)) —
  persistent generic method/call/construct binders now survive substitution and relate under a
  cache-safe local alignment. Namespace value publication subsequently shipped under ADR-0010,
  unblocking `14`; explicit `this`/`ThisType<T>` shipped with `70`.
- **Real-project preview (`72`) paused at WU0** — no screened public project met the multi-file,
  minimal-graph, zero-threshold contract; no implementation or prelude expansion started.
- **Surface-accounting tail shipped 2026-07-12** (archived:
  [`archive/sprint-2026-07-12-surface-accounting-tail.md`](archive/sprint-2026-07-12-surface-accounting-tail.md)) —
  all inventoried expression shapes now report incomplete explicitly or have a durable semantic
  owner; three review/audit failures were remediated before PASS. Backlog `73` is closed, but this
  did not produce the qualifying public witness required by `72`; **do not resume `72` until such
  a witness exists**.
- **JS-exact number stringification (`30`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-js-number-stringification.md`](archive/sprint-2026-07-11-js-number-stringification.md)) —
  numeric literal holes now use ECMA-exact formatting; `${number}` keeps its
  tsc-clean long-decimal acceptance; independent boundary review PASS.
- **Minimal ambient prelude (`38`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-minimal-ambient-prelude.md`](archive/sprint-2026-07-11-minimal-ambient-prelude.md)) —
  the source-backed prelude now supplies a bounded `console`/numeric-`Math` value surface
  through its canonical handoff, with 26 audited `OOS:unresolved → IN` transitions. The shipped
  surface-accounting closure made `72`'s gate measurable; it did not satisfy the public-witness
  gate and does not authorize a project-specific prelude expansion.
- **Silent-FN quick wins sprint shipped partially 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-silent-fn-quick-wins.md`](archive/sprint-2026-07-11-silent-fn-quick-wins.md)) —
  `67` shipped with independent review PASS; `66` hit the `63(d)` protected-lineage
  architecture stop gate and remains open; review-discovered call-signature infer gap is `77`.
- **Declaration hoisting parity (`74`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-declaration-hoisting-parity.md`](archive/sprint-2026-07-11-declaration-hoisting-parity.md)) —
  forward ordinary/generic/overload calls now see stable callable surfaces; `var`
  binds to its function/module owner while initializer and flow timing stay lexical/source ordered.
  `72` was subsequently screened and paused; the now-runnable model/lib step is `14`.
- **2026-07-10 completeness-accounting sprint shipped** (archived:
  [`archive/sprint-2026-07-10-completeness-accounting.md`](archive/sprint-2026-07-10-completeness-accounting.md)) —
  machine-validated OXC surface inventory with compile-time drift tripwires, a first-class
  incomplete outcome (CLI exit `3`, `incomplete[<id>]` records, official-suite `OOS:unsupported`
  with preserved diag diffs), emissions across expressions/statements/annotations/signatures/
  class members, and a structured, validated divergence ledger with deps parity. Backlog `73`
  rescoped to the remaining emission tail; `75` rescoped to the semantic families.
- **M0–M33 shipped; 2026-07-10 soundness-review sprint shipped** (archived) — all ten
  verified review findings fixed (statements/loops/throw, switch-local scope,
  type-only exports, local overloads, any&never, intersection nominal origin, mapped
  recursion guard, keyof domain, annotation depth budget), the official-suite ratchet
  is diagnostic-identity based (re-baselined 345/1674), CI is checked in
  (`.github/workflows/ci.yml`, pinned toolchain), and the 1.0 plan is executable:
  [`backlog/completion-1.0.toml`](backlog/completion-1.0.toml) validated by
  `tests/manifest.rs`, with the pinned TS 6.0.3 lib audit
  ([`backlog/lib-audit-6.0.3.md`](backlog/lib-audit-6.0.3.md)) now backed by the committed namespace
  readiness proof: type/value publication is GO, and `14` is now active under a production
  performance-cutover sprint; `70` this-parameter typing
  subsequently shipped. Backlog `38` is GO
  (ADR-0003). The post-sprint MVP audit added executable
  scope/unsupported censuses (`73`/`75`) and the first honest pinned-project preview gate
  (`72`). **Now:** `72` remains paused at its witness gate; full `lib.d.ts` work (`14`) is active
  under ADR-0017's source-compiled library and its cross-tool gate against native TypeScript 7,
  while the remaining model-completeness items continue independently.
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). The follow-up quick-wins sprint then closed `64` `34`
  `33` `54` `59`; the dedicated inference-policy sprint then closed `65`. The current C
  known-gap set is `60` `62` `32` `21` `66` `71` `78`.
