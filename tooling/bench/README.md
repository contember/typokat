# typokat synthetic benchmarks

Black-box synthetic benchmark harness for the ADR-0001 profiling gate. It
generates TypeScript corpora inside typokat's current implemented scope and
times a prebuilt `typokat` binary against `tsgo` with `hyperfine`. Most families
are single-file clean programs; `errors` is intentionally invalid and measures
diagnostic construction/rendering; `modules` is a multi-file project that
exercises the M29 cross-file pipeline (import resolution + cross-file checking).

By default typokat is timed with its normal rich `codespan` terminal diagnostics.
Use `--typokat-format compact` when you want to separate checker throughput from
source-frame rendering overhead.

Each timed command also gets a separate peak-memory sample by default via
GNU `/usr/bin/time -v`; the report records `max RSS MiB`. Use
`--memory-runs 0` to disable this extra pass or `--memory-runs N` to take the
maximum across several samples.

The generated corpus is intentionally not committed: `corpus/` and `report/`
are gitignored.

## Usage

```sh
cd tooling/bench

# Generate the default 1k / 10k / 100k line corpora.
python3 typobench.py generate

# Install the TypeScript RC native compiler locally (gitignored).
python3 typobench.py setup-tsgo

# Run typokat vs tsgo. Build typokat first; the harness does not import the crate.
cargo build --release
python3 typobench.py run --typokat ../../target/release/typokat

# Quick local smoke run without tsgo.
python3 typobench.py run --tools typokat,tsc --sizes 1000 --runs 3

# Run only the diagnostics/error corpus.
python3 typobench.py run --families errors --sizes 1000 --runs 3

# Run the M31/M32 model-shape corpus (intersections + signature shape).
python3 typobench.py run --families shape --sizes 1000,10000 --runs 3

# Run only the multi-file modules corpus (cross-file checking).
python3 typobench.py run --families modules --sizes 1000,10000 --runs 3

# Measure diagnostics with typokat's compact, low-overhead renderer.
python3 typobench.py run --families errors --typokat-format compact

# Disable memory sampling for a pure timing run.
python3 typobench.py run --families shape --memory-runs 0
```

As of 2026-07-06, `typescript@rc` exposes the native compiler as the `tsc`
binary. The harness labels that tool `tsgo` in reports because this is the tsgo
implementation being compared. `setup-tsgo` installs `typescript@rc` into
`tooling/bench/.tools/`, and `run` uses `.tools/node_modules/.bin/tsc` by
default when it exists. You can also pass a direct binary or command:

```sh
python3 typobench.py run --tsgo /path/to/tsgo
python3 typobench.py run --tsgo "npm exec --yes --package typescript@rc -- tsc"
```

The `npm exec` form is useful for smoke checks, but it includes npm startup in
every hyperfine sample. Use a direct local binary for real benchmark numbers.

Default `tsgo`/`tsc` flags are exactly the no-lib checker flags:

```sh
tsc --noEmit --noLib --skipLibCheck <file.ts>
```

Each single-file corpus starts with the minimal empty global interfaces needed
to avoid `TS2318` under `--noLib` (`Array`, `Object`, `Function`, and friends).
The `modules` project puts those same globals in one non-module `globals.ts`
script (a real module's own declarations are module-scoped and cannot satisfy
the global lookup), passed alongside the module files.

## Corpus families

- `relation` - wide structural object types plus unions, designed to exercise
  assignability, relation cache hits, and repeated source/target comparisons.
- `generics` - generic functions, interfaces, explicit instantiations, and
  call-site inference.
- `typelevel` - conditional, mapped, and template-literal type evaluation, with
  repeated alias instantiations.
- `shape` - M31/M32 model-shape cases: intersection source/target reads and
  assignments, recursive intersection sources, optional/default parameters,
  function rest parameters, construct signatures, tuple rest aliases,
  conditional rest `infer`, and rest-aware calls.
- `flow` - narrowing-heavy functions using `typeof`, `null`, and discriminated
  union checks.
- `errors` - intentionally invalid assignments, calls, object literals, member
  access, readonly writes, flow returns, and M31/M32 shape failures
  (intersections, optional/default/rest signatures, tuple rest, conditional rest
  `infer`). This exercises relation failure paths, diagnostic reason chains, and
  renderer throughput.
- `modules` - a multi-file project: many small modules, each importing the two
  preceding ones (a DAG, not a bare chain) and composing their exported
  types/values, plus a fan-in `main.ts` and a shared `globals.ts` script. This
  exercises typokat's M29 cross-file pipeline (`scan_imports`, dependency order,
  cross-file checking) and is the family where tsgo's cross-file parallelism
  helps most. The `size` is the target total line count spread across the
  modules, so `lines/s` stays comparable with the single-file families.

The default sizes are `1000`, `10000`, and `100000` source lines per family.
For `modules`, a size is roughly `size / 16` separate module files (e.g. ~6k
files at 100000), so the tool is invoked with the whole file set on one command
line.
`run` also includes a small startup file by default so the process cold-start
floor is visible next to throughput numbers.

Before timing, `run` validates the expected outcome for every selected file:
positive families must exit cleanly, while `errors` must exit with diagnostics.
For the `errors` family, hyperfine is invoked with `--ignore-failure` only after
that preflight passes.

## Reports

`typobench.py run` writes:

- `report/latest.json` - machine-readable results, including raw hyperfine
  statistics, lines/s, and peak RSS samples.
- `report/latest.md` - compact table for pasting into profiling notes.
- `report/raw/<timestamp>/...json` - the raw per-file hyperfine exports.

`hyperfine` launches a fresh process for every sample, so the timings include
CLI startup and whole-run parse/bind/check time. The runner uses
`hyperfine --shell=none` by default so shell startup is not counted. The results
do not model editor incrementality or long-lived server warm caches.

## Caveats

The single-file families keep most of the corpus in a per-file scope, which
neutralizes tsgo's main advantage: goroutine parallelism across independent
files. The `modules` family is the multi-file counterpart and is where that
parallelism shows up — as the module count grows, tsgo spreads work across
cores and closes the gap, so read those rows as the cross-file comparison rather
than lumping them with the single-file throughput numbers.

Even with `--noEmit`, tsgo/tsc still carry compiler infrastructure that typokat
does not. Treat these numbers as a profiling instrument and a broad external
baseline, not a correctness comparison. Correctness parity remains covered by
the official-suite harness.
