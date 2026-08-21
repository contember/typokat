# Same-tree verifier

`verify.py` runs an explicit caller-supplied list of verification gates against one unchanged Git
tree. It is a transparent executor, not a mutable gate registry or an automatic test selector.

Pass each command as a separate `--gate` value:

```sh
python3 tooling/verify/verify.py \
  --gate 'cargo test -p typokat-core' \
  --gate 'cargo test' \
  --gate 'cargo test --test conformance conformance -- --exact --nocapture'
```

The command prints three JSON-line records before execution:

- `requested` contains the exact supplied command strings.
- `resolved` contains the `shlex`-parsed argument vectors that will execute.
- `tree` contains `HEAD` and a SHA-256 identity. The identity hashes staged and unstaged diffs
  separately. It also hashes every untracked file's name, regular-file or symlink kind,
  permissions, and content or link target.

Use `--dry-run` to inspect these records without executing a gate:

```sh
python3 tooling/verify/verify.py --dry-run --gate 'cargo test'
```

The verifier removes repeated argument vectors without reordering them. An exact plain `cargo test`
can also subsume exact plain `cargo test -p NAME` and `cargo test --package NAME` commands in the
same invocation. Before removing one, the verifier runs
`cargo metadata --no-deps --format-version 1 --locked` and proves that `NAME` uniquely identifies a
default workspace member. `--locked` prevents resolution from creating or updating `Cargo.lock`.
The verifier retains nonmembers, malformed selectors, ambiguous metadata, metadata failures, and
every command with filters, target flags, features, examples, harness arguments, visible receipts,
or unknown syntax.

Commands execute directly as argument vectors from the repository root, without a shell. Shell
operators are ordinary arguments. The verifier returns a gate's nonzero status and maps a signal to
the conventional `128 + signal` exit status. It rejects the run if `HEAD`, tracked content, or an
untracked regular file or symlink's name, kind, permissions, or content changes before or after any
gate.

The identity covers Git-representable state only. Git does not report special filesystem objects
such as FIFOs or sockets as untracked files. Those objects are out of scope, and the verifier does
not traverse the filesystem to discover them. If Git ever returns an unsupported kind in its
untracked-name list, the verifier rejects it.

Run the verifier tests with:

```sh
python3 -m unittest discover -s tooling/verify -p 'test_*.py'
```
