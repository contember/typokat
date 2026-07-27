// Backlog 101 — an arm's type is read under its own flow branch, so a guarded arm
// is narrowed and the opposite arm is not. The composed row is the backlog-100
// interaction: a `&&`-composed test must still narrow the consequent.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x5.

declare const flag: boolean;
declare const n: number;
declare const nn: number | null;
declare const su: string | number;

// The consequent sees the guard; the alternate does not.
const guarded: string = nn !== null ? nn : n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const inverted: string = nn === null ? n : nn; // error[TK2322]: Type 'number' is not assignable to type 'string'

// The un-narrowed arm keeps the nullish member, so a `number` target rejects it.
const alternateWide: number = nn !== null ? n : nn; // error[TK2322]

// A composed test narrows the consequent too (backlog 100): `nn` is `number` there,
// so this row is clean and only its wrong-target twin reports.
const composed: number = (nn !== null && flag) ? nn : n;
const composedWrong: string = (nn !== null && flag) ? nn : n; // error[TK2322]: Type 'number' is not assignable to type 'string'

// `&&`'s right operand is typed under the left's TRUE edge, so it cannot
// contribute `null`; `||`'s right operand is typed under the left's FALSE edge, so
// it cannot contribute `string`. Both stay clean; losing the narrow reports.
const andNarrowed: boolean | number = nn !== null && nn;
const orNarrowed: boolean | number = typeof su === "string" || su;

// The same two shapes at a target that would only admit the narrowed operand.
const andNarrowedTight: number = nn !== null && nn; // error[TK2322]
const orNarrowedTight: number = typeof su === "string" || su; // error[TK2322]
