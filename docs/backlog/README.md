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
sprint-2026-07-09), and `74` (declaration hoisting parity,
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

## Items

**A. Model completeness — the `lib.d.ts` critical path.** Each kills a silently-permissive
family; together they unblock `14`, whose source text uses all of them.
- [`41`](41-generic-methods.md) — generic methods (method-level type params; subsumes `23`).
- [`42`](42-enums-type-side.md) — enums, type side (not needed by lib.d.ts itself; same family).
- [`43`](43-namespaces-declaration-merging.md) — namespaces (type side) + declaration merging.
- [`44`](44-satisfies-as-const.md) — `satisfies` + `as const` semantics.
- [`70`](70-this-parameter-typing.md) — `this`-parameter typing + `ThisType<T>` (lib.d.ts prerequisite; audit-discovered).

**B. Checker completeness — Tier A/B diagnostic surface.** Independent of A; good sprint
fillers.
- [`18`](18-duplicate-identifier-detection.md) — duplicate identifiers (`TK2300` members + `TK2451` redeclare).
- [`19`](19-call-of-non-callable-diagnostic.md) — `TK2349` call-of-non-callable (unblocked by `40`'s callability model).
- [`45`](45-operator-comparison-typing.md) — operator & comparison typing (TK2362/2363/2365, TK2367).
- [`46`](46-return-path-analysis.md) — return-path analysis (TK2355/2366/2378/7030).
- [`47`](47-definite-assignment.md) — definite assignment (TK2454/2448/2564).
- [`48`](48-no-implicit-any.md) — `noImplicitAny` family (TK7005/7006/7008/7031/7053).
- [`49`](49-possibly-undefined-family.md) — possibly-undefined/null family + optional methods/accessors.
- [`50`](50-type-predicates-assertions.md) — type predicates (`x is T`) + assertion functions.
- [`51`](51-narrowing-tail.md) — narrowing tail: remaining loop forms, member paths, closures.
- [`52`](52-type-reference-tail.md) — type-reference tail (TS2749, TS2314/2315, TK2558).
- [`75`](75-scope-surface-tail.md) — canonical Tier S/A/B semantic ownership tail (its divergence-census infrastructure shipped 2026-07-10).

**C. Known-gap fixes — the soundness/parity tail.**
Silent-FN kills (highest value per effort, schedule first; `54`–`65` are the 2026-07-07
cross-cutting soundness review findings, leader-verified vs tsc 6.0.3). The five HIGH
items — `53` `55` `57` `58` `61` — **shipped** in sprint-2026-07-07-soundness-fn-fixes.
- [`56`](56-silent-instantiation-cycles.md) — instantiation cycles silently resolve to error.
- [`60`](60-fresh-literal-union-targets.md) — fresh literals vs union targets: excess + assignability skipped.
- [`62`](62-index-signature-relation-parity.md) — index-signature relation parity (implicit-index rule, numeric names).
- [`21`](21-local-class-checking.md) — function-local classes entirely unchecked.
- [`22`](22-new-callee-forms.md) — `new (C)()` / aliased `new` miss class-keyed checks.
- [`32`](32-eager-keyof-forward-references.md) — eager `keyof` over forward references.
- [`66`](66-protected-override-compat.md) — protected↔protected incompatible override skips TK2416 (dropped TS2416).
- [`67`](67-utility-alias-constraint-enforcement.md) — utility/prelude alias type-param constraints unenforced (`ReturnType<number>` drops TS2344).
- [`30`](30-numeric-literal-correctness.md) — JS-exact number stringification: a non-canonical `${1e21}` digit string is **accepted** (dropped TS2322 — an under-report, not a safe FP).
- [`71`](71-expression-inference-fn-tail.md) — expression/iteration traversal silent-FN tail (binary results, template interpolations, spread, iterability).
- [`73`](73-unsupported-surface-audit.md) — surface-accounting emission tail (census + incomplete outcome shipped 2026-07-10; the `infer_expr` shape tail still exits clean).
- [`76`](76-lazy-value-type-resolution.md) — exact lazy declaration/value-type queries replace the safe `unknown` forward-return approximation · blocked by `46`, `48`.

FP / tsc-parity tail (safe direction, scheduled by opportunity):
- [`23`](23-static-method-type-params.md) — spurious TK2304 on static method type params (or close via `41`).
- [`26`](26-cross-binder-nested-infer.md) — cross-binder nested `infer` (de Bruijn shifting on embed).
- [`27`](27-template-buried-conditional-evaluation.md) — evaluate template-buried conditionals.
- [`35`](35-keyof-union-and-key-source-edges.md) — `keyof` over unions + edge key sources.
- [`36`](36-conditional-structural-operand-parity.md) — conditional parity for structurally-wrapped operands.
- [`37`](37-constraint-approximation-deferred-args.md) — TK2344 constraint approximation for deferred args.
- [`68`](68-contravariant-infer-intersection.md) — same-name contravariant `infer` over-reports to `never` (should intersect).
- [`69`](69-signature-rest-parity-tail.md) — signature rest parity tail (embedded tuple-rest inference, variadic source tuple infer).
- [`63`](63-review-parity-tail.md) — 2026-07-07 review parity tail (batched small FPs, messages, depth guard).

**D. Scale + IDE — the §12 phase ladder.**
- [`72`](72-real-project-preview-readiness.md) — honest public-CLI preview on a pinned small strict project, with clean + mutation differential ratchet.
- [`38`](38-minimal-ambient-prelude.md) — minimal ambient prelude slice (early real-world signal; replaced by `14`).
- [`13`](13-bytecode-vm.md) — post-evaluator profiling gate (instrument shipped: `tooling/bench/`).
- [`14`](14-libdts-loading.md) — full `lib.d.ts` + parallelism Stage 1 · blocked-by `41` `43` `70`.
- [`15`](15-modules-imports.md) — module-resolver breadth (slice 2; slice 1 shipped as M29).
- [`16`](16-parallelism-type-universe.md) — parallelism Stage 2 (cross-file identity) · blocked-by `14`, `15`.
- [`17`](17-incrementality.md) — incrementality (Phase 5) · blocked-by `16`.

## Recommended order

1. **Make incompleteness executable, then kill the known silent-FN families** — the accounting
   sprint (2026-07-10) shipped `73`'s AST census + incomplete outcome and `75`'s divergence
   census, so new silent paths can no longer disappear from the plan; `73` keeps the emission
   tail. The five HIGH review findings (`53` `55` `57` `58`
   `61`) shipped in sprint-2026-07-07-soundness-fn-fixes; `64` `34` `33` `54` `59` `65`
   shipped in follow-up sprints; the remaining silent-FN C group (`56`, `60`, `62`, `30`, `32`,
   `21`, `22`, `66`, `67`, `71`) is next, every one a dropped-error class. The next
   executable chain is **`73` closure → `38` → `72`** (finish surface emission, then the
   prelude, then the honest real-project preview).
2. **Ship the honest preview slice early:** after `73`, run `38` then `72`. Its WU0 pins a
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
