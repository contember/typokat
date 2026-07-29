// Backlog 103 correctness acceptance: the exact `tooling/full-lib-bench/workloads/collision/`
// shape. The augmentation must merge through the private collision epoch.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean.
interface Array<T> {
  fullLibBenchFirst(): T;
}
