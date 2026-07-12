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
sprint-2026-07-11) — see
[`../archive/`](../archive/README.md). Architecture §12 governs
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
[`lib-audit-6.0.3.md`](lib-audit-6.0.3.md) (headline: `41` generic methods, `43`
namespaces/declaration merging, and the audit-discovered `70` `this`-parameter typing block the
es5 core; `42`/`44` are not used by es5 core). The official-suite scoreboard is the ratchet on
the way there — its syntax gates flip OOS→IN as features land — not a numeric pass/fail gate.

## Roadmap at a glance

The active backlog has **41 items**: **32 checker-1.0 release blockers** and **9 non-blocking,
safe-direction parity items**. The release classification comes from
[`completion-1.0.toml`](completion-1.0.toml); the grouping below is the human roadmap view.

| Track | Active items | Typical effort | Role |
|---|---:|---|---|
| **A — model completeness** | 6 | L–XL | Eliminate silently-permissive model gaps; `41`/`43`/`70` directly unblock full `lib.d.ts`. |
| **B — checker completeness** | 11 | M–L | Exhaust the Tier S/A/B diagnostic surface; independent items make useful sprint fillers. |
| **C — soundness/parity tail** | 18 | S–L | Nine release-blocking known gaps plus nine safe-direction parity improvements. |
| **D — scale + IDE** | 6 | XL | Preview, full standard library, resolver breadth, parallel identity, and incrementality. |

Effort is a **relative planning estimate**, not a time promise:

- **S** — one focused work unit, normally one subsystem;
- **M** — a bounded feature or bug family spanning a few touch points;
- **L** — cross-cutting semantics or several independently reviewable slices;
- **XL** — a milestone-sized architectural or integration project.

Re-check the estimate against HEAD when scheduling the item; the mandatory spec → implementation →
independent-review loop still applies at every size. Unless noted otherwise, every item below blocks
checker 1.0. The FP/tsc-parity subsection is the only non-blocking group.

## Items

**A. Model completeness — the `lib.d.ts` critical path plus exact declaration types.** Every item
kills a silently-permissive or deliberately approximate model family. `41`, `43`, and `70` are the
direct blockers of `14`; `42`, `44`, and `76` still block checker 1.0 but are not needed by the
`lib.es5.d.ts` core.

- **XL** · [`41`](41-generic-methods.md) — generic methods across lowering, inference, constraints, and relation; subsumes `23`.
- **L** · [`42`](42-enums-type-side.md) — enum type/value sides, nominal member types, and narrowing.
- **XL** · [`43`](43-namespaces-declaration-merging.md) — namespace binding, qualified names, and declaration merging.
- **L** · [`44`](44-satisfies-as-const.md) — `satisfies` checking plus readonly literal/tuple semantics for `as const`.
- **L** · [`70`](70-this-parameter-typing.md) — `this` signature slots and contextual `ThisType<T>`; direct `lib.d.ts` blocker.
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

- **M** · [`56`](56-silent-instantiation-cycles.md) — instantiation cycles silently resolve to an error type without `TK2589`.
- **L** · [`60`](60-fresh-literal-union-targets.md) — fresh-literal excess and assignability checks are skipped for union targets.
- **M** · [`62`](62-index-signature-relation-parity.md) — declared-source implicit indexes, numeric names, and optional `undefined` parity.
- **M** · [`21`](21-local-class-checking.md) — function-local classes entirely miss class-keyed checks.
- **S** · [`22`](22-new-callee-forms.md) — parenthesized/aliased `new` misses abstract and constructor-accessibility checks.
- **L** · [`32`](32-eager-keyof-forward-references.md) — eager `keyof` over forward references silently degrades through fill order.
- **L** · [`66`](66-protected-override-compat.md) — protected override compatibility and lineage-aware nominal origins · blocked by `63(d)`.
- **L** · [`71`](71-expression-inference-fn-tail.md) — expression/iteration traversal: elisions, object/call spreads, tagged templates, and iteration targets.
- **M** · [`77`](77-returntype-call-signature-infer.md) — infer `ReturnType` from object and overload call signatures.

FP / tsc-parity tail (safe direction, scheduled by opportunity):

- **S** · [`23`](23-static-method-type-params.md) — suppress spurious `TK2304` on static method type params, or close through `41`.
- **L** · [`26`](26-cross-binder-nested-infer.md) — correct de Bruijn shifting/levels for nested `infer` binders.
- **L** · [`27`](27-template-buried-conditional-evaluation.md) — demand-evaluate conditionals buried in named structural templates.
- **L** · [`35`](35-keyof-union-and-key-source-edges.md) — `never`, template-pattern, and aliased-`keyof` mapped key sources.
- **L** · [`36`](36-conditional-structural-operand-parity.md) — tsc's eager-false/demand behavior for structurally wrapped operands.
- **M** · [`37`](37-constraint-approximation-deferred-args.md) — upper-bound approximation for provable deferred arguments.
- **M** · [`68`](68-contravariant-infer-intersection.md) — intersect same-name contravariant `infer` candidates instead of collapsing to `never`.
- **M** · [`69`](69-signature-rest-parity-tail.md) — embedded tuple-rest and variadic source-tuple inference.
- **L** · [`63`](63-review-parity-tail.md) — batched evaluator/relation/checker FPs, messages, and residual parser-depth guard.

**D. Scale + IDE — the §12 phase ladder.**

- **XL** · [`72`](72-real-project-preview-readiness.md) — public project CLI, pinned strict project, mutation pack, and differential CI ratchet.
- **S gate / XL if triggered** · [`13`](13-bytecode-vm.md) — profile the evaluator; build a VM only if dispatch is the measured bottleneck.
- **XL** · [`14`](14-libdts-loading.md) — full `lib.d.ts` and shared-prelude parallelism Stage 1 · blocked by `41`, `43`, `70`.
- **XL** · [`15`](15-modules-imports.md) — NodeNext/package/tsconfig resolver breadth; the local-relative slice shipped as M29.
- **XL** · [`16`](16-parallelism-type-universe.md) — deterministic parallel cross-file type identity · blocked by `14`, `15`.
- **XL** · [`17`](17-incrementality.md) — semantic batch cache followed by a Salsa-style IDE query layer · blocked by `16`.

## Recommended order

1. **Ship the honest preview slice next** — the accounting sprints (2026-07-10 and 2026-07-12)
   shipped the AST census, explicit incomplete outcome, and the final expression-shape emission
   tail, so new silent paths can no longer disappear from the plan. The five HIGH review findings (`53` `55` `57` `58`
   `61`) shipped in sprint-2026-07-07-soundness-fn-fixes; `64` `34` `33` `54` `59` `65`
   shipped in follow-up sprints; the remaining silent-FN C group (`56`, `60`, `62`, `32`,
   `21`, `22`, `66`, `71`, `77`) remains available as independently valuable dropped-error work.
   The next executable item is **`72`**; the bounded prelude slice `38` shipped on 2026-07-11.
2. **Run `72`:** its WU0 pins a
   genuinely small real project that fits the preview surface; it must not grow a project-specific
   lib shim. This is the first "point it at a project" milestone.
3. **Run track A** to unblock `14` (`25` intersections shipped as M31, `24`/`39` signature
   shape shipped as M32, and `40` overloads shipped as M33); interleave B items and C's
   parity tail as warm-ups between the remaining A milestones. `13` (profiling gate) is cheap now that
   `tooling/bench/` exists.
4. **Climb the full-project/scale ladder** (`14` → `15` → `16` → `17`), finishing the B/C
   remainder along the way. `14` + `15` must graduate the pinned deptective full-stack witness;
   the small `72` preview is not evidence for full resolver/lib fidelity.

Add scope sub-folders (`security/`, `perf/`, …) only once the flat list gets
unwieldy; numbers stay folder-local.
