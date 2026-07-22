# Full-library snapshot feasibility gate

This directory is the external WU0B coordinator. It decides whether a complete,
AST-free semantic snapshot has enough startup headroom to justify the production
cutover. It is intentionally a release **libtest** gate, not the WU0A production
CLI benchmark: WU0B proves a prototype; WU8 reuses WU0A to judge the shipped path.

The contract is fail-closed. It pins the exact ignored test filters, TypeScript
6.0.3 profile identity, WU0A oracle bytes, build command, process protocol,
artifact limit, schedule, and external limits. A child cannot supply the verdict,
p95, RSS, or semantic parity. The coordinator derives those from raw process
records, `wait4(2)` resource usage, and the committed WU0A oracles.

## RED witness

The spec-only commit deliberately does not activate
`check::checker::wu0b_snapshot_spec`. Verify that the exact release libtest has no
matching regeneration or timing probe:

```sh
python3 tooling/full-lib-snapshot/full_lib_snapshot.py assert-red
```

`assert-red` succeeds only for the expected `running 0 tests` result. A build
failure, malformed harness output, unexpected artifact, or partially present
probe is not accepted as RED.

## Authoritative run

Run only from a clean worktree:

```sh
python3 tooling/full-lib-snapshot/full_lib_snapshot.py run \
  --output tooling/full-lib-snapshot/evidence/wu0b.json \
  --window-label morning --window-label afternoon --window-label evening
```

The coordinator performs two independent sanitized offline `cargo test
--release --lib --no-run` builds from separate copies containing only Git-tracked
source. Each build has a fresh configless Cargo home and target; only the host
registry cache/index is exposed, while dependency sources are freshly expanded
and hashed. It records Cargo/rustc versions, Cargo lock/config/toolchain inputs,
effective fingerprint/rustflags, host/affinity/priority, build output, and the
selected Cargo JSON libtest identity. Both builds receive the same ordered path
remaps for both physical build roots, collapsing them to
`/typokat-wu0b/build` across every rustc path scope; all other rustflags are
forbidden. Each root has the exact `build-N/{source,cargo-home,target}` shape,
and the two roots are distinct siblings. The two binaries must be byte-identical
but live at distinct paths.

Before any probe, each release libtest must run the complete pinned WU0B module:
exactly 16 non-ignored tests pass and the four coordinator-only probes remain
ignored. Roundtrip, identity, corruption, completeness, real-checker semantics,
and route calibration therefore form raw preflight evidence rather than child
claims. Each preflight runs from its matching isolated source copy with
`TYPOKAT_WU0B_PROFILE_ROOT` pinned to that copy, and the coordinator hashes the
tracked source immediately before and after it.

It then launches two distinct regeneration children with distinct nonexistent
output paths. Both must create exactly one regular file, terminate cleanly, and
produce byte-identical archives. Generation is never timed. Every warmup and
recorded timing child gets only the canonical first artifact through
`TYPOKAT_WU0B_SNAPSHOT_INPUT`; the output variable is absent, and the artifact's
identity is checked before and after each launch. Regeneration uses the matching
isolated source root in both its working directory and
`TYPOKAT_WU0B_PROFILE_ROOT`, with paired source hashes; timing always runs from
the authoritative repository root and receives no profile-root variable.

The coordinator itself parses both archives. It requires the exact magic,
version, profile and schema digests, ten ordered tags with zero reserved bits,
nonempty contiguous sections, exact body length, no trailing bytes, and valid
body/section SHA-256 values. An archive must be between 1 MiB and 32 MiB.

Each timing record must identify the route as `decoded-base-user-check`, carry a
nonempty runtime-projection SHA-256, and report the exact compiler measurement
object with zero source loads, parse units, bind units, semantic units, and
snapshot generations. Those route counters are cross-checked for every raw
launch; aggregate timing/RSS claims from the child remain non-authoritative.

There are three named windows. Each has five fresh-process warmups followed by
ten recorded eager launches, with at least 60 seconds between authoritative
windows. Every child runs in a new process group with closed stdin, concurrently
drained output, independent per-stream caps, and a 30-second timeout. The raw
monotonic interval is the timing authority. Peak RSS is captured externally from
`wait4`, not parsed from child output.

GO requires all of the following:

- two byte-identical archives, each no larger than 32 MiB;
- two independent clean builds and two complete release preflights;
- exact profile and archive identities in every probe record;
- exact `fast-clean` and `fast-errors` outcomes from WU0A's committed oracles;
- ten complete recorded launches in each of three windows, all with unique PIDs;
- externally calculated nearest-rank p95 at or below 120 ms, both overall and in
  every window;
- every externally observed warmup and recorded peak RSS at or below 512 MiB;
- no source, libtest, profile, oracle, or artifact mutation during collection.

Every process interval must be ordered, non-overlapping, and contained in its
declared timing window. Any gate failure writes a canonical `NO-GO` document with
the partial/raw records collected so far. Output is accepted only as a new direct
`.json` child of the ignored `tooling/full-lib-snapshot/evidence/` directory;
cleanliness is checked with an ignored candidate present and again after its
atomic installation.

The optional strategy and 1/2/32 probes are outside the mandatory GO schedule.
Their exact filters are pinned so later evidence cannot silently point at another
test. WU0B's mandatory witness remains the complete eager decoded base.

## Evidence inspection

```sh
python3 tooling/full-lib-snapshot/full_lib_snapshot.py inspect-evidence \
  tooling/full-lib-snapshot/evidence/wu0b.json
```

Inspection validates canonical structure and recomputes derived statistics, but
always prints `INSPECTED-NON-AUTHORITATIVE`. A retained JSON document cannot prove
that its claimed processes ran, so structural inspection can never certify GO.

For local contract checks without building Rust:

```sh
python3 tooling/full-lib-snapshot/full_lib_snapshot.py verify-contract
python3 -m unittest tooling/full-lib-snapshot/test_full_lib_snapshot.py -v
```
