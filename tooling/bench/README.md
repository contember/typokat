# typokat synthetic benchmarks

Black-box synthetic benchmark harness for the ADR-0001 profiling gate. It
generates single-file TypeScript corpora inside typokat's current implemented
scope and times a prebuilt `typokat` binary against `tsgo` with `hyperfine`.
Most families are clean programs; `errors` is intentionally invalid and measures
diagnostic construction/rendering.

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

Each generated file starts with the minimal empty global interfaces needed to
avoid `TS2318` under `--noLib` (`Array`, `Object`, `Function`, and friends).

## Corpus families

- `relation` - wide structural object types plus unions, designed to exercise
  assignability, relation cache hits, and repeated source/target comparisons.
- `generics` - generic functions, interfaces, explicit instantiations, and
  call-site inference.
- `typelevel` - conditional, mapped, and template-literal type evaluation, with
  repeated alias instantiations.
- `flow` - narrowing-heavy functions using `typeof`, `null`, and discriminated
  union checks.
- `errors` - intentionally invalid assignments, calls, object literals, member
  access, readonly writes, and flow returns. This exercises relation failure
  paths, diagnostic reason chains, and renderer throughput.

The default sizes are `1000`, `10000`, and `100000` source lines per family.
`run` also includes a small startup file by default so the process cold-start
floor is visible next to throughput numbers.

Before timing, `run` validates the expected outcome for every selected file:
positive families must exit cleanly, while `errors` must exit with diagnostics.
For the `errors` family, hyperfine is invoked with `--ignore-failure` only after
that preflight passes.

## Reports

`typobench.py run` writes:

- `report/latest.json` - machine-readable results, including raw hyperfine
  statistics and lines/s.
- `report/latest.md` - compact table for pasting into profiling notes.
- `report/raw/<timestamp>/...json` - the raw per-file hyperfine exports.

`hyperfine` launches a fresh process for every sample, so the timings include
CLI startup and single-file parse/bind/check time. The runner uses
`hyperfine --shell=none` by default so shell startup is not counted. The results
do not model editor incrementality or long-lived server warm caches.

## Caveats

This benchmark is deliberately single-file by default. That keeps it inside
typokat's current module-free scope, but it also neutralizes tsgo's main
advantage: goroutine parallelism across independent files. A future multi-file
corpus should be reported separately and would be expected to favor tsgo more.

Even with `--noEmit`, tsgo/tsc still carry compiler infrastructure that typokat
does not. Treat these numbers as a profiling instrument and a broad external
baseline, not a correctness comparison. Correctness parity remains covered by
the official-suite harness.
