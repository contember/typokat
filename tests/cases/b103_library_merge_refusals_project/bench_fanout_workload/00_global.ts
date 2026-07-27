// Backlog 103 acceptance: the `tooling/full-lib-bench/workloads/fanout/` shape — one colliding
// script augmentation plus a fan of external modules that only read the library. It exited 101
// before the guard; the refusal must be recorded once, at the augmentation, and the modules
// must keep checking normally.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean.
interface Array<T> { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  fullLibBenchFanout(): T;
}
