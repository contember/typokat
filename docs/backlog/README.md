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
`54` `59` `64` (soundness-tail quick wins, sprint-2026-07-08), and `24` `39`
(M32 signature shape, sprint-2026-07-09) — see
[`../archive/`](../archive/README.md). Architecture §12 governs
phase ordering; the bytecode VM stays a deferred, profiling-gated refactor
([ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)). How each item
is built: [`../reference/dev-method.md`](../reference/dev-method.md).

## Definition of done (checker 1.0)

typokat is **complete** when all four hold:

1. **Model completeness** — track A is empty: no construct left that lowers to something
   silently permissive (overloads,
   generic methods, enums, namespaces + declaration merging, `satisfies`/`as const` —
   intersections `A & B` shipped as M31, signature shape shipped as M32).
2. **Checker completeness** — the scope map ([`../reference/scope.md`](../reference/scope.md))
   is exhausted: every Tier S + Tier A code is emitted with fixture coverage; every Tier B
   code is either shipped or explicitly reclassified out-of-scope in `scope.md`.
3. **Scale** — the §12 phase ladder tops out: full `lib.d.ts` (`14`), module-resolver breadth
   (`15`), parallel cross-file identity (`16`), incrementality (`17`); the `13` profiling gate
   is decided either way with recorded numbers.
4. **Parity hygiene** — track C is empty: no known silent-FN family remains, and every
   remaining tsc divergence is deliberate, safe-direction, and documented in
   `docs/reference/divergences.md`.

The official-suite scoreboard is the ratchet on the way there — its syntax gates (`enum`,
`namespace`, `satisfies`, `as const`, `module`) flip from OOS to IN as features land — not a
numeric pass/fail gate.

## Items

**A. Model completeness — the `lib.d.ts` critical path.** Each kills a silently-permissive
family; together they unblock `14`, whose source text uses all of them.
- [`40`](40-function-overloads.md) — overloads (declarations + resolution, TK2769).
- [`41`](41-generic-methods.md) — generic methods (method-level type params; subsumes `23`).
- [`42`](42-enums-type-side.md) — enums, type side (not needed by lib.d.ts itself; same family).
- [`43`](43-namespaces-declaration-merging.md) — namespaces (type side) + declaration merging.
- [`44`](44-satisfies-as-const.md) — `satisfies` + `as const` semantics.

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

**C. Known-gap fixes — the soundness/parity tail.**
Silent-FN kills (highest value per effort, schedule first; `54`–`65` are the 2026-07-07
cross-cutting soundness review findings, leader-verified vs tsc 6.0.3). The five HIGH
items — `53` `55` `57` `58` `61` — **shipped** in sprint-2026-07-07-soundness-fn-fixes.
- [`56`](56-silent-instantiation-cycles.md) — instantiation cycles silently resolve to error.
- [`60`](60-fresh-literal-union-targets.md) — fresh literals vs union targets: excess + assignability skipped.
- [`62`](62-index-signature-relation-parity.md) — index-signature relation parity (implicit-index rule, numeric names).
- [`65`](65-multi-arg-candidate-union-fn.md) — multi-argument inference unions candidates instead of fixing-then-checking (drops TS2345).
- [`21`](21-local-class-checking.md) — function-local classes entirely unchecked.
- [`22`](22-new-callee-forms.md) — `new (C)()` / aliased `new` miss class-keyed checks.
- [`32`](32-eager-keyof-forward-references.md) — eager `keyof` over forward references.
- [`66`](66-protected-override-compat.md) — protected↔protected incompatible override skips TK2416 (dropped TS2416).
- [`67`](67-utility-alias-constraint-enforcement.md) — utility/prelude alias type-param constraints unenforced (`ReturnType<number>` drops TS2344).

FP / tsc-parity tail (safe direction, scheduled by opportunity):
- [`23`](23-static-method-type-params.md) — spurious TK2304 on static method type params (or close via `41`).
- [`26`](26-cross-binder-nested-infer.md) — cross-binder nested `infer` (de Bruijn shifting on embed).
- [`27`](27-template-buried-conditional-evaluation.md) — evaluate template-buried conditionals.
- [`30`](30-numeric-literal-correctness.md) — JS-exact number stringification (dtoa).
- [`35`](35-keyof-union-and-key-source-edges.md) — `keyof` over unions + edge key sources.
- [`36`](36-conditional-structural-operand-parity.md) — conditional parity for structurally-wrapped operands.
- [`37`](37-constraint-approximation-deferred-args.md) — TK2344 constraint approximation for deferred args.
- [`68`](68-contravariant-infer-intersection.md) — same-name contravariant `infer` over-reports to `never` (should intersect).
- [`69`](69-signature-rest-parity-tail.md) — signature rest parity tail (embedded tuple-rest inference, variadic source tuple infer).
- [`63`](63-review-parity-tail.md) — 2026-07-07 review parity tail (batched small FPs, messages, depth guard).

**D. Scale + IDE — the §12 phase ladder.**
- [`38`](38-minimal-ambient-prelude.md) — minimal ambient prelude slice (early real-world signal; replaced by `14`).
- [`13`](13-bytecode-vm.md) — post-evaluator profiling gate (instrument shipped: `tooling/bench/`).
- [`14`](14-libdts-loading.md) — full `lib.d.ts` + parallelism Stage 1 · blocked-by `24` `39` `40` `41` `43`.
- [`15`](15-modules-imports.md) — module-resolver breadth (slice 2; slice 1 shipped as M29).
- [`16`](16-parallelism-type-universe.md) — parallelism Stage 2 (cross-file identity) · blocked-by `14`, `15`.
- [`17`](17-incrementality.md) — incrementality (Phase 5) · blocked-by `16`.

## Recommended order

1. **Kill the known silent-FN families** — the five HIGH review findings (`53` `55` `57` `58`
   `61`) shipped in sprint-2026-07-07-soundness-fn-fixes; the remaining C group (`56`, `60`, `62`,
   `65`, `32`, `21`, `22`, `66`, `67`) is next, every one a dropped-error class.
2. **Run track A** to unblock `14` (`25` intersections shipped as M31 and `24`/`39` signature
   shape shipped as M32); interleave B items and C's
   parity tail as warm-ups between the remaining A milestones. `38` (minimal prelude) and `13` (profiling
   gate) are independent — schedule whenever the signal is worth it; `13` is cheap now that
   `tooling/bench/` exists.
3. **Climb track D** in ladder order (`14` → `15` → `16` → `17`), finishing the B/C remainder along
   the way.

Add scope sub-folders (`security/`, `perf/`, …) only once the flat list gets
unwieldy; numbers stay folder-local.
