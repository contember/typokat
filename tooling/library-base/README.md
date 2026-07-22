# Frozen-library-base release gate

`verify.py` is the fail-closed external coordinator for WU3's production
`FrozenLibraryBase`. It creates two distinct clean clones of committed `HEAD`,
performs two offline release libtest builds with canonical path remapping, and
requires their selected test executables to be byte-identical.

The coordinator then runs exactly
`library::snapshot_base_spec::frozen_library_base_release_probe_once` in 45 new
process groups: three nonoverlapping windows, each containing five warmups and
ten recorded samples. Both independent binaries are sampled. Every process must
emit exactly one `TYPOKAT_LIBRARY_BASE_PROBE=` JSON record, use the production
frozen-base route, publish one stable typed-validation identity, and report zero
compiler, generator, or library-source work.

Coordinator `monotonic_ns` intervals around the complete external launcher are
the wall-time authority; GNU `/usr/bin/time -v` is used only for peak RSS. This
avoids its centisecond wall display weakening the exact 120 ms boundary. The
gate recomputes nearest-rank p95 from all raw sample records and separately for
each window. Every p95 must be at most 120 ms, and every warmup and sample must
remain at or below 512 MiB RSS.

The absolute `/usr/bin/python3` interpreter uses an isolated `-I -S` exec
wrapper which records its PID immediately before becoming the libtest. Thus the
evidence distinguishes the GNU-time wrapper/PGID from the 45 real libtest PIDs.
Every launch seals the libtest identity immediately before and after execution.
Child output is capped live, every command has a timeout, surviving process
groups are killed, and capped failure records remain in NO-GO evidence.

The coordinator resolves Cargo and rustc through the account's absolute rustup
installation and pinned `1.95.0` toolchain without consulting caller `PATH`.
Evidence retains tool paths, versions, byte identities, exact commands and
environments, Git commit/tree/status, tracked-source identities before/after,
physical build-root identities, and both release-binary identities. Only a
small fixed environment reaches children; Cargo is offline and receives
isolated homes and targets.

Run the frozen stdlib-only adversary suite first:

```sh
/usr/bin/python3 -m unittest tooling/library-base/test_verify.py -v
```

Run the complete release gate from a clean committed tree:

```sh
/usr/bin/python3 tooling/library-base/verify.py
```

The ignored Rust integration boundary runs both commands and pins the single
summary line:

```sh
cargo test --test library_base_release_gate \
  canonical_frozen_library_base_retains_release_headroom \
  -- --ignored --exact --nocapture
```

Each run retains raw or partial JSON under the ignored `evidence/` directory.
A completed file can be validated again from raw retained records without
trusting any retained summary (live physical roots are additionally checked
during the authoritative run):

```sh
/usr/bin/python3 tooling/library-base/verify.py --inspect-evidence \
  tooling/library-base/evidence/run-YYYYMMDDTHHMMSSZ-PID.json
```

The gate performs no snapshot generation and supplies no source/compiler
control variables to the timed child. A dirty source root, different release
binary, malformed probe, missing process, reused PID, unstable typed-validation identity,
internal-only timing, or derived-threshold failure is a NO-GO.
