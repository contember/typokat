# PHPStan architecture — comparison + reviewed lessons

Survey note, 2026-07-09. PHPStan (`~/projects/sandbox/phpstan-src`, commit `43e9d32`) was
compared layer-by-layer against typokat's implemented + planned architecture; six candidate
lessons then went through an independent adversarial peer review (two reviewers, refute-first)
before touching the plan. This note records the comparison highlights and — importantly — the
**rejected** proposals with reasons, so they are not re-proposed.

## Comparison in one paragraph

PHPStan solves a differently-shaped problem (extensible analyzer, phpdoc gradual typing,
single-threaded no-shared-memory runtime), so most divergence is forced: freshly-allocated
immutable `Type` objects with *string-keyed* memoization (`describe()` as cache key) where
typokat has hash-consed `TypeId`s; a give-up `RecursionGuard` (re-entry → `ErrorType`) where
typokat has the assume-true cycle stack + provisional-cache soundness; bounded loop iteration
(3 passes + literal generalization) where typokat has the CFG fixpoint. Strong convergences:
reason-carrying relation results (`AcceptsResult`/`IsSuperTypeOfResult` with lazy reasons ≙
`Relation::No(ReasonChain)`), central union/intersection normalization (`TypeCombinator` ≙
canonicalize-before-intern, same quadratic-blowup worries), two named relations
(`isSuperTypeOf`/`accepts` ≙ `RelationKind`), stub files ≙ `lib.d.ts`, and fixture-driven
testing. One live confirmation: PHPStan *unions* template-type candidates across argument
positions (`TemplateTypeMap::union`) — exactly the policy backlog `65` identified as wrong
for tsc parity (fix-then-check). Their `Maybe` (TrinaryLogic) pervades everything because
phpdoc types are non-binding and levels gate reporting; tsc's model is binary and so is ours.

## Peer-review verdicts (2026-07-09)

| # | Proposal | Verdict | Why |
|---|---|---|---|
| A | Tri-state assignment certainty (Yes/Maybe/No) for definite assignment (`47`) | **REJECT** | PHPStan needs Maybe because it renders *two* diagnostics (undefined vs might-be-undefined). tsc has one code (TS2454, all-paths); `report iff ≠ Yes` makes the 3-point lattice extensionally identical to a boolean meet. Backlog `47` already specifies the all-paths walk on the existing CFG; a second forward-propagated state kind would cut against the single-narrowing-model invariant. |
| B | Expression-path narrowing keys (PHPStan keys scope on printed expression strings) | **REJECT** (no plan change) | Backlog `51` slice 2 already specifies the tsc-shaped version (`SymbolId` + property chain, tsc invalidation rules) and subsumes PHPStan's only importable detail (prefix invalidation on head writes). PHPStan additionally keys on *call expressions* (`count($a)` memory) — tsc does not narrow through calls; importing that keying is a dropped-error hazard. String keys would also forfeit interned integer comparison. |
| C | Inline type-assertion fixtures (PHPStan `assertType(...)`) | **NOTE → idea** | The observability gap is real (wrong-but-compatible inferred types are invisible; `65` was exactly that), but PHPStan's string-compare form is unsound here — typokat's type display is deliberately unstable (union order is intern-order dependent). Survives only as a `TypeId`-compare design: [`type-assertion-markers.md`](type-assertion-markers.md). |
| D | Result cache: file hash + inverted dep graph + exported-signature cascade cutoff | **ADOPT (amended)** | Production-proven shape, and it exercises the reserved blake3 stable hash early — but PHPStan's `ExportedNode` cutoff is *syntactic*, which is unsound for TS (body edits change inferred export types). The cutoff must be semantic (hash the **checked** export surface) with worklist-to-fixpoint propagation; the primary reference is tsc's own `--incremental` builder (`.tsbuildinfo`), PHPStan is corroboration. Folded into backlog [`17`](../backlog/17-incrementality.md) as Stage A. |
| E | Lazy on-demand stdlib resolution as a `lib.d.ts` fallback | **REJECT** | No plausible cost problem (tsc/tsgo parse all lib files eagerly; oxc parse of a few MB is ms-scale, and the frozen store is serializable via the §3.2 hash). Worse, it conflicts with Stage 1: a resolve-on-demand prelude is a *growing* shared store during checking — the exact §3.4 knot the frozen prelude eliminates. Declaration merging also makes per-name lazy resolution ill-defined for ambients. |
| F | Striped job scheduling (size-desc round-robin batches) | **REJECT** | An artifact of PHPStan's fixed-batch process model (no work stealing once a batch is composed). Rayon steals at per-file granularity, so the failure mode doesn't exist. The only residue is textbook LPT (largest-first ordering against a heavy-file-starts-last tail) — a one-line `sort_by_key` if profiling ever shows it; needs no reservation. |

## Exits

- **Graduated:** D → backlog `17` (Stage A amendment, same change as this note's commit).
- **Open idea:** C → [`type-assertion-markers.md`](type-assertion-markers.md).
- **Closed:** A, B, E, F — rejected above with reasons; delete this note when C and the
  `17` wording have no more use for the context.
