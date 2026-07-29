// Backlog 103 correctness acceptance: the `tooling/full-lib-bench/workloads/fanout/` shape — one
// colliding script augmentation plus a fan of external modules that only read the library. The
// augmentation must merge once and every module must observe it without disturbing other reads.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean.
interface Array<T> {
  fullLibBenchFanout(): T;
}
