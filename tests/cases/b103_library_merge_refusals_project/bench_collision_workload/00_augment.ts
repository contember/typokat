// Backlog 103 acceptance: the exact `tooling/full-lib-bench/workloads/collision/` shape, which
// exited 101 before the guard. It must now complete as a recorded refusal, never a panic.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` is clean. typokat refuses the merge and
// over-reports TK2339 on the augmented call; ledgered in docs/reference/divergences.md under
// backlog 103.
interface Array<T> { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  fullLibBenchFirst(): T;
}
