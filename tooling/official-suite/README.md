# Official TypeScript conformance — black-box harness

Runs typokat against a curated slice of the **official microsoft/TypeScript
conformance suite** and diffs its diagnostics against tsc's `.errors.txt`
baselines. It is the project's "cross-check vs real `tsc --strict`" step
(`docs/reference/dev-method.md` §1–2) — automated, at scale, and biased to surface
**false negatives** (dropped errors, the project's #1 fear).

It is a **triage dashboard, not a pass/fail gate**: typokat is a deliberate
subset of tsc (no full `lib.d.ts`, narrow local-relative modules, no emit), so a
1:1 match across the whole suite is neither expected nor the goal. The number that
matters is the in-scope **matched %** rising as milestones land, and the
**false-negative list** shrinking.

## Why it can run while the checker is being refactored

The harness never imports or builds the `typokat` crate. It shells out to the
**prebuilt binary** (`typokat check <file>`) and parses the rendered text. Point
`--bin` at any binary; the default is `target/release/typokat`. So a broken
work-in-progress `src/` does not block it.

## Usage

```sh
cd tooling/official-suite

# 1. Pull the corpus (pinned TS commit -> ./corpus, gitignored). One-time / on bump.
python3 tsofficial.py fetch                 # the full curated dir set
python3 tsofficial.py fetch --limit 60      # a quick slice
python3 tsofficial.py fetch --dir conformance/controlFlow   # specific subtree(s)

# 2. Diff + dashboard (uses target/release/typokat by default).
python3 tsofficial.py run
python3 tsofficial.py run --bin ../../target/debug/typokat

# 3. Regression tracking against the committed baseline.
python3 tsofficial.py run --check    # exit 1 if anything regressed vs scoreboard.txt
python3 tsofficial.py run --save     # accept current results as the new baseline
```

`fetch --dir` and `fetch --limit` deliberately create a **partial** corpus for
exploration. You may use plain `run` with one, but never `run --check` or `run
--save`: both ratchet operations require a freshly fetched, full default corpus
and a scoreboard whose TS SHA matches `PINNED_SHA`.

When deliberately replacing the pinned corpus/baseline, use the explicit
`python3 tsofficial.py run --save --rebaseline` after a successful full default
fetch and review. This is the only operation that may replace a missing or
stale scoreboard SHA/set; it still validates the complete format-3 corpus and
filesystem first. Treat it as an intentional baseline migration, not a fix for
a failed ordinary `--save`.

`fetch` uses one explicit Git transport: it maintains an ignored bare **full-blob**
cache in `.tools/typescript.git`, marked `full-blob-v1`. Missing/wrong markers,
partial-clone settings, corrupt repositories, or incompatible origins are recreated
before one non-interactive shallow fetch writes the exact pinned commit to a persistent
`refs/typokat/pinned/<SHA>` ref. The verified local tree and blob batch then need no
per-file network requests. There is no GitHub API or raw-file fallback. `run` only
needs Python 3 and the binary.

> **Build a current binary first.** `run` measures whatever binary you point at — a
> *stale* `target/release/typokat` (not rebuilt after a checker change) silently
> reports old behavior. Run `cargo build --release` before a meaningful run; the
> dashboard prints the binary's build time so staleness is visible.

## How a test is handled

1. **Unit parsing (line fidelity).** TS strips `// @option: value` directive lines
   from the test content, which shifts every following line up. The harness
   replicates this and runs typokat on the **stripped** content, so typokat's line
   N equals baseline line N. `@filename:` splits multi-file tests.

2. **Gating ("discover").** Each test lands in exactly one bucket:
   - `multifile` — more than one `@filename` unit (modules / cross-file).
   - `syntax:<feature>` — uses a checking feature typokat doesn't model and that
     wouldn't self-report (`enum`, decorators, `namespace`/`import`/`export`,
     `satisfies`, `as const`, user-defined type predicates/assertions, or
     `instanceof` narrowing).
     Heuristic regex — see `OUT_OF_SCOPE_SYNTAX`.
   - `unsupported` — typokat exited **3** (incomplete): it recorded an in-scope
     surface it does not yet check (an `incomplete[<id>]` record; the first-class
     incomplete outcome, WU2). Demoted to OOS but, unlike other buckets, it **keeps
     the full diagnostic diff** alongside the incomplete identities — a diagnostic
     regression inside a now-unsupported test must still be visible.
   - `parse-error` — typokat's own parser rejected it.
   - `unresolved` — typokat raised a `TK2304 (cannot find name)` the baseline does
     **not** have: a name tsc resolved via `lib.d.ts`/imports but typokat can't.
     This self-gates out lib/module dependence without maintaining a denylist.
   - otherwise **in-scope** → diffed.

3. **Diff.** Codes map by number (typokat mirrors tsc: `TS2322` == `TK2322`).
   Per source line, the multiset of typokat codes is compared to the baseline's:
   - **matched** — same code on the same line.
   - **false negative** — baseline had it, typokat didn't (surfaced loudly).
   - **false positive** — typokat had it, baseline didn't (often a documented
     safe-direction over-report; sometimes a real bug).

   In-scope results are split **strict** vs **non-strict**: tsc's old tests default
   to `strictNullChecks` **off**, while typokat is always strict, so non-strict
   tests diverge on `null`/`undefined` and are reported separately.

## Output

A dashboard to stdout plus `report/latest.json` (full per-file detail, with a
timestamp — gitignored, for ad-hoc inspection).

## Regression tracking — `scoreboard.txt` (committed)

`scoreboard.txt` is the **committed, deterministic** baseline: one sorted line per
test, no timestamps or machine paths, so a behavior change shows up as a single
changed line in `git diff`. Header lines carry the headline aggregates (corpus /
in-scope / clean-kept / error-exact / diag-recall) at the pinned TS SHA.

```
IN	4 8 0 12	5:2322,9:2345|	conformance/.../assignmentCompatWithCallSignatures2.ts
OOS:syntax:enum	- - - -	-	conformance/.../enumTest.ts
OOS:unsupported	1 1 0 2	3:2322|	expr-infer/template-literal/interpolation	conformance/.../templateCall.ts
```

Columns are `status<TAB>matched fn fp expected<TAB>ids<TAB>rel`. Most out-of-scope
tests are tracked with `-` for the numeric and `ids` columns (a scope flip IN↔OOS is a
regression/progress signal). **`OOS:unsupported` is the exception** (exit 3, WU2): it
carries the real numeric diff **and** a three-segment ids column
`<matched>|<fp>|<incomplete>`, where the third segment is a sorted, comma-joined list of
stable incomplete surface identities. A demotion to unsupported therefore keeps its full
diagnostic diff — a dropped matched identity or a new false positive inside an
unsupported test is still a regression — and its incomplete identities round-trip too (a
dropped incomplete identity regresses; a gained one is progress). `diag-recall` counts
unsupported error tests so the recall number stays comparable across the IN↔unsupported
boundary.

The **`ids` column is diagnostic identity**, not just a count:
`<matched>|<fp>`, each side a comma-separated, sorted list of `line:code` tokens
(with multiplicity). It is what makes the ratchet **identity-based** rather than
count-based: a same-count *swap* of which diagnostics match (e.g. matching a
different line, or a false positive moving) is a regression even though `matched`
and `fp` are unchanged. The numeric columns are kept for the human-readable
headline aggregates and remain exactly consistent with the identities.

Workflow:

- `run --save` writes/updates it. It requires the same full default corpus and
  matching TS-SHA header as `--check`; partial `--dir`/`--limit` fetches are
  exploration-only. Commit it when the change is intended (the git history of
  this file *is* the score-over-time record).
- `run --save --rebaseline` is the deliberate escape hatch for a reviewed
  pinned-SHA migration: it replaces a missing or stale scoreboard with results
  from a validated full default format-3 corpus. It cannot be combined with
  `--check` and should not be used to bypass ordinary ratchet failures.
- `run --check` re-runs and diffs against it by **diagnostic identity**: prints
  **REGRESS** for any previously in-scope file that drops a matched identity, gains
  a false-positive identity, or falls out of scope, and **progress** for
  improvements. It also enforces **corpus completeness in both directions** — a
  scoreboard entry missing from the corpus, or a corpus file missing from the
  scoreboard, is a regression (re-fetch / `--save` respectively). **Exit 1 on any
  regression** — so it can gate CI on *regressions* without the absolute match rate
  being a gate (matches the "dashboard, not gate" decision: the number is free to
  be low, it just must not get worse silently). `--check` requires the full corpus,
  so it rejects `--limit`.

A checker invocation the harness cannot trust — an unexpected exit code, a signal
(crash), a timeout, or output inconsistent with the exit code — is a **hard harness
failure**: the run aborts loudly with the file, exit code, and captured output. It is
never scored as success or as a silent zero (a crash masquerading as a clean file is
exactly the false negative this suite exists to catch). The documented exit codes are
`0` (clean), `1` (type/parse diagnostics), and `3` (incomplete — WU2). Inconsistent
means: exit `0` with **any** diagnostic OR incomplete record (a clean exit must be
silent), exit `1` with nothing parseable **or with an incomplete record** (any
incomplete surface forces exit `3`), or exit `3` with no incomplete record
(unparseable / lost incomplete output). Exit `3` is a discovery result
(`OOS:unsupported`), never a crash — but strict crash/signal/unparseable handling is
unchanged. Incomplete records are parsed **column-0-anchored, rich-format-only** (the
harness never passes `--format`), so quoted source-snippet text containing
`incomplete[...]` can never fabricate a record.

Because the corpus is reproducible from the verified Git cache at the pinned SHA, the
baseline is meaningful on any machine. Re-fetch at the same SHA before `--check`.

This is the "discover → promote" path made concrete: as milestones land, `--save`
ratchets the numbers up; `--check` keeps them from sliding back.

### Harness unit tests

The harness's own logic (identity ratchet, completeness, exit-code handling,
scoreboard round-trip) has stdlib-only unit tests — no third-party deps:

```sh
cd tooling/official-suite
python3 -m unittest test_tsofficial -v
```

## Knobs / config (top of `tsofficial.py`)

- `PINNED_SHA` — the TS commit (currently the `v6.0.3` tag, matching the tsc the
  checker was validated against). Bump it deliberately; baselines move with it.
- `DEFAULT_DIRS` — the curated conformance subtrees fetched by default.
- `OUT_OF_SCOPE_SYNTAX` — the syntax-bucket heuristics.

## Limitations (v1, honest)

- Syntax bucketing is regex-heuristic; a few tests may be mis-bucketed. The
  `unresolved` and `parse-error` self-gates catch most real out-of-scope cases.
- `parse-error` is a whole-file gate. A mixed official file can stay out of scope
  when it contains parser-level syntax diagnostics even if other cases in that
  file use constructs typokat now models. Promote those by adding focused
  typokat fixtures or by curating a split official slice, not by globally
  ignoring parser diagnostics.
- Column positions aren't compared (line + code only) — typokat validates columns
  with its own snapshot tests, and columns are the most baseline-fragile field.
- Message text isn't compared. Codes + lines are the robust contract; message
  wording diverges by design in documented places (see `docs/reference/divergences.md`).
- A test whose baseline has a non-default name (multi-variant baselines) is treated
  as expected-clean. Rare in the curated single-config dirs.

## Licensing

Test inputs/baselines are fetched from microsoft/TypeScript (Apache-2.0) into the
gitignored `corpus/`; nothing is vendored into the repo. If you later vendor a
slice, add a NOTICE with attribution.
