# Dev method — how work is done here

`typokat` is a from-scratch TypeScript type checker in Rust. **M0–M33** are done (see
[`README.md`](../../README.md)). This document is the **method** to follow when continuing the
project — the build loop that kept it sound, and the bug classes the reviews keep catching. The
**invariants you must not break** live in [`invariants.md`](./invariants.md); the **next work** is in
[`../backlog/`](../backlog/README.md).

Read first: [`README.md`](../../README.md) (what exists),
[`architecture.md`](./architecture.md) (the design + §12 phased plan),
[`../archive/mvp-plan.md`](../archive/mvp-plan.md) (how M0–M6 were scoped),
[`tests/cases/README.md`](../../tests/cases/README.md) (the conformance/marker conventions) and
[`divergences.md`](./divergences.md) (every documented `tsc` divergence + deferred check).

For each milestone you pick up, **prepare a per-milestone plan the same way it was built so far** —
spec first, then implement, then independently review.

---

## 1. The method (follow this exactly — it is what made the project sound)

The build loop, per milestone `Mn`:

1. **Leader writes the spec first** = the fixture corpus. Create `tests/cases/mN_<topic>/*.ts` with
   inline `// error[TK…]: <substring>` markers (conventions in `tests/cases/README.md`). The
   fixtures are the acceptance spec and must be written **independently of** the implementation, so
   the review stays honest. Pick fixtures robust to subtle `tsc` choices (e.g. avoid depending on
   literal-widening; keep at most one mismatched argument per call). Update `tests/cases/README.md`
   (milestone index, any new `TK` code) and record any new deferred check / `tsc` divergence in
   [`divergences.md`](./divergences.md). **Commit the spec on its own**
   (`"Add Mn … corpus (spec)"`) — it does not change behavior because the dir is not yet enabled in
   `MILESTONE_DIRS`.
2. **Dispatch an implementation subagent** (`Agent`, general-purpose). Give it: the fixtures, the
   relevant architecture sections, which existing code to REUSE, the exact scope, the explicit
   **deferred** list, and the invariants ([`invariants.md`](./invariants.md)). It must leave
   `cargo build`/`clippy`/`cargo test` green and enable `"mN_<topic>"` in `MILESTONE_DIRS`. It must
   NOT commit.
3. **Dispatch an independent adversarial review subagent** (a *different* agent — independence is the
   point). Its job is to **hunt false negatives** (dropped errors — the worst outcome for a checker),
   verify no regression, confirm faithfulness, and **cross-check probes against real `tsc --strict`**
   (`tsc 6.0.x` is available). It returns **PASS/FAIL** with concrete repros.
4. **On FAIL**, route the fix back to the implementation agent via `SendMessage` (it keeps its
   context). For a soundness fix that touches the relation engine, re-review (via subagent) before
   committing; for a small prescribed fix, a gate-check is enough.
5. **Leader commits** once green. Verify yourself (`cargo test` + `clippy` + spot-run the fixtures)
   before committing — committing unverified code is the one thing the leader owns.

### Required extra gate: inference / contextual typing changes

**If the change touches inference, contextual typing, argument walking, or overload resolution, the
randomized differential corpus is a required gate — a work unit may not report "zero diff" without
it.** The fixture corpus is demonstrably blind to this region: `412f321` altered output on ~15 % of
randomly generated nested-contextual programs while showing zero diff across 471 fixtures in two
formats, project mode in both file orders, eight bench corpora, the official-suite ratchet, and a
2,193-binding inferred-type probe (backlog `96`).

Build the **pre-change** binary in a scratch worktree (never in the main one) and diff against it:

```sh
git worktree add --detach /tmp/pre <base-commit> && (cd /tmp/pre && cargo build --release)
cargo build --release
cd tooling/differential
python3 differential.py fuzz --ref /tmp/pre/target/release/typokat --count 400   # then also --seed 2, 3, …
python3 differential.py repros --check                                          # committed repros
```

Any difference is a finding: two builds have no licence to disagree unless the change intended it.
Each one is auto-shrunk to a minimal repro — adopt the interesting ones into
`tooling/differential/repros/` and re-record `scoreboard.txt` (`repros --save`) in the same commit.
Details, modes and the allowlist rules: [`tooling/differential/README.md`](../../tooling/differential/README.md).

You are the **leader/orchestrator**: you write specs, dispatch agents, review their results, and
commit. Delegate implementation **and** review to subagents; do not write implementation code
yourself. Run agents in the background (`run_in_background`/`SendMessage`) when waiting.

### Commit conventions (from the repo's CLAUDE.md)
- Atomic: `git add <explicit paths> && git commit -m …` in one call. **Never** `git add -A`/`.`;
  list files (a whole `src/` or `tests/cases/<dir>` path is fine when it's all intended).
- Spec commit and implementation commit are **separate**.
- End every commit message with the two trailers (`Co-Authored-By:` + `Claude-Session:`) — copy the
  format from `git log`.
- Never `git stash`/revert work that isn't yours.

---

## 2. Lessons / what the reviews kept catching (so your reviews hunt for it)

Independent review caught these classes of bug that implementation-side tests missed — look for them:

- **Cache poisoning under recursion** (M5): provisional cycle assumptions cached durably →
  order-dependent dropped errors. The deepest soundness trap.
- **A path that was sound when a feature was secondary but became a hole when it was promoted** (M13:
  `static` bodies were skipped while statics weren't members; M15: get-only accessors inherited the
  `readonly`-field constructor carve-out). When you promote something to a real member/type, re-check
  every place that special-cased it.
- **Excess/freshness not recursing** through a new container (M19: index-sig values).
- **Contextual typing too aggressive or not applied** (M18: literal-tuple widening; array literals
  hijacked vs not).

General: every review should write throwaway `.ts` in the scratchpad, run `cargo run -- check`, and
diff against `tsc --strict`. Classify each divergence as false-negative (must fix), false-positive
(usually acceptable, document it), or regression (must fix).

The **official TypeScript suite harness** (`tooling/official-suite/`) runs typokat against the real
microsoft/TypeScript conformance baselines at scale and is biased to surface false negatives. The
gaps it surfaced are filed as backlog items (the `F*` findings — see
[`../backlog/`](../backlog/README.md)).

The **randomized differential harness** (`tooling/differential/`) generates deep compositions of
contextually typed calls and diffs two checkers over them — a previous typokat binary (regressions)
or real `tsc --strict` (truth). It is the answer to a bug class every hand-written gate missed by
construction, and it is a **required** step for inference/contextual-typing work (§1 above).

- **A gate that cannot see a bug class is not evidence.** Five committed gates passed `412f321`
  because the trigger shape did not occur in the corpus. When a review reports "no diff", ask what
  the gate *can* see before believing it.

---

## 3. Picking the next milestone

The roadmap **is** the [`../backlog/`](../backlog/README.md): each milestone and each known gap is
its own item, ordered roughly by value and dependency. Architecture §12 governs ordering — the
relation engine + narrowing come **before** type-level evaluation, never the reverse; that
evaluation is tree-walked, with the bytecode VM a deferred, profiling-gated refactor
([ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)).

The backlog README carries the **definition of done (checker 1.0)**, the four tracks (A model
completeness → unblocks `lib.d.ts`; B checker completeness; C known-gap tail; D scale ladder), and
the **recommended order** — don't duplicate that ordering here; read it there. Items `01`–`12`,
`20`, `28`–`29`, and `31` are shipped/closed; do not plan new work from those deleted backlog
numbers.

For each item you pick: write its fixture corpus first, update `tests/cases/README.md` (and
[`divergences.md`](./divergences.md) for any new divergence/deferral), commit the
spec, dispatch the implementation subagent, dispatch the independent review, fix, and commit. Same
loop, all the way up.
