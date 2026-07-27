// Backlog 101 — `&&` is `falsy-part-of(left) | right`, `||` is
// `truthy-part-of(left) | right`, and `??` is `non-nullish-part-of(left) | right`.
// Each is a real value type, so the annotation mismatch downstream is reported.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x9.

declare const flag: boolean;
declare const other: boolean;
declare const n: number;
declare const s: string;
declare const nn: number | null;
declare const nu: number | undefined;
declare const obj: { size: number };

// `&&` — the left survives only in its falsy shape; an always-truthy left (an
// object) contributes nothing, so the result is exactly the right operand.
const andBoolean: string = flag && other; // error[TK2322]: Type 'boolean' is not assignable to type 'string'
const andObject: string = obj && n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const andMixed: boolean = flag && n; // error[TK2322]

// `||` — the left survives only in its truthy shape, so a nullish left drops its
// `null`/`undefined` member.
const orBoolean: string = flag || other; // error[TK2322]: Type 'boolean' is not assignable to type 'string'
const orNullable: string = nn || n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const orMixed: boolean = s || n; // error[TK2322]

// `??` — only `null`/`undefined` are removed from the left.
const coalesceNull: string = nn ?? n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const coalesceUndefined: string = nu ?? n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const coalesceMixed: boolean = s ?? n; // error[TK2322]
