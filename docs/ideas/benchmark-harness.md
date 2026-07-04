# Benchmark harness (the instrument for ADR-0001's profiling gate)

**Proposal.** A small, committed benchmark harness — criterion (or hyperfine over the release
binary) + a fixture set of real-world-shaped inputs (an ordinary-code file, a narrowing-heavy
file, a type-level-heavy file once `09`–`12` land) + recorded `tsc`/`tsgo` baselines.

**Why now-ish.** Three places in the plan gate decisions on measurement, but no instrument
exists: ADR-0001 makes the bytecode VM *profiling-gated* (backlog `13`); architecture §3.3 says
"benchmark canonicalization early"; §6.2 calls the relation-cache lifetime a measurable trap.
The headline goal is "beyond tsgo" — without a harness the gate in `13` gets decided by feel.
Cheap to build before the type-level phase (`09`–`12`); the perf-acceptance criteria those items
already carry (memoize / accumulator / work-stack / intrinsics) need something to run against.

**Sketch.** `tooling/bench/` mirroring the official-suite pattern (black-box over the release
binary; separate from unit tests); a `--save`-style ratchet is optional — wall-clock is noisy,
so record medians + machine info, don't hard-gate CI on it.

<!-- Origin: architecture assessment, session 2026-07-04. Graduates to a backlog item when
     someone commits to building it (naturally before or alongside backlog 09). -->
