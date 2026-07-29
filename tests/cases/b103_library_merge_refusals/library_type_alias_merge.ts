// Backlog 103 correctness tier, incompatible alias collision control. Type aliases do not merge;
// the private epoch must process this collision while preserving the library alias downstream.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2300 twice (once inside
// lib.es5.d.ts, once here) plus TS2322 on the read, because the library alias wins there too.
// typokat reproduces the TS2322 and has no TS2300 equivalent; ledgered in
// docs/reference/divergences.md under backlog 103.
type Partial<T> = { b103: T };

const partialNumber: Partial<number> = { b103: 1 }; // error[TK2322]
