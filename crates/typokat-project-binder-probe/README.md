# Project-local binder measurement probe

This measurement-only crate authenticates the WU3 phase boundary:

```text
retained immutable declaration product + one prepared user AST -> one fresh project binder
```

The probe uses the committed `reflect_packaged.ts` WU0 project. Product construction and its one
user parse finish before measurement starts. Every sample constructs a new binder. The probe keeps
the returned binder alive until after the wall clock or heap snapshot is read. The binary
serializes the opaque `ProjectBinderPhaseReceiptV1` returned by `typokat-library`; it does not
assemble lifecycle counters itself.

Run the contract in the same release profile used for measurement:

```sh
cpu-lease run -n 2 --no-smt -- cargo test --release \
  -p typokat-project-binder-probe --test contract -- --nocapture
```

Run the DHAT phase-boundary contract separately because it uses a mutually exclusive allocator
feature:

```sh
cpu-lease run -n 2 --no-smt -- cargo test --release \
  -p typokat-project-binder-probe \
  --no-default-features --features dhat-heap \
  --test contract \
  heap_probe_excludes_setup_and_includes_measured_control_allocation \
  -- --exact --nocapture
```

Run wall samples with the production CLI allocator. Ten warm-ups and 31 samples are the defaults:

```sh
cpu-lease run -n 2 --no-smt -- cargo run --release \
  -p typokat-project-binder-probe -- wall
```

Run one DHAT heap sample with the measurement-only allocator:

```sh
cpu-lease run -n 2 --no-smt -- cargo run --release \
  -p typokat-project-binder-probe --no-default-features --features dhat-heap -- heap
```

Each command writes one JSON receipt to stdout. DHAT timing is not a performance result. The heap
contract runs equal-size setup and measured-phase controls to prove the reported heap interval.
