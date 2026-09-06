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

> Four code-adjacent docs stay **outside** `docs/`: [`tests/cases/README.md`](../tests/cases/README.md)
> (how to write/read the conformance fixtures — marker conventions, **not** the divergence ledger,
> which lives in [`reference/divergences.md`](reference/divergences.md)),
> [`tooling/official-suite/README.md`](../tooling/official-suite/README.md) (the official-suite
> harness) and [`tooling/differential/README.md`](../tooling/differential/README.md) (the randomized
> differential harness), plus [`tooling/verify/README.md`](../tooling/verify/README.md) (the explicit
> same-tree gate verifier). The public [`README.md`](../README.md) and the hard-rules
> [`CLAUDE.md`](../CLAUDE.md) live at the repo root.

## Active sprints

The active `as const` sprint targets bounded feature delivery and regression safety, not complete
TypeScript compatibility. General constraint construction and remaining reference-authentication
infrastructure are tracked by [120](backlog/120-semantic-constraint-construction-order.md).

<!-- list the sprint files currently in sprints/ ; empty between sprints -->
- [`sprint-2026-08-17-as-const.md`](sprints/sprint-2026-08-17-as-const.md) — autonomous completion
  through immutable literal modes ([ADR-0031](decisions/0031-preserve-literal-widening-in-immutable-type-identity.md))
  and source-owned assignment work ([ADR-0032](decisions/0032-own-semantic-assignment-work-by-source-and-query-environment.md)); implementation remains uncommitted.
  Callable construction is a proposed dependency, not an approved source cutover
  ([ADR-0034](decisions/0034-own-callable-construction-before-publishing-signatures.md)).

## What's hot

<!-- hand-maintained, keep short: the few things actually in motion + what's next.
     If everything is "hot", nothing is. -->
- **Project-local library semantics shipped** — the archived
  [`project-local universe sprint`](archive/sprint-2026-08-22-project-local-library-universe.md)
  cut every production entry point over to one `CheckerSession`. Processes share only one immutable,
  AST-free declaration product; each project owns complete library/user binding, semantic
  publication, caches, diagnostics, and identity. [`ADR-0023`](decisions/0023-own-default-library-semantics-per-project.md)
  records the accepted boundary and deletes the frozen/sparse/complete-source routing split.
- **Declaration inventory shipped; B44 unblocked** — the archived
  [`declaration-inventory sprint`](archive/sprint-2026-08-18-canonical-declaration-inventory.md)
  cut declaration reservation, publication, and class attachment over to the authenticated
  inventory, removed the parallel declaration walkers, and closed backlog 84.
  [`ADR-0022`](decisions/0022-use-one-binder-authenticated-declaration-inventory.md)
  freezes the one-pass manifest, exact binder join, module-local dense map, and AST-free frozen
  product boundary. The archived
  [`satisfies sprint`](archive/sprint-2026-08-17-satisfies.md) implements source-preserving
  contextual checking and records the comparison with TypeScript PR #46827. The
  [`as const` sprint](sprints/sprint-2026-08-17-as-const.md) now migrates literal producers and
  assignment flow after independent reviews exposed provenance loss in the uncommitted WU1 port.
- **Library compiler decomposition shipped 2026-08-21** — the
  [`archived sprint`](archive/sprint-2026-08-15-library-compiler-decomposition.md) split the
  14,009-line root into a 176-line stable facade, thirteen bounded production owners, and one test
  child without behavior or public-path movement. Full shared/cold parity reported
  `580/696/580/580`; the official-suite scoreboard did not move.
- **CLI and frontend ownership cleanup shipped 2026-08-21** — the
  [`archived sprint`](archive/sprint-2026-08-21-cli-frontend-owned-seams.md) centralized the four
  CLI report channels and status decision behind one private transaction, and made frontend module
  accounting one consuming, owned, single-dispatch authority. Backlogs 112 and 118 are closed.
- **Development feedback sprint shipped 2026-08-21** — the
  [`archived sprint`](archive/sprint-2026-08-21-development-feedback-loop.md) added exact
  marker-aware case/corpus selection, a bounded same-tree gate verifier, and authenticated reuse of
  one release artifact across pull-request CI consumers. Backlogs 117 and 120 are closed.
- **Checker-scaling sprint archived partial 2026-08-14** — the dated
  [`scaling sprint`](archive/sprint-2026-07-25-checker-scaling.md) shipped its measured nonlinear
  fixes, but never recorded a final current-HEAD benchmark sweep or whole-sprint review. Its
  residual scaling gate, flat per-file cost, and memo soundness surfaces remain in backlogs
  [`89`](backlog/89-scaling-guards-for-project-state.md),
  [`94`](backlog/94-flat-per-file-regression-since-july-9.md), and
  [`106`](backlog/106-contextual-memo-residual-key-surfaces.md).
- **Enum type/value model shipped 2026-08-13** — the archived
  [`enum sprint`](archive/sprint-2026-08-12-enum-type-value-model.md) supplies nominal member
  identity, relation and flow behavior, exact merge recovery, namespace/Bundler publication, and
  an audited official-suite ratchet. The immutable
  [`placetext re-screen`](archive/real-project-rescreen-2026-08-13/README.md) removed every old
  enum/module identity and exposed computed-key and predicate-narrowing cascades; backlog
  [`72`](backlog/72-real-project-preview-readiness.md) remains open at the unchanged zero threshold.
- **Optional-parameter argument acceptance shipped 2026-08-11** — the archived
  [`optional-parameter sprint`](archive/sprint-2026-08-11-optional-parameter-undefined-arguments.md)
  admits direct `undefined` and matching undefined-bearing values at optional/defaulted fixed call
  slots without rewriting stored parameter types. The immutable
  [`placetext re-screen`](archive/real-project-rescreen-2026-08-11/README.md) removed the revealed
  `TK2345` with unchanged 9-root/13-resolution accounting and no new identity. Backlog
  [`72`](backlog/72-real-project-preview-readiness.md) remains open at 7 incomplete records and 5
  diagnostics. Review byproducts are [`110`](backlog/110-contextual-return-literal-inference.md)
  and [`111`](backlog/111-structural-inference-literal-provenance.md).
- **The backlog-14 closure audit graduated independent semantic tails** — `globalThis` diagnostic
  fidelity is owned by [`48`](backlog/48-no-implicit-any.md), flow evaluation order by
  [`51`](backlog/51-narrowing-tail.md), construct-only call code parity by
  [`75`](backlog/75-scope-surface-tail.md), apparent built-in object surfaces by
  [`107`](backlog/107-apparent-built-in-surfaces.md), and generic arrow descriptors by
  [`108`](backlog/108-generic-arrow-signature-descriptors.md). None is a default-library routing
  condition.
- **Default-library lifecycle history is closed** —
  [`ADR-0017`](decisions/0017-compile-the-default-library-from-source.md) and
  [`ADR-0022`](decisions/0022-use-one-binder-authenticated-declaration-inventory.md) remain the
  exact source/profile and declaration-inventory authorities.
  [`ADR-0023`](decisions/0023-own-default-library-semantics-per-project.md) supersedes the old
  frozen semantic base, sparse collision epochs, private replay, and split standalone lifecycle.
  [`ADR-0018`](decisions/0018-pin-library-owned-records-as-a-named-census.md) still pins the
  production-owned library outcome census; the accepted WU7 baseline contains 703 named records
  (318 diagnostics and 385 incompletes). The earlier
  [`Stage-1 closure`](archive/sprint-2026-08-02-default-library-cutover-closure.md) and
  [`performance cutover record`](archive/sprint-2026-07-21-full-lib-performance-cutover.md)
  remain historical evidence, not current architecture.
- **Namespaces/declaration merging shipped 2026-07-16** (archived:
  [`archive/sprint-2026-07-15-namespaces-declaration-merging.md`](archive/sprint-2026-07-15-namespaces-declaration-merging.md)) —
  ordered groups, qualified types, keep-pairs, legal global type publication, and immutable
  standalone instantiated namespace values are complete. The pinned ES5 proof is GO and backlog
  `14` passed its namespace model prerequisite; production delivery is archived under its closure
  sprint; `15`, `63`, `76`, and `82` retain their explicit non-namespace tails.
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
  The shipped `check --project-summary json <directory|tsconfig.json>` route accepts the exact
  files-only strict/noEmit/ESNext/Bundler config, configured local `.ts` roots, and named plus
  regular direct default imports; `oxc_resolver 11.24.2` handles extensionless and `.js`→`.ts`
  lookup. The archived
  [`source-re-export sprint`](archive/sprint-2026-08-08-acyclic-source-reexports.md) also ships
  acyclic local named source re-exports over namespace-free value/type slots. The archived
  [`default-slot sprint`](archive/sprint-2026-08-08-default-module-slots.md) ships direct default
  classes/functions, expressions, namespace-free identifiers, and direct/type-only imports through
  a slot separate from named exports. Every unsupported form is an explicit non-clean notice.
  Packages, default bridges/re-exports, mixed and namespace imports, star/namespace re-exports,
  namespace-bearing producers/targets, cycles, NodeNext, and other host profiles remain deferred.
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
- **Bundler project tracer terminated incomplete after shipping its substrate** — the first
  witness-first preview is [`archived incomplete`](archive/sprint-2026-07-12-real-project-preview.md).
  Its [`replacement`](archive/sprint-2026-08-07-bundler-project-tracer.md) shipped local project
  discovery, `oxc_resolver` lookup, complete module accounting, and the deterministic public
  summary, then stopped after six candidates failed the unchanged zero threshold before mutations.
  Backlog [`72`](backlog/72-real-project-preview-readiness.md) retains the witness, mutation, ratchet,
  and CI promise. The archived
  [`acyclic source-re-export sprint`](archive/sprint-2026-08-08-acyclic-source-reexports.md) shipped
  the first bounded slice of [`15`](backlog/15-modules-imports.md). The subsequent
  [`re-screen sprint`](archive/sprint-2026-08-08-real-project-rescreen.md) terminated incomplete at
  WU0 after all six immutable candidates still failed the unchanged zero threshold. No witness
  tooling or production broadening started. The archived
  [`default-slot sprint`](archive/sprint-2026-08-08-default-module-slots.md) removed `placetext`'s
  historical default-export route blocker. The archived
  [`object-binding sprint`](archive/sprint-2026-08-09-object-binding-publication.md) then removed all
  22 unpublished-leaf diagnostics measured by the
  [`pre-change screen`](archive/real-project-rescreen-2026-08-09/README.md). The immutable
  [`post-binding screen`](archive/real-project-rescreen-2026-08-10/README.md) exposed the optional-
  parameter argument bug. The archived
  [`optional-parameter sprint`](archive/sprint-2026-08-11-optional-parameter-undefined-arguments.md)
  removed it; the [`latest screen`](archive/real-project-rescreen-2026-08-11/README.md) still stops
  before witness work on independent enum, computed-key, template-interpolation, and predicate-
  bearing array surfaces. Target/library meaning equivalence also remains unproved. Package breadth
  remains later work under `15`.
- **Surface-accounting tail shipped 2026-07-12** (archived:
  [`archive/sprint-2026-07-12-surface-accounting-tail.md`](archive/sprint-2026-07-12-surface-accounting-tail.md)) —
  all inventoried expression shapes now report incomplete explicitly or have a durable semantic
  owner; three review/audit failures were remediated before PASS. Backlog `73` is closed, but this
  did not produce the qualifying public witness required by `72`; the bounded resolver and acyclic
  named source-re-export surfaces are now shipped, while the witness gate remains open.
- **JS-exact number stringification (`30`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-js-number-stringification.md`](archive/sprint-2026-07-11-js-number-stringification.md)) —
  numeric literal holes now use ECMA-exact formatting; `${number}` keeps its
  tsc-clean long-decimal acceptance; independent boundary review PASS.
- **Minimal ambient prelude (`38`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-minimal-ambient-prelude.md`](archive/sprint-2026-07-11-minimal-ambient-prelude.md)) —
  the historical bounded `console`/numeric-`Math` surface established the canonical value/type
  handoff, with 26 audited `OOS:unresolved → IN` transitions. Production now uses the full pinned
  default library; the retained corpus guards the handoff and does not authorize a project-specific
  ambient expansion.
- **Silent-FN quick wins sprint shipped partially 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-silent-fn-quick-wins.md`](archive/sprint-2026-07-11-silent-fn-quick-wins.md)) —
  `67` shipped with independent review PASS; `66` hit the `63(d)` protected-lineage
  architecture stop gate and remains open; review-discovered call-signature infer gap is `77`.
- **Declaration hoisting parity (`74`) shipped 2026-07-11** (archived:
  [`archive/sprint-2026-07-11-declaration-hoisting-parity.md`](archive/sprint-2026-07-11-declaration-hoisting-parity.md)) —
  forward ordinary/generic/overload calls now see stable callable surfaces; `var`
  binds to its function/module owner while initializer and flow timing stay lexical/source ordered.
  `72` was subsequently screened and paused; the full default-library step is shipped and archived
  after final WU7 PASS and exact-`d1aa6d4` remote CI.
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
  readiness proof: type/value publication is GO, and `14` is shipped and archived under its
  [`closure sprint`](archive/sprint-2026-08-02-default-library-cutover-closure.md); `70` this-parameter typing
  subsequently shipped. Backlog `38` is GO
  (ADR-0003). The post-sprint MVP audit added executable
  scope/unsupported censuses (`73`/`75`) and the first honest pinned-project preview gate
  (`72`). **Now:** `14` is closed; the bounded Bundler CLI/accounting and flat object-binding
  publication are shipped, while `72` still lacks its public witness and `15` retains module
  breadth. The remaining
  model-completeness items stay open.
- **Cross-cutting soundness review + fix sprint shipped 2026-07-07.** Four adversarial
  reviewers (relate/CFG/evaluator/M29+M30) confirmed the §6.3 relation-cache and loop-fixpoint
  invariants CLEAN and filed `53`–`65`; the five HIGH silent-FN families then shipped through
  the full dev-method loop (sprint archived — `53` CFG assignment loss, `55` template memo
  poisoning, `57` Tuple↔Array inference, `58` project scope-key collision, `61` class field
  initializers, all five reviews PASS). The follow-up quick-wins sprint then closed `64` `34`
  `33` `54` `59`; the dedicated inference-policy sprint then closed `65`. The current C
  known-gap set is `60` `62` `32` `21` `66` `71` `78`.
