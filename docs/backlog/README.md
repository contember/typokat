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
(M29 local-relative modules) plus its bounded files-only Bundler project route, deterministic
accounting substrate, and acyclic named source-re-export slice, `53` `55` `57` `58` `61` (the five
HIGH silent-FN fixes,
sprint-2026-07-07), `25` (M31 intersection types, sprint-2026-07-07), `33` `34`
`54` `59` `64` (soundness-tail quick wins, sprint-2026-07-08), `24` `39`
(M32 signature shape, sprint-2026-07-09), `65` (inference candidate policy,
sprint-2026-07-09), `40` (M33 function overloads,
sprint-2026-07-09), `74` (declaration hoisting parity, sprint-2026-07-11),
`38` (minimal ambient prelude, sprint-2026-07-11), `14` (full source-compiled default library,
sprint-2026-08-02), `67` (modeled `ReturnType`
constraint, sprint-2026-07-11), and `30` (JS-exact number stringification,
sprint-2026-07-11), and `41` + `23` (persistent generic method/call/construct signatures,
including the static generic binder path, sprint-2026-07-12), and `22` (parenthesized and
non-generic class-value construction facts, sprint-2026-07-12), and `56` (cycle-aware
evaluator memoization + `TK2589`, sprint-2026-07-12), and `77` (callable-object
`ReturnType` extraction, sprint-2026-07-12), and `70` (explicit receiver slots,
contextual `ThisType<T>`, and receiver utilities, sprint-2026-07-12), and `13` (the ADR-0001
profiling gate, strict DEFER/no-VM, WU5 of
[`sprint-2026-07-12-pre-lib-hardening.md`](../archive/sprint-2026-07-12-pre-lib-hardening.md),
closed 2026-07-13), `43` (namespace/declaration-space completion,
[`sprint-2026-07-15-namespaces-declaration-merging.md`](../archive/sprint-2026-07-15-namespaces-declaration-merging.md)),
and `84` (canonical declaration owners and publication completeness,
[`sprint-2026-08-18-canonical-declaration-inventory.md`](../archive/sprint-2026-08-18-canonical-declaration-inventory.md)). The earlier shipped items are in
[`../archive/`](../archive/README.md), including the completed WU6 official-suite ratchet.
The historical implementations for `102` (visible frozen-prefix writes) and `103` (source-native
sparse collision routing) shipped through the backlog-`14` closure, then were replaced by the
archived [`project-local universe sprint`](../archive/sprint-2026-08-22-project-local-library-universe.md);
their backlog files remain removed.
The flat object-variable publication slice of `48` shipped through
the archived [`object-binding sprint`](../archive/sprint-2026-08-09-object-binding-publication.md);
the implicit-any family and excluded binding shapes remain open. Backlog `109` shipped through the
archived [`optional-parameter sprint`](../archive/sprint-2026-08-11-optional-parameter-undefined-arguments.md).
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

The pinned TS 6.0.3 `lib.d.ts` surface audit — the historical start gate for `14` — is
[`lib-audit-6.0.3.md`](lib-audit-6.0.3.md) (generic method/call/construct signatures shipped with
`41`, receiver typing shipped with `70`, and namespace/merging work shipped in the archived
2026-07-15 sprint). The committed proof is GO for starting `14`. Backlogs `50` and `75`
independently block 1.0; `63` owns both canonical Callable/Newable compatibility and its surplus
diagnostic cardinality as parity work. The official-suite
scoreboard is the ratchet on the way there — its syntax gates flip OOS→IN as features land — not a
numeric pass/fail gate.

## Roadmap at a glance

The roadmap has **60 remaining items**: 51 release/consumer items in the existing four-track view
and 9 non-blocking engineering-maintenance items. The release classification comes from
[`completion-1.0.toml`](completion-1.0.toml) — that manifest, not this prose, decides what blocks
checker 1.0; the grouping below is the human roadmap view. Consumer-surface items are deliberately
absent from the manifest — they gate *consumers* of the checker, not the checker.

| Track | Active items | Typical effort | Role |
|---|---:|---|---|
| **A — model completeness** | 1 | L–XL | Eliminate the remaining silently-permissive model gaps; namespace/declaration merging is shipped. |
| **B — checker completeness** | 11 | M–L | Exhaust the Tier S/A/B diagnostic surface; independent items make useful sprint fillers. |
| **C — soundness/parity tail** | 32 | S–XL | Release-blocking known gaps, safe-direction parity improvements, reporting/robustness, default-library fallout, and checker scaling. |
| **D — scale + IDE** | 7 | M–XL | Preview, resolver breadth, parallel identity, incrementality — plus the non-blocking consumer surface (resolution queries, the resolution oracle). |
| **Maintenance — non-blocking** | 9 | S–XL | Safe checker seams and reliable development feedback. |

Effort is a **relative planning estimate**, not a time promise:

- **S** — one focused work unit, normally one subsystem;
- **M** — a bounded feature or bug family spanning a few touch points;
- **L** — cross-cutting semantics or several independently reviewable slices;
- **XL** — a milestone-sized architectural or integration project.

Re-check the estimate against HEAD when scheduling the item; the mandatory spec → implementation →
independent-review loop still applies at every size. Unless noted otherwise, every item below blocks
checker 1.0. The FP/tsc-parity subsection, D's consumer-surface subsection, and the maintenance
section are non-blocking groups.

## Items

**A. Model completeness — exact declaration types.** Every item kills a silently-permissive or
deliberately approximate model family. `76` still blocks checker 1.0 but is not needed by the
`lib.es5.d.ts` core; `44` shipped (bounded `satisfies` + `as const`).

- **XL** · [`76`](76-lazy-value-type-resolution.md) — demand-driven declaration/value types and inferred-return cycles · blocked by `46`, `48`.

**B. Checker completeness — Tier A/B diagnostic surface.** Independent of A; good sprint
fillers.

- **L** · [`18`](18-duplicate-identifier-detection.md) — duplicate members, bindings, and function implementations without breaking legal merging.
- **M** · [`19`](19-call-of-non-callable-diagnostic.md) — provable `TK2349` call-of-non-callable without false positives on deferred signatures.
- **L** · [`45`](45-operator-comparison-typing.md) — arithmetic, unary, comparison-overlap, `in`, and `instanceof` typing.
- **M** · [`46`](46-return-path-analysis.md) — CFG return coverage, accessors, and bare-return inference.
- **L** · [`47`](47-definite-assignment.md) — use-before-assignment, TDZ, and constructor property initialization.
- **M** · [`48`](48-no-implicit-any.md) — implicit-any declarations, remaining binding elements,
  element access, and `globalThis` property code fidelity; flat ready-source object variables ship.
- **L** · [`49`](49-possibly-undefined-family.md) — nullable receivers, optional members/calls, optional chaining, and non-null assertions.
- **L** · [`50`](50-type-predicates-assertions.md) — predicate/assertion signatures wired into flow narrowing.
- **XL** · [`51`](51-narrowing-tail.md) — remaining loops, member-path invalidation, closure
  narrowing, and assignment-target evaluation order.
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
- **L** · [`111`](111-structural-inference-literal-provenance.md) — preserve member-level literal
  widenability through structural inference so fresh object/array candidates do not silently stay
  narrower than tsc.
- **XL** · [`120`](120-semantic-constraint-construction-order.md) — semantic generic-constraint
  construction order, source-owned signature demand, and the remaining reference-authentication
  dependencies; separate from bounded `as const` delivery.
- **L** · [`122`](122-signature-candidate-fixing-policy.md) — remaining candidate-position,
  contextual parameter fixing and compound return-role policy beyond bounded feature regressions.
- **XL** · [`123`](123-contextual-class-initializer-publication.md) — effective contextual class
  initializer inputs, including self-dependent contexts; separate from bounded field `as const`.
- **S–M** · [`124`](124-class-initializer-ready-reference-projection.md) — project ready tuple/array element references inside const-asserted class initializers.

FP / tsc-parity tail (safe direction, scheduled by opportunity):

- **L** · [`26`](26-cross-binder-nested-infer.md) — correct de Bruijn shifting/levels for nested `infer` binders.
- **L** · [`27`](27-template-buried-conditional-evaluation.md) — demand-evaluate conditionals buried in named structural templates.
- **L** · [`35`](35-keyof-union-and-key-source-edges.md) — `never`, template-pattern, and aliased-`keyof` mapped key sources.
- **L** · [`36`](36-conditional-structural-operand-parity.md) — tsc's eager-false/demand behavior for structurally wrapped operands.
- **M** · [`37`](37-constraint-approximation-deferred-args.md) — upper-bound approximation for provable deferred arguments.
- **M** · [`68`](68-contravariant-infer-intersection.md) — intersect same-name contravariant `infer` candidates instead of collapsing to `never`.
- **M** · [`69`](69-signature-rest-parity-tail.md) — embedded tuple-rest inference, callable-union rest-shape synthesis, and variadic source tuples.
- **M** · [`83`](83-contextual-generic-signature-relation.md) — contextual generic-signature instantiation under a cache-safe query-local relation environment.
- **M** · [`107`](107-apparent-built-in-surfaces.md) — apparent `Function` members on callable
  objects and apparent `Object` members on ordinary object sources.
- **M** · [`108`](108-generic-arrow-signature-descriptors.md) — retain generic arrow descriptors
  through contextual object-method assignment.
- **M** · [`110`](110-contextual-return-literal-inference.md) — contextual return inference must
  preserve a proved singleton without overriding dependent defaults with a widened candidate.
- **L** · [`63`](63-review-parity-tail.md) — Callable/Newable canonical compatibility and surplus
  cardinality, batched evaluator/relation/checker FPs, messages, and the residual parser-depth guard.
- **XL** · [`82`](82-declare-global-value-space.md) — legal `declare global` value-space
  publication for variables, functions, complete class type/constructor pairs, and cross-file
  class/function+namespace payloads; not required by `lib.es5.d.ts` loading.

Reporting + robustness:

- **M** · [`90`](90-assignability-span-precision.md) — assignability diagnostics anchor on the declarator instead of the expression.
- **M** · [`91`](91-missing-property-presence-pass.md) — missing required properties should be reported before value mismatches.

Default-library base + method (fell out of the ADR-0017 snapshot removal):

- **M** · [`98`](98-library-diagnostic-count-delta.md) — an unattributed 273 → 265 library diagnostic delta that a digest-only witness let drift for 102 commits. Its forward half shipped with ADR-0018; only the backwards attribution is open.

Checker scaling (from the archived [`2026-07-25 sprint`](../archive/sprint-2026-07-25-checker-scaling.md)):

- **S** · [`106`](106-contextual-memo-residual-key-surfaces.md) — two residual surfaces in the contextual-walk memo are argued rather than constructed: one-shot consumption, and a 64-bit type-param hash.
- **M** · [`94`](94-flat-per-file-regression-since-july-9.md) — remeasure the current route and
  close the flat per-file gap left after the shipped merge and completion-storage reductions.
- **M** · [`86`](86-free-param-summary-base-reset.md) — explicit layered-interner consumers discard free-param summary bases on mutation; production sessions no longer use that lifecycle.
- **M** · [`89`](89-scaling-guards-for-project-state.md) — add a multi-file benchmark gate and
  require scaling notes for compilation-wide structures; local-layer probes already shipped.

**D. Scale + IDE — the §12 phase ladder.**

- **L** · [`72`](72-real-project-preview-readiness.md) — select a zero-clean public project atop the
  shipped CLI, then land its mutation pack and differential CI ratchet.
- **XL** · [`15`](15-modules-imports.md) — remaining Bundler module semantics and resolver breadth;
  the archived [`2026-08-08 sprint`](../archive/sprint-2026-08-08-acyclic-source-reexports.md)
  shipped acyclic named source re-exports, and the archived
  [`default-slot sprint`](../archive/sprint-2026-08-08-default-module-slots.md) shipped direct default
  declarations/expressions and regular default imports. Default bridges, namespace/star forms,
  cycles, and packages remain.
- **XL** · [`16`](16-parallelism-type-universe.md) — deterministic parallel cross-file type identity · blocked by `15`.
- **XL** · [`17`](17-incrementality.md) — semantic batch cache followed by a Salsa-style IDE query layer · blocked by `16`.

Consumer surface (non-blocking — these gate consumers of the checker, not checker 1.0; absent from
`completion-1.0.toml` by design):

- **L** · [`79`](79-resolution-query-surface.md) — span → declaration provenance: the resolution
  map, `DeclId` → declaration site, an interface-member decl side table (**outside** the type hash),
  and `.d.ts.map` re-anchoring.
- **M** · [`80`](80-pavouk-resolution-oracle.md) — differential *resolution* oracle against
  pavouk/ts-morph: ~138k real resolution assertions over the Contember monorepo as a scale ratchet ·
  blocked by `79`; coverage tracks `15` over the shipped default-library baseline.
- **M** · [`81`](81-resolve-only-driver-mode.md) — resolve-only driver (no relation engine, no
  diagnostics) · blocked by `79`. **Low priority**: an optimization, not a capability;
  profiling-gated, drop it if the relation engine does not dominate.

**Research architecture and engineering maintenance — non-blocking.** These items change or reduce
standing architecture and feedback cost. They do not change the checker 1.0 completeness manifest
and must not weaken soundness or parity gates merely to improve layout or speed.

- **M** · [`119`](119-class-publication-preparation.md) — extract owned class preparation while
  preserving one visible atomic publication facade.
- **S** · [`121`](121-optional-project-binder-probe-test-build.md) — repair the optional workspace
  probe's stale normalized import/export test projection.
- **S** · [`125`](125-checker-duplicate-helper-cleanup.md) — delete two unused dev-dependencies and
  dedupe the alias-surface and tuple-rest helpers (audit M12/M6/M7).
- **S+M** · [`126`](126-pass-request-and-manifest-ownership.md) — collapse the mirrored pass request,
  then build and authenticate the declaration manifest once per route (audit M4 then M1).
- **M** · [`127`](127-test-guard-parsers-and-shape-assertions.md) — replace the four partial Rust
  scanners and the spelling assertions in the architecture test guards (audit M2/M3).
- **M** · [`128`](128-typed-native-library-roles.md) — replace the eight-slot positional native
  library role protocol with a typed role (audit M5).
- **S** · [`129`](129-reexport-notice-formatter.md) — format the re-export provenance notice in
  one place (audit M10).
- **M** · [`130`](130-transition-regression-coverage.md) — transition-oriented regression coverage
  across publication, copy and write boundaries in the differential grammar and fixtures (audit S4/U5).
- **S–M** · [`131`](131-conditional-ownership-cleanups.md) — conditional: member-context scope
  helper and driver error payload retention (audit M9/M11); each half may close as rejected.

`125`, `126` and `129` are scheduled in the active
[`audit cleanup sprint`](../sprints/sprint-2026-09-09-audit-cleanup-batch.md).

## Recommended order

1. **Close the remaining general blockers exposed by the immutable `placetext` screen.** The
   archived [`object-binding sprint`](../archive/sprint-2026-08-09-object-binding-publication.md)
   removed all 22 measured binding `TK2304` identities, and the archived
   [`optional-parameter sprint`](../archive/sprint-2026-08-11-optional-parameter-undefined-arguments.md)
   removed the next `TK2345`; the archived
   [`enum sprint`](../archive/sprint-2026-08-12-enum-type-value-model.md) then removed all four
   enum/module-recovery identities. The latest screen has six incomplete records and four
   diagnostics: computed keys plus their revealed `TK2741` (`75`), template interpolation (`71`),
   and predicate-bearing array diagnostics plus their revealed downstream `TK2322` (`50`/`107`).
   Re-screen only from reviewed general commits; do not patch the project.
2. **Close `72` only when a re-screen is genuinely zero-clean.** Preserve the
   native-versus-overlay distinction and prove target/library meaning equivalence. Only after the
   unchanged zero gate passes may the descriptor, three exact TS/TK mutations, deterministic
   fresh-cache runner, fault controls, and CI identity ratchet land. Do not relax a threshold or
   add a shim.
3. **Continue the full-project/scale ladder** (package/config/`.d.ts` remainder of `15` → `16` →
   `17`), finishing the A/B/C remainder along the way. After `15` ships, it and the shipped
   production work of `14` must graduate a pinned Bundler-compatible full-stack witness (deptective
   only if it qualifies); the small `72` preview is not evidence for full resolver/lib fidelity. Per
   [`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md), `15` integrates and
   differentially validates `oxc_resolver` for the 1.0 Bundler profile; NodeNext/alternate profiles
   are deferred and physical lookup is not reimplemented locally.
4. **Land `79` + `80` alongside the breadth climb.** The shipped project summary provides the
   bounded orchestration base; the resolution oracle adds breadth without enlarging the public
   checker claim. Pavouk already holds ~138k compiler-accurate resolution assertions over a real monorepo,
   so `80` turns `14`/`15` into a measured coverage derivative — including ~16.5k member-call edges.
   Expect the first run to be mostly `incomplete`; that baseline is the point. `81` stays last and
   may be dropped on the profile.
Add scope sub-folders (`security/`, `perf/`, …) only once the flat list gets
unwieldy; numbers stay folder-local.
