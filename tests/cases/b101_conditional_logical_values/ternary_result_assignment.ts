// Backlog 101 — a ternary's VALUE is the union of its arm types, not the error
// type, so an ordinary annotation mismatch downstream of it is reported instead of
// being absorbed. Rows whose arms share one type carry the full message; rows whose
// result is a union are code-only (union member order is not stable, and typokat
// names the offending member where tsc names the whole union).
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x8.

declare const flag: boolean;
declare const n: number;
declare const m: number;
declare const s: string;
declare const nv: never;

// Both arms the same type: the result is that type.
const same: string = flag ? n : m; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Differing arms: the union of both, so a target admitting neither or only one
// still reports.
const bothWrong: boolean = flag ? n : s; // error[TK2322]
const onlyNumberOk: number = flag ? n : s; // error[TK2322]
const onlyStringOk: string = flag ? n : s; // error[TK2322]

// Literal arms stay literals: `1 | 2` is assignable to `number` and to `1 | 2`,
// but not to `string`.
const literalArms: string = flag ? 1 : 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const stringLiteralArms: number = flag ? "x" : "y"; // error[TK2322]

// `never` is the identity element of `|`, so a `never` arm contributes nothing.
const neverArm: string = flag ? nv : n; // error[TK2322]: Type 'number' is not assignable to type 'string'

// The test is still walked for its own diagnostics.
const badTest: number = missingName ? n : m; // error[TK2304]: Cannot find name 'missingName'
