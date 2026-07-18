# WU0E diagnostic runner

`run.pl` diagnoses the full-library primary workload. It is measurement tooling only: it cannot
produce WU0D evidence, authorize candidate B, or change WU0D's fixed 5-second/512-MiB release gate.

The runner builds one release libtest from Cargo's JSON artifact stream and publishes it beneath a
verified real `target/wu0e-diagnostic/frozen/` directory. Publication uses an exclusive no-follow
temporary and an atomic no-replace link. Every launch opens that frozen executable first, verifies
its pathname identity and digest, and executes the opened `/proc/self/fd/` handle. The shared
scheduler then runs exactly:

1. `plain` workload, then its same-binary validator;
2. `measured-off` workload, then its same-binary validator;
3. `candidate-b` workload, then its same-binary validator.

An infrastructure failure or crash stops before a validator or later mode. Before each callback the
runner revalidates the binary and the complete strict-profile inventory: 82 declaration files plus
the six root metadata files. Completed semantic digests must agree; contained prefixes always use
`unavailable`.

Workloads and validators use one production launch contract for command construction, environment
allowlisting, frozen identities, cgroup admission, child setup, and stable-handle execution. Both
paths revalidate the frozen executable pathname and digest after the child completes; the final
candidate-B validator is covered by the same check. The former alternate v1 launch and dossier
writers no longer exist.

## Delegated containment

A real diagnostic reexecutes itself exactly once, without a shell, using:

```text
/usr/bin/systemd-run --user --scope --quiet --no-ask-password \
  --property=Delegate=yes --expand-environment=no -- \
  /usr/bin/perl ABSOLUTE_RUN_PL
```

The inner coordinator derives its unified cgroup from `/proc/self/cgroup` and cross-checks the
scope's `ControlGroup` and `Delegate` with absolute `/usr/bin/systemctl --user`. It creates a
`supervisor/` cgroup, moves itself there, proves the delegated root has no internal processes, and
only then enables `memory` for child cgroups. Teardown restores only the controller state changed by
this runner, moves the coordinator back, proves `supervisor/` empty, and removes it.

Every workload and validator receives an exclusive launch cgroup. Mandatory preflight verifies
`cgroup.type`, membership/events/kill interfaces, and the memory controller files, then writes and
reads back:

- `memory.max=1073741824` (1 GiB);
- `memory.swap.max=0`;
- `memory.oom.group=1`.

The child moves itself into that cgroup as its first post-fork action, before `setsid`, readiness,
environment installation, or stable-handle exec. The parent accepts readiness only after proving
both membership and the new process group. Descendants inherit the same hard memory backstop.

The coordinator also samples summed live cgroup RSS from bounded `cgroup.procs` snapshots. Vanished
members retry; stable unreadability, unresolved churn, or arithmetic uncertainty fail closed. The
10-ms target is attribution telemetry, not a termination limit. `memory.current`, `memory.peak`, and
the baseline/final/delta values from `memory.events.local` are recorded separately. A `max` delta
alone reports cap contact; only causal OOM counters identify kernel OOM containment.

Other fixed bounds are 180 seconds of coordinator monotonic time, 128 KiB each for stdout and
stderr, and 256 KiB for a workload trace. One adjudicator applies both loop-time and final
post-read discoveries in this order:

```text
infrastructure > trace > stdout > stderr > rss > deadline > crash/normal
```

Cleanup retains the leader zombie while descendants remain, checks both `cgroup.events:populated`
and the confirmed PGID, and redundantly attempts direct-child, PGID, and `cgroup.kill` containment.
It then reads final memory events, reaps the leader, and removes exactly the launch cgroup. Cleanup
has two bounded emergency attempts; failure retains evidence and aborts the enclosing scope.
If any outer lifecycle exception leaves a launch cgroup retained, the coordinator first fsyncs a
complete v2 process-metadata record, re-verifies the live delegated scope identity, and invokes the
exact `/usr/bin/systemctl --user --no-block stop UNIT` abort path before propagating the exception.

## Commands and artifacts

Run the three-mode diagnostic:

```sh
perl tooling/wu0e-diagnostic/run.pl
```

Validate the coordinator without building or launching the compiler:

```sh
perl -c tooling/wu0e-diagnostic/run.pl
perl tooling/wu0e-diagnostic/run.pl --dry-run
perl tooling/wu0e-diagnostic/run.pl --self-test
```

`--dry-run` validates the exact strict-profile inventory and prints the frozen schedule and limits.
`--self-test` preserves the original process-group compatibility suite. The ignored Rust hardening
acceptance additionally invokes the private `--self-test-evidence` mode with Rust-owned fixtures;
that mode routes all six scheduled launches through the production contract, records the real
parent/child preflight action sequence, and uses real delegated cgroups, including a self-test-only
64-MiB kernel-OOM cgroup. The 64-MiB value is never used by the production diagnostic.

Commands, bounded stdout/stderr/trace captures, process metadata, cgroup metrics, build output, host
facts, validation lines, and the v2 dossier remain in a unique real directory beneath
`target/wu0e-diagnostic/runs/`. Failed hardening-acceptance scratch is retained beneath
`target/wu0e-runner-hardening-acceptance/`; successful scratch is removed.
