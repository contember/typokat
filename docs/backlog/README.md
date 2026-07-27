# backlog

Decided work items ("issues") not yet scheduled into a sprint. One self-contained
file per item: `NN-<slug>.md` (zero-padded, **folder-local** sequence — don't
renumber, gaps are fine). Copy [`_template.md`](_template.md).

**No `status:` field** — an item is alive because it lives here. It leaves by being
**deleted** on ship (default; git holds the record) or moved to `../archive/` if it
documents something a future reader needs. Dependencies go in frontmatter:
`blocked-by: [./NN-other.md]`.

This **is** the roadmap. Shipped so far: items `01`–`04` (current-impl bugs), `05` (object/interface
signatures, F1), `06`+`20` (class completeness + ctor accessibility), `07` (unstructured-flow
narrowing, M23), `08`–`12` (constraints M24 → utility types M28: the type-level evaluation phase,
complete), `28`–`29` (soundness warm-ups), `31` (M30 contextual literals), backlog `15` slice 1
(M29 local-relative modules), `53` `55` `57` `58` `61` (the five HIGH silent-FN fixes,
sprint-2026-07-07), `25` (M31 intersection types, sprint-2026-07-07), `33` `34`
`54` `59` `64` (soundness-tail quick wins, sprint-2026-07-08), `24` `39`
(M32 signature shape, sprint-2026-07-09), `65` (inference candidate policy,
sprint-2026-07-09), `40` (M33 function overloads,
sprint-2026-07-09), `74` (declaration hoisting parity, sprint-2026-07-11),
`38` (minimal ambient prelude, sprint-2026-07-11), `67` (modeled `ReturnType`
constraint, sprint-2026-07-11), and `30` (JS-exact number stringification,
sprint-2026-07-11), and `41` + `23` (persistent generic method/call/construct signatures,
including the static generic binder path, sprint-2026-07-12), and `22` (parenthesized and
non-generic class-value construction facts, sprint-2026-07-12), and `56` (cycle-aware
evaluator memoization + `TK2589`, sprint-2026-07-12), and `77` (callable-object
`ReturnType` extraction, sprint-2026-07-12), and `70` (explicit receiver slots,
contextual `ThisType<T>`, and receiver utilities, sprint-2026-07-12), and `13` (the ADR-0001
profiling gate, strict DEFER/no-VM, WU5 of
[`sprint-2026-07-12-pre-lib-hardening.md`](../archive/sprint-2026-07-12-pre-lib-hardening.md),
closed 2026-07-13), and `43` (namespace/declaration-space completion,
[`sprint-2026-07-15-namespaces-declaration-merging.md`](../archive/sprint-2026-07-15-namespaces-declaration-merging.md)). The earlier shipped items are in
[`../archive/`](../archive/README.md), including the completed WU6 official-suite ratchet.
Architecture §12 governs
phase ordering; the bytecode VM stays a deferred, profiling-gated refactor
([ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)). How each item
is built: [`../reference/dev-method.md`](../reference/dev-method.md).

## Definition of done (checker 1.0)

The **executable source of truth** is the machine-validated manifest
[`completion-1.0.toml`](completion-1.0.toml), enforced by `tests/manifest.rs` under
`cargo test`/CI (it fails on duplicate/missing ids, unknown links, missing owners/witnesses,
inconsistent states, an unpinned TS lib audit, or a Tier S/A/B scope family without exactly one
manifest owner). Every criterion there carries a stable id, track, owner, witness, and
complete/incomplete state — edit the manifest, not this prose, when status changes. Backlog `75`
owns the remaining structured divergence census needed to make Track C equally executable.

In one line, typokat is **complete** when its four tracks are: **A** model completeness (no
construct lowers to something silently permissive), **B** checker completeness (the
[`scope.md`](../reference/scope.md) Tier S/A/B surface is exhausted), **C** parity hygiene (no
known silent-FN family; every remaining divergence deliberate, safe-direction, and documented
in [`divergences.md`](../reference/divergences.md)), **D** scale + IDE (the §12 phase ladder:
`lib.d.ts` → modules → parallelism → incrementality, plus the `13` profiling gate decided).

The pinned TS 6.0.3 `lib.d.ts` surface audit — what actually blocks `14` — is
[`lib-audit-6.0.3.md`](lib-audit-6.0.3.md) (generic method/call/construct signatures shipped with
`41`, receiver typing shipped with `70`, and namespace/merging work shipped in the archived
2026-07-15 sprint). The committed proof is GO for starting `14`. Backlogs `50` and `75`
independently block 1.0; `63` owns parity-only surplus
diagnostics. `42`/`44` are not used by ES5 core. The official-suite scoreboard is the ratchet on the
way there — its syntax gates flip OOS→IN as features land — not a numeric pass/fail gate.

## Roadmap at a glance

The active backlog has **52 items**. The release classification comes from
[`completion-1.0.toml`](completion-1.0.toml) — that manifest, not this prose, decides what blocks
checker 1.0; the grouping below is the human roadmap view. Consumer-surface items are deliberately
absent from the manifest — they gate *consumers* of the checker, not the checker.

| Track | Active items | Typical effort | Role |
|---|---:|---|---|
| **A — model completeness** | 3 | L–XL | Eliminate the remaining silently-permissive model gaps; namespace/declaration merging is shipped. |
| **B — checker completeness** | 11 | M–L | Exhaust the Tier S/A/B diagnostic surface; independent items make useful sprint fillers. |
| **C — soundness/parity tail** | 30 | S–XL | Release-blocking known gaps, safe-direction parity improvements, reporting/robustness, the default-library base, and checker scaling. |
| **D — scale + IDE** | 8 | M–XL | Preview, full standard library, resolver breadth, parallel identity, incrementality — plus the non-blocking consumer surface (resolution queries, the resolution oracle). |

Effort is a **relative planning estimate**, not a time promise:

- **S** — one focused work unit, normally one subsystem;
- **M** — a bounded feature or bug family spanning a few touch points;
- **L** — cross-cutting semantics or several independently reviewable slices;
- **XL** — a milestone-sized architectural or integration project.

Re-check the estimate against HEAD when scheduling the item; the mandatory spec → implementation →
independent-review loop still applies at every size. Unless noted otherwise, every item below blocks
checker 1.0. The FP/tsc-parity subsection and D's consumer-surface subsection are the two
non-blocking groups.

## Items

**A. Model completeness — exact declaration types.** Every item kills a silently-permissive or
deliberately approximate model family. `42`, `44`, and `76` still block checker 1.0 but are not
needed by the
`lib.es5.d.ts` core.

- **L** · [`42`](42-enums-type-side.md) — enum type/value sides, nominal member types, and narrowing.
- **L** · [`44`](44-satisfies-as-const.md) — `satisfies` checking plus readonly literal/tuple semantics for `as const`.
- **XL** · [`76`](76-lazy-value-type-resolution.md) — demand-driven declaration/value types and inferred-return cycles · blocked by `46`, `48`.

**B. Checker completeness — Tier A/B diagnostic surface.** Independent of A; good sprint
fillers.

- **L** · [`18`](18-duplicate-identifier-detection.md) — duplicate members, bindings, and function implementations without breaking legal merging.
- **M** · [`19`](19-call-of-non-callable-diagnostic.md) — provable `TK2349` call-of-non-callable without false positives on deferred signatures.
- **L** · [`45`](45-operator-comparison-typing.md) — arithmetic, unary, comparison-overlap, `in`, and `instanceof` typing.
- **M** · [`46`](46-return-path-analysis.md) — CFG return coverage, accessors, and bare-return inference.
- **L** · [`47`](47-definite-assignment.md) — use-before-assignment, TDZ, and constructor property initialization.
- **M** · [`48`](48-no-implicit-any.md) — implicit-any declarations, binding elements, and element access.
- **L** · [`49`](49-possibly-undefined-family.md) — nullable receivers, optional members/calls, optional chaining, and non-null assertions.
- **L** · [`50`](50-type-predicates-assertions.md) — predicate/assertion signatures wired into flow narrowing.
- **XL** · [`51`](51-narrowing-tail.md) — remaining loops, member-path invalidation, and closure narrowing.
- **M** · [`52`](52-type-reference-tail.md) — value/type-space misuse, generic arity, and explicit call-site type args.
- **XL** · [`75`](75-scope-surface-tail.md) — family-by-family disposition of the remaining Tier S/A/B semantic surface.

**C. Known-gap fixes — the soundness/parity tail.**
Silent-FN kills (highest value per effort, schedule first; `54`–`65` are the 2026-07-07
cross-cutting soundness review findings, leader-verified vs tsc 6.0.3). The five HIGH
items — `53` `55` `57` `58` `61` — **shipped** in sprint-2026-07-07-soundness-fn-fixes.

- **L** · [`60`](60-fresh-literal-union-targets.md) — fresh-literal excess and assignability checks are skipped for union targets.
- **M** · [`62`](62-index-signature-relation-parity.md) — declared-source implicit indexes, numeric names, and optional `undefined` parity.
- **M** · [`21`](21-local-class-checking.md) — function-local classes entirely miss class-keyed checks.
- **L** · [`32`](32-eager-keyof-forward-references.md) — eager `keyof` over forward references silently degrades through fill order.
- **L** · [`66`](66-protected-override-compat.md) — protected override compatibility and lineage-aware nominal origins · blocked by `63(d)`.
- **L** · [`71`](71-expression-inference-fn-tail.md) — expression/iteration traversal: elisions, object/call spreads, tagged templates, and iteration targets.
- **M** · [`78`](78-generic-class-value-aliases.md) — generic class const aliases retain substitution and class-keyed construction facts.

FP / tsc-parity tail (safe direction, scheduled by opportunity):

- **L** · [`26`](26-cross-binder-nested-infer.md) — correct de Bruijn shifting/levels for nested `infer` binders.
- **L** · [`27`](27-template-buried-conditional-evaluation.md) — demand-evaluate conditionals buried in named structural templates.
- **L** · [`35`](35-keyof-union-and-key-source-edges.md) — `never`, template-pattern, and aliased-`keyof` mapped key sources.
- **L** · [`36`](36-conditional-structural-operand-parity.md) — tsc's eager-false/demand behavior for structurally wrapped operands.
- **M** · [`37`](37-constraint-approximation-deferred-args.md) — upper-bound approximation for provable deferred arguments.
- **M** · [`68`](68-contravariant-infer-intersection.md) — intersect same-name contravariant `infer` candidates instead of collapsing to `never`.
- **M** · [`69`](69-signature-rest-parity-tail.md) — embedded tuple-rest and variadic source-tuple inference.
- **M** · [`83`](83-contextual-generic-signature-relation.md) — contextual generic-signature instantiation under a cache-safe query-local relation environment.
- **L** · [`63`](63-review-parity-tail.md) — batched evaluator/relation/checker FPs, messages, and residual parser-depth guard.
- **XL** · [`82`](82-declare-global-value-space.md) — legal `declare global` value-space
  publication for variables, functions, complete class type/constructor pairs, and cross-file
  class/function+namespace payloads; not required by `lib.es5.d.ts` loading.

Reporting + robustness:

- **S** · [`84`](84-function-local-interface-panic.md) — a function-local interface panics the checker.
- **M** · [`90`](90-assignability-span-precision.md) — assignability diagnostics anchor on the declarator instead of the expression.
- **M** · [`91`](91-missing-property-presence-pass.md) — missing required properties should be reported before value mismatches.

Default-library base + method (fell out of the ADR-0017 snapshot removal):

- **M** · [`102`](102-frozen-prefix-writes-vanish-silently.md) — binder writes into the frozen library prefix vanish silently; an ordinary cross-file `globals.d.ts` yields a spurious `TK2304`. **Not a collision problem** — schedule ahead of `103`.
- **XL** · [`103`](103-library-merge-panics-and-routing.md) — a merge into a library-owned name is refused rather than performed (`declare global`, `interface Window`, `namespace Intl`); no collision route exists. The guard tier shipped; this is the correctness tier. Blocks the WU7 CLI cutover.
- **M** · [`98`](98-library-diagnostic-count-delta.md) — an unattributed 273 → 265 library diagnostic delta that a digest-only witness let drift for 102 commits. Its forward half shipped with ADR-0018; only the backwards attribution is open.
- **S** · [`97`](97-orphaned-wire-serialization.md) — ~6,300 lines of wire serialization left with no consumer.

Checker scaling (from sprint-2026-07-25; `95` carries a committed RED guard):

- **M** · [`95`](95-memoize-the-discarded-contextual-walks.md) — memoize the discarded contextual argument walks (exponential in nesting depth).
- **M** · [`94`](94-flat-per-file-regression-since-july-9.md) — a flat 3x per-file regression sitting under the modules exponent.
- **M** · [`85`](85-owner-closure-representation.md) — replay owner closure is quadratic on an accumulating chain.
- **M** · [`86`](86-free-param-summary-base-reset.md) — the free-param summary cache discards its sealed base on any mutation.
- **M** · [`105`](105-type-group-fragment-resort.md) — type-group fragments are re-sorted on every append, and a `cfg(not(test))` second sort makes test and production order library fragments by different keys.
- **M** · [`89`](89-scaling-guards-for-project-state.md) — nothing guards against per-item scans of whole-project state.

**D. Scale + IDE — the §12 phase ladder.**

- **XL** · [`72`](72-real-project-preview-readiness.md) — public project CLI, pinned strict project, mutation pack, and differential CI ratchet.
- **XL** · [`14`](14-libdts-loading.md) — full `lib.d.ts` and shared-prelude parallelism Stage 1; its namespace model gate is GO.
- **XL** · [`15`](15-modules-imports.md) — Bundler resolution via `oxc_resolver` plus typokat-owned
  module semantics; the local-relative slice shipped as M29.
- **XL** · [`16`](16-parallelism-type-universe.md) — deterministic parallel cross-file type identity · blocked by `14`, `15`.
- **XL** · [`17`](17-incrementality.md) — semantic batch cache followed by a Salsa-style IDE query layer · blocked by `16`.

Consumer surface (non-blocking — these gate consumers of the checker, not checker 1.0; absent from
`completion-1.0.toml` by design):

- **L** · [`79`](79-resolution-query-surface.md) — span → declaration provenance: the resolution
  map, `DeclId` → declaration site, an interface-member decl side table (**outside** the type hash),
  and `.d.ts.map` re-anchoring.
- **M** · [`80`](80-pavouk-resolution-oracle.md) — differential *resolution* oracle against
  pavouk/ts-morph: ~138k real resolution assertions over the Contember monorepo as a scale ratchet ·
  blocked by `79`; coverage tracks `15`, `14`.
- **M** · [`81`](81-resolve-only-driver-mode.md) — resolve-only driver (no relation engine, no
  diagnostics) · blocked by `79`. **Low priority**: an optimization, not a capability;
  profiling-gated, drop it if the relation engine does not dominate.

## Recommended order

1. **Keep `72` paused at its WU0 witness gate.** The accounting sprints (2026-07-10 and
   2026-07-12) shipped the AST census, explicit incomplete outcome, and the final expression-shape
   emission tail, but no public candidate met the preview contract without a project-specific shim.
   The five HIGH review findings (`53` `55` `57` `58` `61`) shipped in
   sprint-2026-07-07-soundness-fn-fixes; the remaining silent-FN C group (`60`, `62`, `32`,
   `21`, `66`, `71`, `78`) remains available as independently valuable dropped-error work.
2. **Execute the active full-library performance-cutover sprint (`14`).** The pinned ES5 proof is
   GO after immutable standalone namespace values shipped under ADR-0010. The
   [active sprint](../sprints/sprint-2026-07-21-full-lib-performance-cutover.md) owns the production
   cutover and the fail-closed cross-tool gate against native TypeScript 7. The shipped semantic
   snapshot it once carried was retired by
   [`ADR-0017`](../decisions/0017-compile-the-default-library-from-source.md); `97` and `98` are its
   fallout.
   Backlogs `50`/`75` remain independent 1.0 work; `63` is parity-only. The `13` profiling gate
   closed DEFER/no-VM under ADR-0001.
3. **Climb the full-project/scale ladder** (`14` → `15` → `16` → `17`), finishing the A/B/C
   remainder along the way. `14` + `15` must graduate a pinned Bundler-compatible full-stack
   witness (deptective only if it qualifies); the small `72` preview is not evidence for full
   resolver/lib fidelity. Per
   [`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md), `15` integrates and
   differentially validates `oxc_resolver` for the 1.0 Bundler profile; NodeNext/alternate profiles
   are deferred and physical lookup is not reimplemented locally.
4. **Land `79` + `80` alongside that climb, not after it.** With `72` paused at its witness gate we
   are short of real-world evidence, and the resolution oracle is a cheaper way to buy it: pavouk
   already holds ~138k compiler-accurate resolution assertions over a real monorepo, so `80` turns
   `14`/`15` from "shipped" into a *measured* coverage derivative — including the ~16.5k member-call
   edges that are the whole reason a checker exists here. Expect the first run to be mostly
   `incomplete`; that baseline is the point. `81` stays last and may be dropped on the profile.

Add scope sub-folders (`security/`, `perf/`, …) only once the flat list gets
unwieldy; numbers stay folder-local.
