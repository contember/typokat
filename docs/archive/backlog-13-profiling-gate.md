> **OUTCOME — DEFER / no VM (2026-07-13).** The ADR-0001 trigger was not met.
> The required profiler could not collect user-space samples on this host because
> `kernel.perf_event_paranoid=4`; without dispatch self-time, the strict gate cannot
> establish the required dominance in every type-level repetition. No VM work is
> authorized. This record is kept because it is the durable profiling evidence and
> the gate may be re-opened only with fresh, profiled evidence on a capable host.

# Backlog 13 — post-evaluator profiling gate

The bytecode VM remains a deferred optimization under
[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md).
This was an evidence gate, not implementation authorization.

## Protocol and inputs

| Field | Value |
|---|---|
| Semantic revision | `a1bcc59` |
| Date | 2026-07-13 |
| Host | `matej21-hp`; AMD Ryzen 7 PRO 8840HS, 8 cores / 16 threads |
| Kernel | Linux 6.17.0-40-generic, x86_64 |
| Rust | rustc/cargo 1.95.0 |
| Harness | Python 3.13.7; hyperfine 1.20.0; GNU time `/usr/bin/time -v` |
| External baseline | local TypeScript 7.0.1-rc (`tooling/bench/.tools/.../tsc`) |
| Corpus | generated `flow` and `typelevel`, one 100,000-line file each |
| Corpus hashes | retained run manifest `6f84bfc0cd6cb6d87e0a2f52396df0c54c1c76d2a07ca7f3c6cd93e5de7e6743`; flow `fb89224fa983e1d3d2b798dc9db21d11163f9c0526abbdfa44ffd257229e4435`; typelevel `6209b9c7293aef30be0b54fb722e7f089c492b1482b46b6c36718adaf9c93e77` |

Commands, run from the repository root unless noted:

```sh
cargo build --release
cd tooling/bench
python3 typobench.py generate --families flow,typelevel --sizes 100000
python3 typobench.py run --families flow,typelevel --sizes 100000 \
  --runs 10 --memory-runs 3 --typokat ../../target/release/typokat

cd ../..
CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none \
  RUSTFLAGS='-C force-frame-pointers=yes' cargo build --release
for family in flow typelevel; do
  for run in 1 2 3; do
    samply record --save-only --unstable-presymbolicate --rate 1000 \
      --iteration-count 25 --profile-name typokat-${family}-${run} \
      --output tooling/bench/report/wu13-samply/${family}-${run}.json.gz -- \
      target/release/typokat check tooling/bench/corpus/${family}-100000.ts
  done
done
```

The harness preflight validated both positive corpora before timing. Hyperfine used fresh
processes (`--shell none`, 10 runs, no warmup); GNU time took three independent peak-RSS samples.
The retained manifest was generated at `2026-07-13 09:41:14 UTC` and has an mtime before the clean
raw run began at `09:41:39 UTC`; it records exactly these two 100k corpora plus the harness startup
file. Its SHA-256 above is therefore the manifest that governed the clean run. The earlier reported
`a737…` manifest digest was incorrect and is superseded by this retained evidence.

The clean raw report records only the timing command path, not a binary digest. The ordinary release
binary used for timing was overwritten by the later symbolized, frame-pointer profiling build, and no
separate copy was retained, so its exact SHA-256 is unavailable. The current
`target/release/typokat` is the profiling binary (SHA-256
`2561e69cfc6303d9dc7179bbf0707deaf6c8d5a7901599ef88a2eb6f8557a536`), built with the profiling
flags shown above at `2026-07-13 09:48:23 UTC`; it must not be treated as the timing binary.

## Timing and RSS

| Corpus | Tool | Median s | Mean ± σ s | Range s | Lines/s | Peak RSS MiB (three samples) |
|---|---|---:|---:|---:|---:|---:|
| flow-100000 | typokat | 3.217733 | 3.211649 ± 0.043643 | 3.115720–3.274034 | 31,078 | 90.4 (90.3, 90.1, 90.4) |
| flow-100000 | tsgo | 1.227783 | 1.225723 ± 0.025403 | 1.187707–1.267636 | 81,448 | 293.2 (291.7, 293.2, 285.4) |
| typelevel-100000 | typokat | 0.452554 | 0.451705 ± 0.012695 | 0.424115–0.466693 | 220,968 | 117.6 (117.6, 117.2, 117.1) |
| typelevel-100000 | tsgo | 19.963402 | 20.731865 ± 2.828399 | 17.714246–25.376915 | 5,009 | 200.5 (200.5, 198.0, 194.9) |

The timing artifacts are generated and intentionally ignored:
`tooling/bench/report/latest.{json,md}` and
`tooling/bench/report/raw/20260713-114139/`. Two aborted, overlapping attempts
(`20260713-113951`, `20260713-114031`) were excluded from every result above.

Raw wall-clock samples in seconds, in harness order (all forty flow/typelevel timing commands exited
zero):

| Corpus | Tool | Ten samples (s) |
|---|---|---|
| flow-100000 | typokat | 3.231453215, 3.224835970, 3.187676206, 3.274034040, 3.187793620, 3.200928417, 3.256118931, 3.227300198, 3.210630204, 3.115720009 |
| flow-100000 | tsgo | 1.195552182, 1.243840168, 1.228907456, 1.255843477, 1.267636460, 1.229194612, 1.210448562, 1.211442470, 1.226658873, 1.187706816 |
| typelevel-100000 | typokat | 0.424115408, 0.450988333, 0.446128308, 0.454119325, 0.464087920, 0.462811381, 0.466692975, 0.450185224, 0.457116169, 0.440800052 |
| typelevel-100000 | tsgo | 22.532725116, 18.357126946, 17.714246089, 18.470657114, 20.056447285, 25.376914823, 22.083938458, 24.843123868, 19.870355852, 18.013116123 |

## Sampling result and decision buckets

`samply 0.13.1` was attempted three times for each required corpus (six invocations total),
with the protocol's 1000 Hz rate and 25 iterations. Every invocation exited `1` before starting
the target with:

```text
'/proc/sys/kernel/perf_event_paranoid' is currently set to 4.
In order for samply to work with a non-root user, this level needs to be set to 1 or lower.
```

No privilege change was requested or made. Therefore no user-space samples exist for the required,
non-overlapping buckets: evaluator task-dispatch self-time; evaluator algorithms; relation;
instantiation/substitution/inference; allocation/hash-consing; and parse/bind/report/other. They
are **unavailable**, not zero. The timing/RSS rows do not substitute for this classification.

## Strict decision

**DEFER / no VM.** ADR-0001 permits GO only when evaluator dispatch *self-time* is the largest
actionable bucket in every typelevel repetition, is at least 25% of user-space samples, and exceeds
relation, instantiation/substitution/inference, and allocation/hash-consing. The sampling restriction
leaves every one of those predicates unproven, so the fixed strict rule requires DEFER. A future
proposal must rerun this protocol on a host permitting unprivileged sampling; it must not treat these
throughput numbers or inclusive evaluator time as a substitute.
