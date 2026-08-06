# One-pass full-library probe

This directory defines a disposable, non-authoritative probe for the test-only
complete-combined checker path. It answers one question before a production design change:

> Can one source-backed pass over the 82 default-library files and the user project beat both
> the current collision route and the pinned native comparator without changing semantics?

This is not a snapshot, cache, daemon, incremental mode, or production route. Every one-pass
process compiles the committed TypeScript 6.0.3 profile from source through
`compile_complete_combined_profile_for_test(..., library_count = 82)`. The receipt below is
corroborated by the measured one-pass binary SHA-256 and independent code review; a self-reported
route string alone is not evidence.

The candidate is a complete-source fallback probe for the current production design. It is not
the phase-boundary design known as variant (b), and a promising result does not approve that
design. The authoritative WU6 run remains required after any production change.

## Frozen references and authoritative primitives

The probe owns no workload, profile, oracle, process supervisor, diagnostic parser, or RSS parser.
It binds the SHA-256 of `../full-lib-bench/full_lib_bench.py` and these four WU6 artifacts:

- `contract.toml`;
- `expected-libraries.txt`;
- `workloads.lock` and every file it identifies;
- `oracles.json`.

Reference verification parses the locks rather than trusting their self-hashes. It requires all
82 contiguous library entries, verifies every vendored filename, length, and SHA-256, verifies all
locked workload bytes, parses the exact four oracle rows, verifies the staged comparator package,
binary, sentinel, and all 82 staged library bytes, then runs its exact `--listFilesOnly` command for
each row. The listed order must be the 82 staged libraries followed by that row's inputs.

`one_pass_probe.py` imports `full_lib_bench.py` and directly reuses its `run_process`, sanitized
environment and invocation descriptors, process records and executable identity, diagnostic
normalizer (including ReasonChain grammar), output limit, complete process-group containment, and
`/usr/bin/time -v` RSS parser. It must not fork or simplify those implementations. The collector's
thin `three_route_preread_attestation` extends the authoritative `verify_regular` and length-framed
digest convention only to include the third binary. `run_probe` accepts injected executor,
identity-reader, pre-reader, and comparator-verifier callables only for acceptance tests. Their
defaults are the authoritative process/identity/comparator verifier and the thin pre-reader. The
injected pre-reader must preserve the same framing and use the executor's monotonic clock, so the
full fake run exercises strict global chronology rather than bypassing it. Even with an injected
identity reader, collection must call its comparator verifier exactly once before any comparator
evidence is trusted.

The frozen rows are `fast-clean`, `fast-errors`, `collision`, and `fanout`. The profile is the exact
82-file, 2,936,611-byte TypeScript 6.0.3 ES2025-full source set. The exact native comparator is
`typescript@7.0.2` Linux x64 from the WU6 contract.

## Exactly three routes

| ID | Direct command | Required probe |
|---|---|---|
| `production` (`P`) | release `typokat check --format compact` | `library-info --format json` |
| `one-pass` (`O`) | release test-only example `one_pass_probe check --format compact` | `probe-info --format json` |
| `tsgo` (`T`) | exact staged native comparator and frozen flags | exact version, binary and listFiles inventory |

Production JSON has exactly these keys:

```json
{"schema":2,"profile_sha256":"<frozen profile>","file_count":82,"check_route":"production-complete-source-once","provider_route":"production-default-library"}
```

One-pass JSON has exactly these keys:

```json
{"schema":1,"profile_sha256":"<frozen profile>","file_count":82,"probe_route":"test-only-complete-combined","source_backed":true,"replay_index":false}
```

Both JSON responses are retained as raw process records. All three measured binaries have exact
path, byte length, and SHA-256 identities. Every invocation records the guarded executable identity
before and after and rejects any change.

The collector captures one `{affinity,nice,rlimits}` tuple before the first invocation. Every
invocation descriptor must use that exact tuple. Validation rejects a coherently re-keyed
descriptor whose affinity or resource limits differ from the captured collection conditions.

## Raw process and invocation contract

Every subprocess in route probing, listFiles, semantics, controls, warmups, timing, and memory is a
raw authoritative process record. Its invocation is referenced by the SHA-256 of the exact argv,
canonical repository cwd, sanitized environment, affinity, priority, and resource limits. Records
retain a globally unique positive PID, start/end monotonic nanoseconds, derived finite positive wall
seconds, return code, raw stdout/stderr, complete-process-group result, and executable before/after
identity. Memory records additionally retain the unmodified `/usr/bin/time -v` stderr; RSS is
re-parsed from that raw report and never trusted as a free-standing number.

Schema validation is exact at every level. Booleans are not integers. Zero, negative, NaN, infinity,
overlapping/reversed chronology, duplicate PID, dirty process group, unknown output, wrong channel,
fast crash, missing/extra/duplicate diagnostic, orphan ReasonChain line, output over 1 MiB, wrong
invocation, stale identity, and malformed RSS are fatal.

Every compiler process in every phase must reproduce its row oracle. `fast-errors` must have its six
diagnostics even during warmup, timing, and memory. A typokat ReasonChain continuation is accepted
only after a supported primary diagnostic and only under the authoritative grammar.

## Phase order and stop gate

Collection is serial and fail-closed:

1. Verify references, identities, route probes, and staged listFiles inventories.
2. Run all four rows through all three routes and reproduce the committed oracles.
3. Rename every input, prepend the exact ASCII line
   `// one-pass probe perturbation; semantics unchanged\n`, retain the changed bytes, and reproduce
   the line-adjusted oracle through all three routes (`fast-errors` moves from lines 4–9 to 5–10).
4. Stop without any warmup, timing, or memory process if steps 1–3 differ.
5. Collect timing, then memory.

Execution events have one contiguous sequence and non-overlapping monotonic intervals. A later phase
cannot precede or overlap an earlier phase.

## Timing design

Each row first runs five cyclic fresh-process warmup rounds. Every round contains O, P, and T once;
the order rotates left by the round ordinal. Warmups are semantic evidence and must match the oracle.

Each row then runs exactly 15 superblocks. Before every superblock, the thin three-route pre-reader
attests O, P, T, the staged 82 library sources, and that row's sources in one framed record. Each
pre-read has exact path order, total bytes, framed SHA-256, and non-overlapping monotonic start/end
values.

Every superblock contains these three complete ABBA panels:

- `OT`: O,T,T,O — binding on all four rows;
- `OP`: O,P,P,O — binding only on `collision` and `fanout`;
- `PT`: P,T,T,P — evidence-only on every row.

Panel order rotates modulo three by superblock ordinal: `OT,OP,PT`, then `OP,PT,OT`, then
`PT,OT,OP`, repeated five times. Panels are never flattened or rescheduled.

Each binding panel recomputes median speedup, p95 ratio, and a deterministic one-sided 95% lower
bound from 100,000 resamples of the complete 15-superblock panel positions. The frozen WU6 seed is
used with a deterministic row/panel offset. `PROMISING` requires every O/T statistic to be strictly
greater than 1.00 for all four rows, and every O/P statistic to be strictly greater than 1.00 for
`collision` and `fanout`. P/T never affects the verdict. Its known collision/fanout failures must
remain visible rather than being smoothed away.

## Memory design

Each row has five memory superblocks. Before each one, the runner performs the same three-route
pre-read attestation. Every memory superblock contains the complete `OT` and
`PT` ABBA panels; their order alternates by superblock ordinal. This produces exactly ten O, ten P,
and twenty T RSS samples per row.

Only O memory is binding: every O sample must be at most 524,288 KiB and the median O/T ratio must
be at most 1.25. P/T RSS is evidence-only. In particular, the synthetic acceptance evidence retains
the already-observed P/T ratio above 1.25 on collision and fanout while remaining `PROMISING` when O
passes.

## Verdict and authority boundary

The only verdicts are `PROMISING` and `NOT-PROMISING`. The validator derives them from raw records;
it rejects a stored mismatch. `PROMISING` only justifies specifying and reviewing a production
change. It is not `GO`, does not satisfy WU6, and cannot support a public performance claim.

Evidence output is forbidden anywhere under `tooling/full-lib-bench`, including through a resolved
symlink. That directory is reserved for authoritative attempts. Example:

```sh
flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 2 -- cargo build --release --bin typokat'
flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 2 -- cargo build --release --example one_pass_probe'

target/release/examples/one_pass_probe probe-info --format json
target/release/examples/one_pass_probe check --format compact \
  tooling/full-lib-bench/workloads/fast-clean/main.ts
target/release/examples/one_pass_probe check --format compact \
  tooling/full-lib-bench/workloads/fast-errors/main.ts

flock -w 3600 /tmp/typokat-perf.lock -c \
  'cpu-lease run -n 4 --no-smt -- python3 tooling/one-pass-probe/one_pass_probe.py run \
    --production target/release/typokat \
    --one-pass target/release/examples/one_pass_probe \
    --tsgo tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc \
    --output /tmp/typokat-one-pass-probe.json'
```

The production binary and test-only example must always be built by those two separate Cargo
invocations. Never combine them in one Cargo command: the example's `test-utils` dependency feature
would otherwise be unified into the production binary and invalidate the route comparison.
The leader must witness both builds immediately before collection from committed, independently
reviewed build-relevant sources. The run log records the exact HEAD, both separate builds, and the
leased collector command. These are run-log obligations, not a new self-contained build-record
schema, and they do not make this probe authoritative.

The live CLI binds P and O to `target/release/typokat` and
`target/release/examples/one_pass_probe` under the repository root. Arbitrary P/O paths are allowed
only through the explicitly injected runner seam used by acceptance tests.

The direct release acceptance is binding before measurement can be interpreted:

- `probe-info` exits 0, writes no stderr, and prints exactly the six-key one-pass JSON receipt above;
- `fast-clean` exits 0 with empty stdout and stderr;
- `fast-errors` exits 1 with empty stdout, and the authoritative compact diagnostic parser must
  produce exactly
  `fast-errors/main.ts:4:7:2322` through `fast-errors/main.ts:9:7:2322`, one of each and no other
  output. The collector repeats this semantic preflight for O, P, and T before starting timing.

The full collector command is also mandatory; passing the example-presence or direct smoke checks
alone is insufficient. It emits only `PROMISING` (exit 0) or `NOT-PROMISING` (exit 1). CLI usage,
unknown arguments, and a forbidden output path fail before collection with exit 2.

Acceptance:

```sh
python3 -m unittest tooling/one-pass-probe/test_one_pass_probe.py -v
```

The corpus contains a full injected-executor collection, not only offline validator fixtures. It
also injects a semantic mismatch and proves collection stops before warmup/timing. Before the runner
exists, the current legitimate RED is 20 tests: one explicit failure
`one-pass probe implementation is absent` and 19 skips. An import or syntax failure is not valid RED
evidence.
