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

> Two code-adjacent docs stay **outside** `docs/`: [`tests/cases/README.md`](../tests/cases/README.md)
> (how to write/read the conformance fixtures — marker conventions, **not** the divergence ledger,
> which lives in [`reference/divergences.md`](reference/divergences.md)) and
> [`tooling/official-suite/README.md`](../tooling/official-suite/README.md) (the official-suite
> harness). The public [`README.md`](../README.md) and the hard-rules
> [`CLAUDE.md`](../CLAUDE.md) live at the repo root.

## Active sprints

<!-- list the sprint files currently in sprints/ ; empty between sprints -->
- [`sprint-2026-07-12-real-project-preview.md`](sprints/sprint-2026-07-12-real-project-preview.md) —
  paused at WU0's zero-threshold witness gate after public candidate screening.
- [`sprint-2026-07-13-semantic-duplication-layering.md`](sprints/sprint-2026-07-13-semantic-duplication-layering.md) —
  active bounded deduplication and measurement-gated repeated-work hardening.

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **Semantic duplication/layering sprint active 2026-07-13** — WU1a's immutable class application
  representation is complete; reviewed fixture commit `889cc19` specifies the remaining class
  surface behavior. [`ADR-0008`](decisions/0008-class-surface-lowering-and-lexical-event-ownership.md)
  requires WU1b-d to land as one serialized atomic production cutover containing compilation-wide
  reservation, capability-isolated lowering, lexical `EventStore` ownership, SCC publication,
  bounded typed projection, every consumer, and legacy deletion. The immutable `ClassInstance`/
  deferred-indexed representation from
  [`ADR-0006`](decisions/0006-immutable-class-instances-and-scc-publication.md) remains binding.
  Neutral test/parameter/prelude
  seams, call/construct selector parity, lazy conditional-infer work, and measurement-only mapped/
  constraint follow-up retain their explicit cache/reuse stop gates.
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
  `InferRewrite`, `InferenceConstraintEvaluator`, and `MappedRewrite` now use separate
  iterative walkers with their distinct cycle/memo/exhaustion policies. WU8 also corrects
  mapped generic constraint/default metadata; its 10,005-deep public-evaluator witness,
  independent reviews, and zero-regression official-suite ratchet all passed.
- **Generic methods (`41`) shipped 2026-07-12** (archived:
  [`archive/sprint-2026-07-12-generic-methods.md`](archive/sprint-2026-07-12-generic-methods.md)) —
  persistent generic method/call/construct binders now survive substitution and relate under a
  cache-safe local alignment. The remaining direct model/lib blocker is `43` namespaces/merging;
  explicit `this`/`ThisType<T>` shipped with `70`.
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
  `72` was subsequently screened and paused; the next recommended model/lib work is `43`.
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
  ([`backlog/lib-audit-6.0.3.md`](backlog/lib-audit-6.0.3.md)) naming the es5
  blocker: `43` namespaces/merging; `70` this-parameter typing subsequently shipped. Backlog `38` is GO
  (ADR-0003). The post-sprint MVP audit added executable
  scope/unsupported censuses (`73`/`75`) and the first honest pinned-project preview gate
  (`72`). **Now:** `72` remains paused at its witness gate; track A
  (`43` next) continues toward full `lib.d.ts`.
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). The follow-up quick-wins sprint then closed `64` `34`
  `33` `54` `59`; the dedicated inference-policy sprint then closed `65`. The current C
  known-gap set is `60` `62` `32` `21` `66` `71` `78`.
