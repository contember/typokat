# WU0D release evidence runner

`run.pl` is the external, fail-closed process coordinator for the disabled WU0D Candidate-B
release gate. It builds exactly one release libtest from Cargo JSON, freezes a digest-addressed
copy under `target/wu0d-release/`, verifies and warms that binary plus the complete strict-profile
runtime inventory, and launches a selected probe in a fresh process group. Before every workload or
validator it reproduces the strict Rust loader's complete filesystem inventory: the profile root
and sole `lib/` directory, all directory entries and non-symlink file types, and all 88 regular
files (82 declarations plus `.gitattributes`, `profile.toml`, both upstream notices, `README.md`,
and `THIRD_PARTY_NOTICE.md`). Every regular file is read into the filesystem cache after the exact
tree and pinned loader inputs have been revalidated.

The two embedded control fixtures are provenance-checked independently of their semantic output.
The runner extracts their exact ordinal-zero name/source records from
`wu0d_candidate_release.rs`, applies the same
`u64be(name_len) || u64be(source_len) || name || source` framing as Rust, and requires the pinned
non-cycle and reporter-control identities before building and again before every launch.

The evidence `elapsed_us` and five-second deadline use the parent coordinator's monotonic clock
with a conservative microsecond ceiling. A readiness pipe confirms `setsid` before the parent
addresses a process group; until then, timeout handling targets only the direct child. At the
deadline the runner sends `TERM`, follows with `KILL` after a short grace period, and uses an
absolute bounded drain window. On Linux the parent observes the leader through `/proc` and does
not call `waitpid` while a confirmed process group still has live members. A terminated leader
therefore remains an unreaped zombie that reserves both PID and PGID until descendants are gone
or group escalation completes, closing the PID-reuse window. It deliberately does not use
coreutils `timeout`. GNU `/usr/bin/time` independently corroborates wall time and supplies the
child exit plus peak RSS. The host identity covers the raw machine ID and current boot ID in
addition to kernel, CPU, and toolchain facts. Raw stdout, stderr, timing output, Cargo JSON, build
diagnostics, host facts, and process metadata stay
in a unique `target/wu0d-release/runs/` directory even when the run fails.

The frozen executable must remain a regular non-symlink file with the expected SHA-256 before and
after every warmup, workload launch, and validator launch. Stdout and stderr are monitored while
the child runs and are each capped at 128 KiB. Crossing either cap terminates the process tree and
fails closed; the oversized file remains at its artifact path. Post-run reads are independently
bounded to one byte beyond the applicable limit, including a 4 KiB bound for GNU `time` output,
so a late-growing or oversized capture is never loaded wholesale by the runner.

Run the fast non-cycle baseline smoke probe:

```sh
perl tooling/wu0d-release/run.pl --smoke-control
```

Run one explicit workload and mode:

```sh
perl tooling/wu0d-release/run.pl --single primary off
perl tooling/wu0d-release/run.pl --single reporter-control candidate-b
```

`WORKLOAD` is `primary`, `non-cycle`, or `reporter-control`; `MODE` is `off` or `candidate-b`.
Every child starts after all inherited `TYPOKAT_WU0D_*` variables are removed. Candidate mode then
adds only `TYPOKAT_WU0D_CANDIDATE=candidate-b-v1`.

Run the complete release schedule only when the single-process probes are known to fit the frozen
limits:

```sh
perl tooling/wu0d-release/run.pl --full
```

Full mode executes primary, non-cycle, and reporter-control in the exact
`A1,B1,B2,A2,A3,B3,B4,A4,A5,B5` order. It revalidates and rewarms the frozen binary and complete
88-file runtime inventory before each of the 30 fresh process groups and before the validator,
assigns identities unique across the complete run, atomically writes the canonical evidence
artifact, and invokes the checked-in validator through that same frozen libtest. A timeout, failed
process, malformed timing record, oversized stdout, memory breach, or non-GO validator result
stops the run and leaves its partial raw artifacts intact.

Check the runner without building or launching a probe:

```sh
perl -c tooling/wu0d-release/run.pl
perl tooling/wu0d-release/run.pl --self-test
perl tooling/wu0d-release/run.pl --dry-run --single primary off
perl tooling/wu0d-release/run.pl --dry-run --full
```

`--self-test` also exercises the supervisor against delayed pre-`setsid` children (including a
TERM-ignoring direct-KILL path), a leader that exits while a TERM-ignoring descendant remains, a
leader that exits while its descendant floods output, the bounded post-read path, and an
executable that changes after launch. It also verifies the complete profile inventory and proves
that synthetic tree or embedded-control drift fails closed. It does not build or run any WU0D
workload.

The single-probe command is an operational primitive, not sufficient release evidence. Candidate B
remains unauthorized until the exact 30-process artifact is accepted by the checked-in Rust
validator.
