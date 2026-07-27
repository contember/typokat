// Backlog 103, the guard tier. A user `type Partial<T>` targets the frozen alias's group.
// Type aliases do not merge in TypeScript either, but the binder still reached the frozen
// group and panicked; now the refusal is recorded and the library alias keeps winning.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2300 twice (once inside
// lib.es5.d.ts, once here) plus TS2322 on the read, because the library alias wins there too.
// typokat reproduces the TS2322 and has no TS2300 equivalent; ledgered in
// docs/reference/divergences.md under backlog 103.
type Partial<T> = { b103: T }; // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global

const partialNumber: Partial<number> = { b103: 1 }; // error[TK2322]
