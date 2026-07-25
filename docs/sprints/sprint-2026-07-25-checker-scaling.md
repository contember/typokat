# Sprint — checker scaling on real code (2026-07-25)

**Goal.** Remove the quadratic and exponential terms that make typokat lose to native TypeScript 7
on multi-file projects, generic-heavy code, and failing comparisons — and leave behind guards that
make the next instance fail a test instead of a benchmark.

**Theme.** A benchmark sweep against `tsgo` found typokat **wins** relation, shape, errors,
typelevel and cold start, with a *lower* growth exponent than tsgo in each — and loses three rows
badly: `modules` 665×, `generics` 17.7×, `flow` 3.9×. Six independent complexity hunts plus a bisect
converged on five distinct nonlinearities, none of them in the type model. Four of the five are
"scan a whole-project collection once per item"; the fifth re-derives failing relations. This sprint
is the batch that removes them. The success condition is not a target number — it is that every
family's *exponent* is at or below tsgo's, and that a regression of this class cannot land silently
again.

The precipitating fact: on the identical 6,249-file corpus, the committed report
`tooling/bench/report/raw/20260709-174055/modules-100000.json` records typokat at **0.3068 s**
against tsgo's 0.3741 s. Today the same corpus takes **188.72 s**. We did not fail to reach this
performance — we had it and lost it, over nine days of active perf work, with nothing to catch it.

## Refs re-verified at HEAD (2026-07-25)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `check_project` is serial by design — one shared type universe on a single `CHECK_STACK_SIZE`
  worker — `src/driver.rs:160-187`. `check_files` (`src/driver.rs:143`) is the only `rayon` site and
  has no cross-file resolution, so the CLI never reaches it. Measured: every typokat benchmark row is
  exactly 1.00 cores; tsgo uses 6.67 on `modules-100000`.
- ✔ Import resolution is O(1) per import via a `path_to_index` map (`src/driver.rs:~312-320`), and an
  **import-free** corpus is equally quadratic — so cross-file resolution is not the problem.
- ✔ `try_add_library_modules` (`src/binder/bind.rs:1317-1355`) already does the correct thing:
  collect per file, `finalize_namespace_metadata` **once** after the loop, then fill. The project path
  does not. It is the in-repo template for WU2.
- ✔ `ecb179e` "perf: index lexical owner lookups" (2026-07-23) is the in-repo template for the
  index-don't-rebuild fixes: maintain `by_source` indexes incrementally at reservation time.
- ⚠ `src/types/layered.rs:332` instruments `iter()` with `record_full_view_base_scan_for_test` — the
  **sealed library base**. `local_iter()` at `:339`, the growing user-project layer that every one of
  these quadratics scans, has **no probe**. This is why six instances landed unseen.
- ⚠ ADR-0008 authorized the compilation-wide `LexicalReservations` table with **no** complexity or
  scaling consideration recorded.
- ⚠ The packaged `src/library/typescript-6.0.3/canonical.snapshot` was last regenerated at `90ff28d`,
  before the 2026-07-24 optimization batch. Four `library::*` specs fail at HEAD as a result; this is
  a known open item, not a regression from this sprint.

## Work units

### WU1 — indexed placement-syntax lookups (effort M)

- **Problem.** `set_placement_syntax` (`src/binder/namespace.rs:6036`) and `placement_syntax` (`:6053`)
  scan every placement bucket and participant in the project to find the participant
  `push_placement` inserted one line earlier; `DeclId` is already a dense index. A third scan of the
  same shape sits at `:5020` (the `namespace_fragment` back-patch). Θ(D²).
- **Verify first.** RED guard `placement_syntax_probes_scale_with_declarations_not_with_every_placement`
  — committed `53b7916`, failing with 63× probes for 8× declarations, each path landing on D²/4.
- **Scope.** Return the `MergeKey`/index from `push_placement`; add a `DeclId → (bucket, index)` side
  map for the read path; cover the `:5020` scan. Respect the sealed-base/local split — the index must
  not flatten them.
- **Acceptance / witness.** The guard's durable bound (≤ 4 probes per declaration) holds; conformance
  green; before/after wall clock on `shape-100000.ts` (15.7 s, of which 13.9 s is this) and a 624-file
  `modules-10000`.
- **Touch points.** `src/binder/namespace.rs`, possibly `src/binder/bind.rs`.

### WU2 — hoist project namespace finalization out of the per-module loop (effort M)

- **Problem.** `NamespaceTable::classify` (`src/binder/namespace.rs:2023`) clears and rebuilds every
  canonical index over the **whole accumulated project**, and runs once per module via
  `add_module` → `bind_namespace_metadata` → `finalize_namespace_metadata`. Instrumented on the
  6,249-file corpus: **177.43 s of 189.73 s (93.5%)**, 195,243,775 placements re-processed for ~62,495
  declarations — 3,124× amplification. Decisive control: at constant program size (8,000 declarations)
  varying only the file split, time = 135 ms + 1.85 ms × M.
- **Verify first.** Reproduce the constant-size/varying-split control before touching anything; it is
  the cleanest proof and the cleanest regression test.
- **Scope.** Adopt the `try_add_library_modules` shape verbatim: collect per module, finalize once
  after the loop, fill attachments in a second pass. Subsumes
  `fill_namespace_value_attachments` (5.7%) and `resolve_local_ambient_export_alias_targets`.
  `classify` also *sorts* fragments, so an incremental variant needs per-namespace dirty tracking —
  and current callers rely on the rebuild as a freshness barrier.
- **Acceptance / witness.** A guard on the constant-size/varying-split shape; conformance green;
  `modules-100000` before/after. Projection with WU1: ~190 s → ~5.5 s.
- **Touch points.** `src/binder/namespace.rs`, `src/binder/bind.rs`, `src/check/checker/mod.rs`
  (the per-unit loop).

### WU3 — working-set-local query transactions (effort L)

- **Problem.** `SemanticQueryState::fork` is `self.clone()` (`src/check/query/mod.rs:400`), deep-copying
  seven monotonically growing tables on every transaction, at five call sites. Θ(N²) in time and the
  cause of roughly half the peak RSS. On `generics-100000`: fork 28.5% + drop 23.4%, **only 7.14% real
  work** — ~19 s of a 37 s run copying and freeing a memo.
- **Verify first.** RED guard `semantic_query_transactions_copy_only_their_own_working_set` — committed
  `c58b6ca`. Doubling the program doubles transactions and planner visits but **quadruples** copied
  entries (339,226 → 1,358,526).
- **Scope.** Undo journal for six maps (layering them would push `&FxHashMap` lifetime churn through
  `eval/`); internally layered `RelationCache` in `src/relate/cache.rs` for the seventh, which is 39.6%
  of the copy volume and whose writes are invisible from `src/check/query/`. Signatures stay identical
  so `src/relate/relation/*.rs` needs no edits.
- **Acceptance / witness.** Guard passes; conformance green; `generics-100000` before/after (37.09 s /
  558 MB baseline); **plus** observed max/median `RelationCache` chain depth and lookup throughput
  before/after — §6.1 calls this cache possibly the largest single perf element, so a chain walk must be
  measured, not assumed.
- **Touch points.** `src/check/query/`, `src/check/checker/expr.rs`, `calls.rs`, `src/check/infer/`,
  `src/relate/cache.rs`.

### WU4 — reason-free relation probes (effort M) → [ADR-0016](../decisions/0016-reason-free-relation-probes.md)

- **Problem.** A cached `false` is never a memo hit: the engine re-derives the whole failing subtree to
  rebuild a `ReasonChain` the three-word cache cannot store. Every OR rule that keeps probing after a
  failure becomes a branch point → `a^d`. An ordinary nested discriminated union with one wrong leaf
  costs 1.14 s at ten levels and 16.8 s at twelve, on 24- and 28-line files. At depth 8, 1,251 of 1,277
  frames rebuild a verdict the cache already holds.
- **Verify first.** RED guard + four reporting pins — committed `0a0733c`.
- **Scope.** Thread `want_reason` through `Relater::relate`; return a shared leaf on a cached `false`
  when the caller discards reasons. **The audit of every call site is the real work**, not the flag —
  a missed one is a silently degraded diagnostic, not a test failure.
- **Acceptance / witness.** Guard passes, four reporting pins byte-identical, conformance green,
  arm-sweep and Redux-shape re-measured. Amend architecture §6.1/§6.4 and `invariants.md` §1;
  flip ADR-0016 to accepted.
- **Touch points.** `src/relate/` except `cache.rs`, `docs/reference/architecture.md`,
  `docs/reference/invariants.md`.

### WU5 — local-layer scan probe + declaration owner ranges (effort M)

- **Problem.** Two things, deliberately paired: the missing `local_iter()` probe (see Refs), and
  `attach_type_decl_owners` (`src/check/checker/mod.rs:2441`), which filters **all** project
  declarations once per module — Θ(modules × declarations). The latter is ~0.5% today but projects to
  ~33% of the residual once WU1–WU3 land.
- **Verify first.** The probe must fail on the real offender, not a synthetic one.
- **Scope.** Add the local-layer probe as a matched pair with the base probe; record per-module
  `[start, end)` `DeclId` ranges and iterate the slice. Note `attach_type_decl_owner` deliberately takes
  the **last** declaration for merges while `ecb179e` uses first-wins `.or_insert` — preserve what each
  site needs.
- **Acceptance / witness.** A deliberately re-introduced per-item project scan is caught by the probe
  assertions, not by wall clock.
- **Touch points.** `src/types/layered.rs`, `src/check/checker/mod.rs`.

### WU6 — memoize the contextual argument re-walk (effort M)

- **Problem.** `infer_contextual_source_after_walked` (`src/check/checker/expr.rs:821`) re-walks the
  whole argument expression under a contextual type with no memo, three times per call level, plus once
  per overload candidate and rest-arity alternative. Measured **×2.97 per nesting level** — depth 12 is
  1.35 s and depth 14 is **11.98 s on a 14-line file**. The control pins the exponent precisely:
  arguments that are neither fresh literals nor arrows skip the re-walk and stay flat to depth 20.
  Nested callbacks and config literals at depth 8–12 are everyday TypeScript; this is an editor hang.
- **Verify first.** A deterministic counter guard on walk count vs nesting depth.
- **Scope.** Memoize the contextual walk per `(expression span, contextual TypeId)` for the lifetime of
  the enclosing call check, so the inference re-walk, the committed re-walk and each candidate trial
  reuse one result. Collapses base 3 → base 1.
- **Acceptance / witness.** Guard passes; conformance green; the depth sweep flat.
- **Touch points.** `src/check/checker/expr.rs`, `src/check/checker/calls.rs`.

## Out of scope (explicit)

- **Parallelism.** Everything runs on 1.00 cores and tsgo uses up to 6.67, but parse (0.14%) and
  per-file bind (1.12%) are the only embarrassingly parallel phases here; the rest shares one mutable
  universe. Ceiling ≤7× against quadratics worth ~100×. Revisit only after this sprint.
- **The type store, interner, hash-consing, substitution, evaluator.** Measured **under 2% combined**
  and explicitly ruled out — the `condfan` control (3¹⁶ unmemoized evaluations would be 43 M) is flat at
  2.3–2.6 ms. Do not optimize here.
- **Diagnostics rendering.** Producing 54,000 diagnostics costs 0 s against a zero-diagnostic twin; the
  whole module is 0.71% of a 100k-line run.
- **Regenerating `canonical.snapshot`** — needs a clean committed tree; do it once the batch lands, not
  between commits.
- Filed instead of done here: backlog `86` (free-param summary base reset), `87` (unbounded reason
  chain), `88` (symbol declaration re-sort — a `lib.d.ts` blocker), `89` (the guard rails), `90`
  (assignability span precision).

## Decisions

- **[ADR-0016](../decisions/0016-reason-free-relation-probes.md)** — amend the "never a bare `bool`"
  pillar with a reason-free probe mode. Alternatives rejected: caching the `ReasonChain` (undoes §6.1's
  three-`u32` cheapness for reasons discarded 98% of the time) and tsc-style two-pass `reportErrors`
  (pass two *is* today's behaviour and reproduces the same exponential).
- **Fix forward, do not revert.** The curve is non-monotone — 0.031 s → 0.24 s (07-14) → 23.3 s (07-16)
  → 0.94 s at HEAD on the 624-file corpus. This is a recurring pattern across many commits, not one bad
  change, and `a7923b6`'s own defect was already fixed by `ecb179e`.
- **`src/relate/cache.rs` belongs to WU3**, the rest of `src/relate/` to WU4, so the two run
  concurrently. WU3 keeps `RelationCache`'s method signatures identical; that property is what makes the
  split safe and must not be traded away.
- **Soundness outranks the target.** Where a change would let a transaction's speculative entry survive
  rollback, or would expose an entry the clone-based fork would not have seen, stop and report. The
  §6.3 cache-soundness discipline is untouched by every WU here.

## Sequencing

| WU | files | may run with |
|---|---|---|
| WU1 | `binder/namespace.rs`, `binder/bind.rs` | WU3, WU4, WU5 |
| WU2 | same as WU1 | after WU1 |
| WU3 | `check/query/`, `expr.rs`, `calls.rs`, `infer/`, `relate/cache.rs` | WU1, WU4, WU5 |
| WU4 | `relate/` except `cache.rs` | WU1, WU3, WU5 |
| WU5 | `types/layered.rs`, `checker/mod.rs` | WU1, WU3, WU4 |
| WU6 | `expr.rs`, `calls.rs` | after WU3 |

Every WU is spec-first: the RED guard is committed on its own before the fix, per
[dev-method](../reference/dev-method.md) §1.

## Run log

### 2026-07-25 — measurement, hunts, and the RED batch

- Benchmark sweep vs `tsgo` 7.0.1-rc (direct native ELF, not the node shim), hyperfine, `--noEmit
  --noLib --skipLibCheck` both sides. Wins: relation 0.28, shape 0.39, errors 0.41, typelevel 0.56,
  startup 0.46 — each with a **lower exponent than tsgo**, so the margin grows with size. The `errors`
  win is not from checking less: typokat emits 33,984 diagnostics to tsgo's 31,985. Losses: flow 3.94,
  generics 17.67, modules 665.
- Six parallel complexity hunts (project pipeline, binder, relation, checker/inference, type store,
  diagnostics). Four converged on the same two binder quadratics from different directions; type store
  and diagnostics returned clean negatives with controls strong enough to close those areas.
- Bisect: first bad commit `a7923b6` (2026-07-14), boundary proven by build-input equivalence
  (`d5c73eb` 0.0330 s → `b6aa825` 0.2403 s, md5-verified identical corpus). Cause was unindexed
  project-wide `owner_at` scans. **Diagnostics identical to the pre-regression build** — zero gained or
  lost, so a pure perf regression; the sole behavioural delta (assignability column precision) → backlog
  `90`.
- RED guards committed: `53b7916` (WU1), `c58b6ca` (WU3), `0a0733c` (WU4 + ADR-0016). Each reproduced by
  the leader before committing.
- WU3 hit a scope boundary and stopped rather than working around it: `relation_cache` is 39.6% of the
  copy volume and its writes are invisible from `src/check/query/` because the coordinator hands the
  cache to the relater by value and `RelationCache` exposes no iterator. Resolved by granting
  `src/relate/cache.rs` to WU3. The agent declined the available shortcut — letting relation-cache
  entries survive rollback — on the grounds that `invariants.md` §1 forbids it, and flagged that
  `src/check/query/mod.rs:1478` assigns the cache back **unconditionally**, which does not obviously
  agree with that invariant. Routed to WU4 for a verdict; if it is a real hole it gets its own item.
- Second RED pair committed: `9638cb8` (WU2), `5704075` (WU6), both reproduced by the leader first.
  Two exact laws replaced the sprint's estimates. **WU2**: at constant program size, every
  finalization counter is `rows x (M+1)/2` — one full-project replay per module, 7.22x growth for an
  8x finer split. **WU6**: the contextual re-walk is `(3^d - 1)/2` per phase and two phases fire, so
  `3^d - 1` total — base 3 exactly, not the "~2.97" the wall clock suggested; both the arrow and the
  object-literal shape hit it independently.
- WU2 found the attachment fill does **not** fall out of the hoist — it gets worse. Today module *m*
  filters merges accumulated so far (`D x (M+1)/2`); hoisting makes all *M* second-pass fills see the
  full `D` (`D x M`), ~2x more rows, on the project and library paths alike. Net still a large win
  because classify's per-row cost dwarfs the filter, but the `Theta(modules x declarations)` term
  survives and needs its own index. Deliberately left out of the asserted bound rather than pinned to
  something the hoist cannot meet; to be filed as a backlog item once the post-fix numbers are in.
- `cargo fmt --check` is a CI gate (`.github/workflows/ci.yml`, pinned rustfmt via
  `rust-toolchain.toml`) and four files had drifted past it — two from `7ba2c01`, one from `904642f`,
  one from `cfe61fb`. Repaired in `0b41477`. Neither `cargo test` nor `cargo clippy` catches this, so
  it is worth running before handing back any batch.
- **WU6 cannot reach its target and stopped there.** Base 1 is unreachable without changing output:
  of the three walks per level, two *retain* effects and both emit, so one error nested `d` deep is
  reported `2^d` times where `tsc` reports it once (leader-verified at HEAD, and against
  `tsc 6.0.3 --strict`, at depths 1–5 on both shapes) → backlog `92`. Collapsing the walks would
  delete those duplicates, which is the diagnostic change WU6 was told to stop on; and serving a
  retaining walk from a memo needs effect replay, which `CheckerEffects` (not `Clone`) and
  ticket uniqueness (invariants §1) both block. A base-2 prototype exists — 70–83× at depth 14,
  byte-identical diagnostics over 465 fixtures × 2 formats and 8 bench corpora — held back because
  its memo state is a production `thread_local!` that belongs on `Pass` (`checker/mod.rs`, owned by
  WU2 until it lands).
- The WU6 guard as first written would have gone **green** on that base-2 prototype (`510/30 = 17`
  against a bound of 512). Agent caught it against its own work and reported rather than shipping.
  Retuned in `12b44e6` to bound the *ratio of ratios*, which is 1 for every polynomial and unbounded
  for every exponential; still red at HEAD, and red on the prototype.
- **`generics` is no longer a loss.** Leader-measured at HEAD `543d635`, median-of-5:
  `generics-100000` **2.064 s vs tsgo 2.108 s (0.98×)**, down from 36.45 s / 17.67× this morning —
  a 17.7× improvement from the landed WU1–WU5 batch. `generics-10000` is 0.181 s vs 0.139 s (1.30×).
  The 13.26 s figure recorded for WU3 above is superseded; WU4 and WU5 landed after it.
  (WU6 reported 0.50× — it timed tsgo at 4.24 s, 2× every other measurement of the same binary; the
  typokat side matched.)
- The `tsgo` at `/tmp/typescript-go-compare/.../bin/tsgo` is a **node shim that exits immediately**
  with `Executable not found`; timing it measures a crash. The committed reports are unaffected —
  they invoke `tooling/bench/.tools/node_modules/@typescript/typescript-linux-x64/lib/tsc`, the
  native TypeScript 7 binary, with exit code 0 recorded in every result.
- WU6's "unrelated" panic on `b43_namespaces_declaration_merging/slot_shadowing.ts` is already
  filed as backlog `84`, which names that fixture; not re-filed.
- **WU2 landed (`202f0bc`): `modules-100000` 188.72 s → 6.74 s, 28×**, leader-measured median-of-3,
  with `modules-40000` 32.7× and `modules-10000` 9.2×. Diagnostics unmoved — leader-diffed full
  rendered output over all 465 fixtures (0 differing) and `errors-10000.ts`. Hazard the agent found
  and fixed: `continuation_publication` was cleared at the end of `add_module`, but the deferred fill
  reads it, so deferring naively would have published continuation declarations to the lexical scope
  instead of the compilation global.
- **The modules target is still not met: tsgo runs the same corpus in 0.307 s, so we are 22× off**,
  and the 2026-07-09 baseline of 0.3068 s stands unbeaten. The after-curve exponent is ~1.0 to 600
  files, 1.41 at 2,499 and 2.59 from 2,499→6,249, so a superlinear term survives. The measured
  `Θ(modules × declarations)` attachment scan is ~1.5 s of the 6.74 s → backlog `93`; **the rest of
  that knee is unattributed and needs its own hunt.**
- **WU6's prototype is deliberately not landed.** Its own realistic-shape study killed the case: the
  memo fires *zero* times on zod-style builders (`object<T>(shape: T)` — a bare type variable never
  triggers the candidate re-walk, so those are base 2 already) and on `describe`/`it` (non-generic,
  no inference phase). It only helps structurally-embedded generics (`map`/`then`/`pipe`), which nest
  3–5 deep where the cost is 2.8–8 ms; the 70–83× lands at depth 12–14, where real code is not. The
  one realistic file that does hang — 40 schemas at depth 12, 525 lines, 1.11 s — is 1.12 s after.
  Adding memo machinery to a hot path for no measured gain, in code backlog `92` will restructure,
  is not worth it. Patch and the full discriminator table preserved in backlog `92`.
- Agent-reported numbers that did not survive leader re-measurement, both from contention rather than
  error: WU6's tsgo figure (4.24 s vs 2.09 s true, since retracted by the agent on its own re-run)
  and WU2's `modules-100000` (7.06 s vs 6.74 s, within noise). Measuring while a sibling WU runs a
  6,249-file corpus is not measuring.
- **The knee hunt closed the question: it is one term, and it is backlog `93`.** Skipping the
  attachment fill takes `modules-100000` from 6.86 s to **0.977 s — 86 % of wall clock** — and
  flattens the residual exponent to 1.03–1.12 out to 12,000 files, with every other phase scaling
  4.9–6.6× for 6.25× more files. The counter reads `fills=6249 rows=390,531,255 targets=0`: 390
  million rows producing nothing on this corpus. Leader-reproduced.
- **`93` was filed 4× too small and is corrected.** The original ~1.5 s came from measuring
  0.246 ms/module at `D = 37,500` and extrapolating linearly to 6,249 modules — across a cache
  boundary. Per-row cost is itself a function of `D` (4.53 → 21.57 ns/row from `D = 24 k` to 192 k, as
  the ~190 B/merge table leaves a 16 MiB L3), so the real per-module cost at `D = 62,474` is 0.903 ms
  and the term is 5.88 s. The lesson generalises: never extend a per-unit cost linearly without
  quoting it at the target size.
- **`93` is not a benchmark artifact — real code is hit harder.** At a realistic 30 declarations/file
  the fill is already 60 % of a 2,000-file project, and 70 % on a namespace-using corpus. The bench
  corpus has zero namespaces, so it pays only the scan and never the `M`-fold target re-derivation.
- **`93` landed (`c8fc029`): `modules-100000` 6.729 s → 0.997 s, 6.75×**, exponent 2.54 → **1.07**;
  `attachment_merge_rows` 4,608/36,864 → 576/576. Against tsgo the family goes **22× → 3.4×**.
  Leader-measured interleaved median-of-5; diagnostics leader-diffed over 465 fixtures single-file,
  122 case directories as projects in both file orders, four cross-file merge corpora in both
  orders, and `errors-10000.ts` — zero differing.
- The agent improved on the briefed design and I adopted it: build the target list **whole**, then
  partition its *application* by module, rather than narrowing the merge scan per module. Since
  `202f0bc` froze the merge vector for the fill loop, one global sort/dedup is byte-identical to
  what every fill built, so the winner cannot move — hazard 2 becomes structurally impossible
  instead of something to reason about.
- **An order-sensitive write the plan alone would have broken.** `symbol.value` is shared by
  same-named members in different files, and the two paths already disagreed at HEAD: the project
  path took the globally last declaration by span, the library path the last module's. A plain
  partition silently switches the project path to file order. Preserved by replaying the plan once
  after the loop, gated on the same flag that decided whether the old fill filtered. Found by
  measuring the existing behaviour before changing it, not by reading the code — and it is **not
  reachable through the checker today** (two files declaring the same namespace value member report
  an incomplete surface), so the byte-diff could never have caught it. The unit guard is the net.
- **`92` landed (`243a878`): one diagnostic per nested contextual error, down from `2^d`.** The
  committed walk now reports and the raw walk is held, discarded only when superseded. Leader-verified
  classification over 469 fixtures: 4,758 → 2,750 diagnostics, 2,008 removed, **the set of distinct
  diagnostics unchanged — 0 lost, 0 added** — and only the four new `b92` fixtures shrink, so the other
  465 are byte-identical. Seven bench corpora identical, official suite 0 regressions.
- Two things made that safe rather than lucky. The agent kept the raw walk **provisional** instead of
  deleting it, having found it is the only walk in nine distinct shapes, and pinned every one of them
  in `retained_raw_walks.ts` — the obvious fix (drop the raw walk) deletes all of it. And it needed no
  event/ticket change after all: a held batch merges *at most once*, so no ticket is completed twice.
  The blocker recorded in `92` was for *replaying* a batch, which holding does not do.
  `invariants.md` §1 now says this explicitly — the line is *when* a batch reaches an owner, not
  *whether* — because "held and maybe discarded" reads close to the post-hoc suppression it forbids.
- What remains of `92` is the walk count, split out as `95`, and it is now unblocked for the first
  time: with only one walk retaining, the other two discard, so a memo over them returns only
  `(TypeId, Span)` and replay equivalence holds by construction. The WU6 prototype becomes applicable
  to *both* of them rather than one.
- **`94` bisected (2026-07-26).** Four steps, all eager per-declaration substrate built before
  anything consumes it: `a7923b6` lexical-event preallocation (11.4 KB/file), `b6ecfa4` +`fe61867`
  declaration/namespace metadata walks (15.1 KB/file combined), plus a diffuse 7.7 KB/file tail.
  Endpoints leader-reproduced today: `f065e89` gives **0.30 s / 154 MB**, HEAD 0.997 s / 372 MB, same
  corpus. Both substrates must be fixed — neither alone covers the 165 MB gap. Details in `94`.
- Two methodological results worth keeping. **Peak RSS is the right bisect oracle on a busy box** —
  near-deterministic where wall clock is not — **but it quantizes by ±5 MB**, so a step must be
  confirmed at two or more corpus sizes; `6c82ead` looked like a +4.7 MB step at one size and is zero
  at three others. And **RSS and wall clock disagreeing is information**: here it proved two
  independent regressions had been sitting on top of each other, since `a7923b6` carried both the
  memory constant and the attachment-fill exponent, making wall clock unattributable between 07-14
  and 07-25 at any corpus size.
- This also sharpens backlog `89`: the 07-13 sprint's ≤2 % regression gate never measured a
  multi-file corpus, which is exactly how a 3× multi-file constant walked in unnoticed.
- **Underneath the exponent is a flat ~3× regression → backlog `94`.** Post-`93` projection is
  0.98 s against typokat's own committed **0.3068 s of 2026-07-09** on the identical corpus (157 vs
  49 µs/file), with peak RSS **372 MB against 159.9 MB** — leader-verified against the committed
  report, which also shows typokat *beating* tsgo that day (0.3068 vs 0.3741). It is spread evenly
  across eight linear phases with no dominator, so it needs a bisect, not a hunt. **`93` gets the
  exponent, not the target.**
