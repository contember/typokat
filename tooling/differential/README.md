# Randomized differential corpus — black-box harness

Generates deep compositions of contextually typed TypeScript, runs **two checkers**
over each program, and reports every place they disagree — automatically reducing each
disagreement to a minimal repro.

It exists because the hand-written gates are blind by construction. `412f321` (an
unsound contextual-walk memo, reverted by `40540a1`) changed output on ~15 % of
randomly generated nested-contextual programs while showing **zero diff** across 471
fixtures in two formats, project mode in both file orders, eight bench corpora, the
official-suite ratchet, and a 2,193-binding inferred-type probe. The trigger shape —

> an argument a contextual re-walk can supersede (arrow / fresh object or array
> literal), nested inside a contextually typed callback, whose value depends on that
> callback's contextually typed parameter

— simply does not occur in `tests/cases`. A hand-written corpus covers the cases
someone thought of. The method consequence is written down in
[`docs/reference/dev-method.md`](../../docs/reference/dev-method.md) §1: for changes touching
inference or contextual typing, running this harness is required before reporting "zero diff".

## Why it can run while the checker is being refactored

Like `tooling/official-suite/`, the harness never imports or builds the `typokat`
crate. It shells out to a **prebuilt binary** (`typokat check --format compact <file>`)
and parses the rendered diagnostics. Point `--bin` at any binary; the default is
`target/release/typokat`. A work-in-progress `src/` does not block it.

## Three modes

```sh
cd tooling/differential

# (b) REGRESSION MODE — what a work unit runs. Any difference between two typokat
#     binaries is a finding. Build the pre-change binary first (a git worktree keeps
#     your tree untouched); point --ref at it.
python3 differential.py fuzz --ref /path/to/old/typokat --count 400

# (a) TRUTH MODE — what the project runs periodically. Compares against real tsc.
python3 differential.py fuzz --ref tsc --count 400

# (c) SELF-CONSISTENCY / SANITY — one binary against itself. Catches nondeterminism,
#     crashes, parse errors, and output inconsistent with the exit code. Runs in CI.
python3 differential.py fuzz --ref target/release/typokat --count 600 --check

# The committed minimal-repro corpus (needs neither tsc nor a second binary).
python3 differential.py repros --check      # identity ratchet; exit 1 on any change
python3 differential.py repros --save --refresh-tsc   # re-record, tsc baselines too
```

(`--ref none` is the same sweep without the second run: it still hard-fails on a crash,
a parse error or an inconsistent exit code, it just cannot see nondeterminism.)

Useful flags: `--seed N` (default 1), `--depth-min/--depth-max` (default 1–4),
`--time-budget SECONDS` (hard wall-clock cap — what makes CI bounded), `--adopt`
(write the shrunk repros straight into `repros/`), `--keep-work` (leave the generated
corpus in the gitignored `work/`), `--no-shrink`, `--check` (exit 1 on divergence).

> **Build a current binary first.** `fuzz` measures whatever binary you point at. A
> stale `target/release/typokat` silently reports old behaviour.

## The generator (`grammar.py`)

Narrow and deep, not broad and shallow. It composes a handful of constructs to
arbitrary depth: contextually typed callbacks (plain / overloaded / generic / with a
non-`void` return), arrow arguments (including generic `<U,>() => …`), fresh object and
array literals, overloaded and generic consumers (`shapeOf<T>(shape: T)` — the real
`zod` shape), projections out of the callback parameter (`p0`, `p0.a`, `p0[0]`,
`p0 + 1`), and an optional `class` / `this` context. Roughly half the leaves are
deliberately type-mismatched, so the trigger actually produces a visible verdict.

Everything is a tree of dataclasses, never text. That buys structural shrinking and
exact reproducibility: `generate(seed, index)` is a pure function (seeded through
`random.Random(str)`, so `PYTHONHASHSEED` does not matter). Top-level names are
prefixed `g<seed>_<index>_` so a whole batch can go to **one** `tsc` process — tsc
costs ~1 s of start-up per invocation and would otherwise dominate the run. Typokat is
invoked per file (~4 ms).

A unit test asserts the grammar still reaches the trigger shape in >70 % of programs.
If that ever fails, the harness is blind again and its "zero diff" means nothing.

## Shrinking (`shrink.py`)

The review's original finding became actionable only when it reduced to three lines
with no generics. Every finding here is reduced the same way, automatically:

1. **Structural** — greedy delta-debugging over the generated tree: drop a statement,
   splice out a nesting level, drop an overload signature, de-genericise an arrow,
   shorten a projection, drop the `class` wrapper, prune declarations the body stopped
   calling.
2. **Textual** — strip the origin comment, remove the name prefix, drop any line that
   turns out not to matter (this layer also handles hand-written inputs).

The oracle answers *does this source still show the **same** divergence?* — the reduced
program's signature must stay a non-empty subset of the original's, or the shrinker
happily wanders off to an unrelated disagreement and reports it as the minimal repro of
the first one. Candidates that dangle a reference (`TK2304`/`TK2339` — the reduction
deleted a declaration the body still uses) are rejected outright: that is a hole, not a
repro.

`differential.py shrink --seed N --index M --ref …` re-runs the reduction for one
program; `--file X.ts` shrinks an arbitrary file (textual layer only).

## What counts as a difference

**Reference-binary mode** compares the FULL diagnostic — line, column, code, message
and reason chain — plus the exit code and any `incomplete[…]` records. Two typokat
builds have no licence to disagree about anything, and `412f321`'s symptoms included
*wrong types rendered in messages*, not only dropped and invented diagnostics.
Findings are classified `dropped:` (the scary one), `invented:`, `message:`, `exit:`,
`incomplete-gone:`/`incomplete-new:`.

**tsc mode** compares at (line, code-number), the same robust contract the official
suite uses: columns and message wording diverge by design. Codes map by number
(`TK2322` == `TS2322`).

## `allowlist.txt` — committed, and deliberately hard to abuse

typokat is a deliberate subset of tsc, so a raw tsc diff drowns the signal. Each
allowlist line is a **cancellation rule**, `tk[<code>]~ts[<code>]` (either side may be
empty), applied per source line to the codes only one side reported. Three properties
matter:

- A rule cancels **one code pair**, never a line and never a file. A line carrying an
  allowlisted divergence *and* a new one still reports the new one.
- A near neighbour is not covered: `tk[2345]~ts[2322]` says nothing about
  `tk[2345]~ts[2769]`.
- Every entry names an owner, **validated on load**: `div:<id>` must match a real
  `<!-- div: id=… -->` marker in [`docs/reference/divergences.md`](../../docs/reference/divergences.md),
  `backlog:<NN>` a live item in `docs/backlog/`. An entry cannot exist without a
  documented reason someone owns.

Rule hits are printed with their counts, and rules that matched nothing are flagged as
stale — so the file cannot quietly grow past what it explains. The allowlist applies to
`--ref tsc` **only**; in reference-binary mode every difference is a finding.

## `repros/` + `scoreboard.txt` — the committed corpus

A finding is worthless as "seed 42 file 317". Every reported divergence is written out
as a minimal `.ts` file with a provenance header; the good ones are committed to
`repros/` and their behaviour is pinned in `scoreboard.txt`:

```
status<TAB>typokat-ids<TAB>tsc-ids<TAB>signature<TAB>path      (ids are line:code)
```

`tsc-ids` is a **frozen** tsc 6.0.3 baseline, exactly like the official suite's
committed `.errors.txt` baselines — so `repros --check` needs only the typokat binary:
no tsc, no network, milliseconds. It fails on

- any change to a repro's typokat ids (the identity ratchet),
- any change of status (MATCH ↔ ALLOWED ↔ DIVERGE),
- a repro missing from the scoreboard, or a scoreboard row missing from `repros/`.

`repros --save` rewrites it (commit the diff — the file's git history *is* the record);
`--refresh-tsc` additionally re-runs tsc to refresh the frozen baselines.

The four committed repros pin: the overload/object-literal shape from backlog 95, the
canonical three-line generic-arrow shape, a case where an unrelated *sibling* callback
is load-bearing, and one live tsc divergence (per-element elaboration cardinality).
On the `412f321` binary, `repros --check` fails on three of the four in milliseconds.

## CI

Two bounded jobs in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml):

- **`differential`** (every push / PR, hard gate): the harness's own unit tests, then
  `repros --check`, then a fixed-seed self-consistency sweep of 600 generated programs
  with `--time-budget 120`. Everything after the (cached) release build is capped at
  ~2.5 minutes, so the job cannot make CI unbounded.
- **`differential-tsc`** (weekly schedule only): re-verifies the frozen tsc baselines
  against a pinned `typescript@6.0.3` and prints the truth-mode dashboard for a human
  to triage. Generation capped with `--time-budget 300`.

Reference-binary mode (the one that reproduces `412f321`) is **not** in CI — building a
second binary per PR is not a bounded cost, and a PR that deliberately changes
diagnostics would fail it by design. It is a required step in
[`docs/reference/dev-method.md`](../../docs/reference/dev-method.md) §1 instead, for
changes touching inference or contextual typing.

## Harness unit tests

```sh
cd tooling/differential && python3 -m unittest test_differential -v
```

Stdlib only, no binary, no tsc. They cover the parts that could silently *hide* a
divergence: output parsing (a crash or an unparseable line must be a hard failure, never
a silent zero), the comparison, allowlist cancellation, the scoreboard round-trip,
generator determinism and trigger coverage, and the shrinker's refusal to accept a
program the oracle rejected.

## Limitations (v1, honest)

- The grammar covers the contextual-typing region and nothing else — no narrowing, no
  classes beyond a `this` field, no modules, no type-level programming. It is meant to
  grow towards whatever region a bug last came from.
- tsc-mode comparison is (line, code) only. A divergence that keeps the code and moves
  the column is invisible there (it is *not* invisible in reference-binary mode).
- The allowlist cannot distinguish a documented cosmetic difference from an
  undocumented one that happens to have the same code pair on the same line. The
  per-rule hit counts are the mitigation: a shape that suddenly explodes is visible.
- Shrinking is greedy, not optimal; it minimizes until no single reduction helps.
- `tsc` batching puts a whole generation in one program. Names are namespaced per
  program, but a future grammar that emits modules or global augmentation would have to
  revisit that.
