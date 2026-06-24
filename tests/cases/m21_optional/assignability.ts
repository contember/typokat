// M21 — optional vs required in type-to-type assignability (both directions).
//
// An optional member's effective type is `T | undefined`, and it may be absent. So a
// REQUIRED source satisfies an OPTIONAL target, but an OPTIONAL source does NOT satisfy
// a REQUIRED target (the member might be absent / undefined). All targets are object
// types -> code-only TK2322.

let req: { a: number } = { a: 1 };
let opt: { a?: number } = {};
let both: { a?: number } = {};

opt = req;  // ok — a required `a` satisfies an optional `a`
both = opt; // ok — both optional
opt = both; // ok
req = opt;  // error[TK2322] — an optional `a` may be absent in a required target
