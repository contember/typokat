# Full default-library benchmark

This directory freezes the WU0A cross-tool contract for the production
TypeScript 6.0.3 ES2025-full cutover. It is deliberately separate from
`tooling/bench`: that older synthetic harness uses `--noLib --skipLibCheck`, while
this gate measures normal default-library loading and rejects either flag.

WU0A supplies the sole collector and structural evidence inspector. It stages and attests
the native comparator, verifies the semantic oracles, proves today's production
path RED, executes the frozen semantic/control/timing/memory commands, and
rejects incomplete or ineligible evidence. WU8 runs this collector on the final
production binary without replacing its schema, commands, rows, or statistics.

## Frozen inputs

- `contract.toml` pins `typescript@7.0.2`, its npm and platform-package
  integrities, upstream revision, the Linux x64 native binary's 24,101,026-byte
  SHA-256 identity, all flags, thresholds, scheduling, limits, and controls.
- `expected-libraries.txt` has one comment header plus exactly 82 ordered
  records. Each record pins the filename, byte length, and SHA-256 from the
  vendored TypeScript 6.0.3 profile. The parser explicitly requires 82 records;
  the header is not an 83rd library.
- `workloads.lock` pins every input byte. The four immutable rows contain 1, 1,
  2, and 32 files respectively.
- `oracles.json` pins normalized `relative-path:line:column:code` outcomes.
  `fast-errors` has six TS2322 diagnostics; every other comparator row is clean.

The matrix is:

1. `fast-clean`: arrays/tuples, Promise, regexp, primitives, DOM, Intl, and a
   generator/iterator in one external module;
2. `fast-errors`: six stable errors over the same library surface;
3. `collision`: a legal script-global `Array<T>` merge plus ordinary library
   demands;
4. `fanout`: 32 files with one global collision and mixed library consumers.

## Comparator staging and oracle proof

Installations/downloads are never timed. `npm` verifies the recorded package
integrity; the runner then verifies package metadata and the direct executable's
exact bytes before copying anything. Use a disposable directory outside the
repository:

```sh
tool_root=$(mktemp -d /tmp/typokat-tsgo7.XXXXXX)
npm install --prefix "$tool_root" --ignore-scripts --no-audit --no-fund \
  typescript@7.0.2

python3 tooling/full-lib-bench/full_lib_bench.py stage-comparator \
  --package-root "$tool_root/node_modules/@typescript/typescript-linux-x64" \
  --destination tooling/full-lib-bench/.stage/tsgo-7.0.2

python3 tooling/full-lib-bench/full_lib_bench.py verify-oracles \
  --tsgo tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc
```

The staged runtime contains the pinned native `tsc`, its required unselected
`lib.d.ts` location sentinel, and exactly the 82 vendored profile files. For
every row, `--listFilesOnly` must print those 82 libraries in registry order and
then precisely the locked inputs in invocation order. A missing, extra,
reordered, or byte-different library fails before semantic or timing work.
Timing never uses `--listFilesOnly`.

Both measured commands use a fresh direct process:

```text
<stage>/lib/tsc --strict --target es2025 --module preserve --noEmit --pretty false <inputs...>
target/release/typokat check --format compact <inputs...>
```

No npm/Node wrapper, daemon, `--noLib`, `--skipLibCheck`,
`--skipDefaultLibCheck`, `--noCheck`, incremental state, watch/build mode,
custom `RUSTFLAGS`, test binary, or WU0 environment switch is eligible. Option
matching is case-insensitive and handles `--name=value` spellings.

## RED production acceptance

Build the ordinary release binary, then run the explicit RED witness:

```sh
cargo build --release
python3 tooling/full-lib-bench/full_lib_bench.py assert-red \
  --typokat target/release/typokat
```

At WU0A HEAD this succeeds only by observing the expected failure. The normal
CLI still bootstraps `crates/typokat-check/src/prelude.ts`: `Promise`, DOM, Intl, Date and other
library names are missing, native members are absent, regexp literals are
incomplete, and the clean rows do not exit 0. This is the precise remaining RED
reason—not a comparator or fixture failure.

The future production assertion is intentionally disabled in the normal test
run. Enabling it today must fail; after the real provider lands it must pass and
`assert-red` must be retired:

```sh
TYPOKAT_FULL_LIB_ACCEPTANCE=1 python3 -m unittest \
  tooling/full-lib-bench/test_full_lib_bench.py

python3 tooling/full-lib-bench/full_lib_bench.py assert-production \
  --typokat target/release/typokat
```

`assert-production` runs the ordinary release CLI on all rows, demands exact
oracles, and repeats each row from renamed files with a harmless leading
comment. Filename/hash/output special cases therefore cannot satisfy it.

## Sampling and GO formula

The primary measurement includes process creation, production startup, default-library
source compilation, source I/O, parse/bind/check, diagnostic construction, and shutdown.
Before each balanced block, the collector pre-reads both binaries,
the 82 libraries, and that row's inputs and records their framed digest. Each of three separate time
windows has five recorded fresh-process warmups per tool followed by fifteen
`typokat,tsgo,tsgo,typokat` blocks: exactly 30 samples per tool and trial.

For every row and every trial:

```text
speedup = median(tsgo wall seconds) / median(typokat wall seconds)
```

The speedup and the `tsgo p95 / typokat p95` ratio must each exceed `1.00`, as must
the deterministic one-sided 95% bootstrap lower bound (100,000 resamples of complete
ABBA blocks, preserving within-block dependence and drift). `1.25` is the engineering
target but does not replace the hard gate. Memory is
ten additional interleaved `/usr/bin/time -v` processes per tool and row:
every typokat sample must be at most 512 MiB RSS and its median at most 1.25
times the tsgo median.

All child processes use a sanitized environment, closed stdin, a new process
group, a 30-second timeout, and concurrently drained stdout/stderr pipes with a
live 1 MiB cap per stream. A flood is killed as soon as it crosses the cap.
Timeout and normal leader exit both kill/check the complete process group, so a
pipe-holding descendant cannot survive. Malformed UTF-8, an unexpected output
channel/line, a reused PID, or an incomplete ABBA schedule is invalid.

The authoritative collector requires a completely clean worktree. It records
Cargo/rustc versions, `Cargo.lock`, Cargo configuration, and the raw result of
the exact sanitized `cargo build --release` command before it probes or measures
the resulting canonical binary. The public
`typokat library-info --format json` probe must identify the 82-file profile,
its source identity, and the production provider route; a missing or incorrect probe
is NO-GO before timing.

Three explicit, distinct window labels are required. Timing windows have a real,
recorded gap of at least 60 seconds (the default). The collector writes NO-GO
evidence and stops before timing if provider or semantic/control parity is RED:

```sh
python3 tooling/full-lib-bench/full_lib_bench.py run \
  --typokat target/release/typokat \
  --tsgo tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc \
  --output tooling/full-lib-bench/evidence/candidate.json \
  --window-label morning --window-label afternoon --window-label evening
```

## Evidence artifact

The canonical JSON object has exact top-level keys:

```text
schema verdict contract_sha256 identities host build provider_probe invocations semantics controls windows memory final_worktree
```

- `identities` contains the staged profile computed from the comparator's actual
  82 library files, comparator, and final typokat binary SHA-256/size/git commit.
  The same typokat identity must have produced semantic and timing evidence.
- `host` records hostname, kernel, machine, CPU model/count/affinity, priority,
  relevant resource limits, timezone, and UTC start time. At least two eligible
  CPUs and normal priority are required.
- `build` records the clean-worktree check, toolchain versions, `Cargo.lock`,
  repo Cargo configuration, `rust-toolchain{,.toml}` presence/identity, and the
  exact standard Cargo release build. It uses a freshly prepared, configless
  `CARGO_HOME` under `target/full-lib-bench/build-home`, exposes only the registry
  cache and index, forces offline mode and empty effective
  rustflags, and hashes the build-home layout before and after Cargo runs.
- `provider_probe` retains the raw public CLI probe and its independently
  observed profile identity and production route.
- `invocations` stores canonical argv, sanitized environment, absolute cwd and
  execution conditions under a SHA-256 key. Thread-limiting environment
  variables are forbidden. Every process record references one entry.
- every semantic/control/timing/memory process stores raw exit, stdout, stderr,
  monotonic interval, PID and process-group result; derived booleans are not
  trusted;
- every semantic row stores the exact `--listFilesOnly` process and a digest
  recomputed from the committed oracle;
- controls retain the exact renamed/comment-perturbed source bytes so the
  verifier can execute both tools again;
- each of three timestamped windows separated by at least 60 seconds stores alternating warmups,
  fifteen framed pre-read records and all 60 raw ABBA processes per row;
- every memory row stores five pre-reads and twenty raw ABBA `/usr/bin/time -v`
  processes (ten per tool), with RSS re-parsed from the raw report;
- each process attests the invoked executable bytes before and after the
  invocation. Pre-reads use the staged comparator's 82 library files, never the
  vendored source tree.
- `final_worktree` repeats the raw clean-worktree check after every measurement.

`run` is the only command that can authoritatively report `GO`: it validates the
complete in-memory result before returning success. A JSON file cannot
authenticate its collector or prove that measurements happened, so offline
inspection deliberately never certifies GO. Inspect a retained artifact with:

```sh
python3 tooling/full-lib-bench/full_lib_bench.py inspect-evidence \
  tooling/full-lib-bench/evidence/candidate.json \
  --typokat target/release/typokat \
  --tsgo tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc
```

For a complete GO-shaped artifact, the inspector hashes/sizes the two supplied
real binaries, verifies the native version/profile, validates provenance and
every canonical invocation, and recomputes raw outcomes, block-bootstrap
statistics and memory gates. For NO-GO artifacts it validates only the envelope,
contract/host binding, and invocation-registry structure; it does not claim to
replay an intentionally partial attempt. The command cannot emit GO. Historical
performance claims require the trusted `run`/CI log or an independent rerun; a
structurally valid hand-written artifact receives only an inspection result.
Complete validation rejects an
absent row/trial/window/pre-read, partial/reordered timing or memory schedule,
reused PID anywhere in the artifact, fast crash, semantic mismatch, failed
perturbation, stale identity, malformed output, or missing memory evidence. Raw
NO-GO attempts remain evidence and are never overwritten by a later run.

## Self-tests

```sh
python3 tooling/full-lib-bench/full_lib_bench.py verify-contract
python3 -m unittest discover -s tooling/full-lib-bench -p 'test_*.py' -v
```

The tests mutate locks/bytes/schedules/identities in temporary directories and
cover wrong library order/bytes, stale and transiently mutated binaries, exact
isolated build provenance, user-Cargo-config injection, rust-toolchain drift,
provider mismatch, staged pre-read paths, execution-condition
drift, window spacing/chronology, extra list files, normalized forbidden flags,
duplicate/nonstandard JSON, malformed/live-flood output, timeout/orphan
containment, offline non-authority, invalid RED preconditions, global PID reuse,
missing windows/pre-reads, reordered memory, fast crashes, oracle digest drift,
block-bootstrap determinism, and the disabled production acceptance.
