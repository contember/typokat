# SOTA checker survey — comparison + reviewed lessons

Survey note, 2026-07-09. Web research across tsgo (typescript-go), ty (Astral), Pyrefly
(Meta), Flow types-first, Sorbet, Hack (hh_server), rustc's next trait solver, ezno,
Swift's constraint solver, Biome/oxc type-aware linting, and the academic literature
(recursive/semantic subtyping, algebraic subtyping, occurrence typing, incremental
computation). Tier-1 candidates went through independent adversarial peer review (three
refute-first reviewers grounded in the actual code) before touching the plan. Companion
note: [`phpstan-architecture-lessons.md`](phpstan-architecture-lessons.md).

## Peer-review verdicts (2026-07-09)

| # | Proposal (source) | Verdict | Why |
|---|---|---|---|
| 1a | Two-tier provisional/global relation cache (rustc next solver; GLP ICFP 2000) | **NOTE — profiling-gated** | Perf premise defeated today: `Relater` + its durable cache are constructed fresh at ~29 call sites (per statement/call/inference step), so a provisional cache would optimize the innermost sliver while the §6.2 cache-lifetime strategy — the order-of-magnitude lever — is still unimplemented (`crates/typokat-relate/src/relate/cache.rs` TODO). Intra-cycle keys recompute ~twice, not O(cycle). rustc's own provisional-reuse rules needed fuzz-fixes (rust PR #128828); the current 14-line auditable commit site with the signature-threaded `assumed` set is the safer structure. Revisit only if profiling **after §6.2 lands** shows intra-cycle recomputation hot. |
| 1b | Fixpoint iteration on cycle heads (rustc) | **REJECT** | typokat's relation is purely coinductive with no side channels (no evaluation mid-relation, no inference vars, constraints recurse through the same stack) — one-shot assume-true **is** the gfp algorithm (Amadio–Cardelli), and durable verdicts are root-computed, hence entry-order-independent by construction. rustc iterates because it mixes inductive goals + constraint side-effects; we have neither. |
| 1c | Depth-tainted cache entries (rustc; same class as our b55) | **NOTE → invariants sentence** | Vacuous today — no depth/budget exists in `crates/typokat-relate/src/relate/` (TK2589 is evaluator-only, already b55-guarded). Becomes mandatory the day a relation depth limit is added; recorded as one sentence in `invariants.md` §1, not a backlog item. |
| G | Member-level diffable export surface for `17` Stage A (Hack shallow-decl diff, `typing_deps` member edges) | **REJECT** | Member granularity would *create* the negative-facts hazard (Hack's `NotSubtype` edges) that file-level surface-hash invalidation covers for free; dep *recording* would instrument the relation engine into the §6.3 bug class; structural typing (`keyof T`, mapped types) degenerates member edges to whole-type edges; Hack's economics are a resident daemon at 10⁸ LOC, not a batch cache at our scale. Cheap revisit if Stage A fanout measures badly: **per-export-name** hashes (aligned with named imports, positive-edge-safe). |
| H | Determinism under parallelism as explicit `16` acceptance (tsgo lesson) | **ADOPT** | tsgo shipped different *errors* across runs (encounter-order type IDs + ID-sorted unions) and needed content-based ordering. Ours has the same shape: union canonical order = run-local `TypeId` sort. Load-bearing corollary found in review: the **stable hash must define a content-based canonical member order** — hashing unions over TypeId-sorted members would make structurally identical types hash differently across workers, breaking cross-file identity itself. Amended into backlog `16`. |
| P | Polarity discipline (Simple-sub, Parreaux ICFP 2020) as theory backbone for candidate policy | **NOTE — with hard caveats** | Matches tsc only at candidate *collection* (`candidates`/`contraCandidates` in `inferTypes`) and in the signatureless conditional-`infer` path (`getTypeFromInference`: union covariant, **true intersection** of contraCandidates — the backlog `68` case, confirmed). At *fixing* time in the signature/call-site path tsc violates both poles: `getCommonSubtype`/`getCommonSupertype` select a single candidate (first-wins for incomparables), and b65 is a direct counterexample to joining covariant candidates. Parity target is checker.ts, not the lattice — the theory must never justify a "principled" nonconformant fix. Backlog `68` got the precise tsc-mechanism wording instead of the theory. |

## Reference shelf (recorded without review — pointers, not plan changes)

**Scale / incrementality era (items `14`–`17`):**
- Content-addressed persistent check results (Unison; tsc `.tsbuildinfo` is the practical shape) — key the Stage A cache on the stable hash so results survive sessions/machines.
- Pyrefly: interface extraction + **eviction** (keep only exported-symbol types, drop AST/bindings/answers per module) as the memory discipline at monorepo scale; **optimistic invalidation** of import cycles (recompute only the changed module against stale cycle-mates, escalate iff its interface changed).
- Flow types-first: signature-cutoff recheck skipping (validates Stage A); saved-state snapshots per commit.
- "Build Systems à la Carte" (ICFP 2018) vocabulary for the Stage A→B migration: early cutoff, verifying traces; Nominal Adapton (OOPSLA 2015) keyed cache slots.
- Cross-file identity consensus: **nobody shares a growing interner** — Hack/Flow serialize-don't-share (shared byte heap, per-consumer re-intern), Sorbet builds the symbol table sequentially + ID-substitution merges, tsgo duplicates per checker (~20% overhead, measured viable). Validates architecture §8.2's staging; tsgo's model is the measured plan B.

**LSP era (Phase 5+):**
- ty: multi-granularity Salsa queries (definition-level inference breaks import-cycle query cycles); `Divergent` widening type for non-convergent recursive inference; bind-eagerly/evaluate-lazily narrowing constraints.
- Sorbet: structure-hash fast path / cancelable-preemptible slow path; "delete file's symbols + re-enter" as the robust incremental unit.
- Pyrefly: transactional snapshot reads, single-writer commit.
- Warning: rust-analyzer's new-Salsa port caused a ~4× memory regression (rust-analyzer #19402) — benchmark memory first when Stage B lands.

**Relation / evaluator (profiling-gated):**
- Lazy-union BDD normal forms for union-heavy operations (Frisch/CDuce; Elixir in production 2025) — an internal engine for narrowing subtraction / emptiness / large discriminated unions, strictly behind the tsc-parity relation and display; never a replacement.
- Kozen–Palsberg–Schwartzbach O(n²) automata-product as a fallback for pathological recursive type pairs.
- Match types (POPL 2022): disjointness-gated branch commitment + fuel-not-termination-checker — independent validation of the evaluator's defer/commit and TK2589 budget design.
- Minimal-witness reason ranking (Pavlinovic OOPSLA 2014, as heuristic): when a relation fails many ways, report the cheapest reason chain, not the first.

**QA:**
- tsgo: fuzz the diagnostic/LS surface against a corpus of top real repos (20× failure reduction); `--singleThreaded` as a determinism escape hatch; a `--stableTypeOrdering`-style diff-reducer flag in the *reference* implementation when comparing baselines.

**Anti-patterns confirmed by others' pain (keep not doing):**
- Open constraint solving for overloads — Swift's solver is exponential on mundane code; tsc-style local directional inference is the escape.
- Interpreter-grade effect tracking — ezno is retreating to a `--basic` mode.
- Aggressive un-annotated inference without the gradual guarantee (Pyrefly/ty split) — moot for us (tsc defines the answer), but the same conservatism.
- Published theory on parallel type checking and efficient narrowing essentially does not exist — we are not behind any literature there.

## Exits

- **Graduated:** H → backlog `16` acceptance wording (+ the stable-hash member-order
  constraint); 1c → one sentence in `reference/invariants.md` §1; P's mechanism half →
  backlog `68` approach wording.
- **Closed:** 1b, G — rejected above with reasons.
- **Open (profiling-gated):** 1a (two-tier cache, after §6.2), BDD engine, KPS fallback,
  minimal-witness ranking — revisit with numbers; the reference shelf serves items
  `14`–`17` when they schedule. Delete this note when the shelf has been consumed by the
  scale sprints.
