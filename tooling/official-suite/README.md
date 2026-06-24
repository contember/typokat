# Official TypeScript conformance — black-box harness

Runs typokat against a curated slice of the **official microsoft/TypeScript
conformance suite** and diffs its diagnostics against tsc's `.errors.txt`
baselines. It is the project's "cross-check vs real `tsc --strict`" step
(HANDOFF.md §1, §4) — automated, at scale, and biased to surface **false
negatives** (dropped errors, the project's #1 fear).

It is a **triage dashboard, not a pass/fail gate**: typokat is a deliberate
subset of tsc (no `lib.d.ts`, no modules, no emit, only M0–M22 constructs), so a
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
```

`fetch` needs `gh` (authenticated) for directory listing and pulls file blobs from
`raw.githubusercontent.com`. `run` only needs Python 3 and the binary.

## How a test is handled

1. **Unit parsing (line fidelity).** TS strips `// @option: value` directive lines
   from the test content, which shifts every following line up. The harness
   replicates this and runs typokat on the **stripped** content, so typokat's line
   N equals baseline line N. `@filename:` splits multi-file tests.

2. **Gating ("discover").** Each test lands in exactly one bucket:
   - `multifile` — more than one `@filename` unit (modules / cross-file).
   - `syntax:<feature>` — uses a checking feature typokat doesn't model and that
     wouldn't self-report (conditional/mapped/template-literal types, `infer`,
     `enum`, decorators, `namespace`/`import`/`export`, `satisfies`, `as const`).
     Heuristic regex — see `OUT_OF_SCOPE_SYNTAX`.
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

A dashboard to stdout plus `report/latest.json` (full per-file detail, diffable
over time). Promote a clean in-scope slice into a locked gate later by reading that
JSON — that's the "discover → promote" path.

## Knobs / config (top of `tsofficial.py`)

- `PINNED_SHA` — the TS commit (currently the `v6.0.3` tag, matching the tsc the
  checker was validated against). Bump it deliberately; baselines move with it.
- `DEFAULT_DIRS` — the curated conformance subtrees fetched by default.
- `OUT_OF_SCOPE_SYNTAX` — the syntax-bucket heuristics.

## Limitations (v1, honest)

- Syntax bucketing is regex-heuristic; a few tests may be mis-bucketed. The
  `unresolved` and `parse-error` self-gates catch most real out-of-scope cases.
- Column positions aren't compared (line + code only) — typokat validates columns
  with its own snapshot tests, and columns are the most baseline-fragile field.
- Message text isn't compared. Codes + lines are the robust contract; message
  wording diverges by design in documented places (see `tests/cases/README.md`).
- A test whose baseline has a non-default name (multi-variant baselines) is treated
  as expected-clean. Rare in the curated single-config dirs.

## Licensing

Test inputs/baselines are fetched from microsoft/TypeScript (Apache-2.0) into the
gitignored `corpus/`; nothing is vendored into the repo. If you later vendor a
slice, add a NOTICE with attribution.
