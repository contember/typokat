// Backlog 45 — `+` keeps its own string/number rules, but a combination that
// satisfies none of them is now reported as TK2365 instead of silently becoming the
// error type. A string-like operand still makes the whole expression `string`, which
// is what turns the following annotation mismatches into real diagnostics.
// tsc 6.0.3 --strict --target es2025: TS2365 x4, TS2322 x4.

declare const s: string;
declare const n: number;
declare const b: boolean;
declare const shape: { size: number };
declare const anyValue: any;

// No rule applies: neither operand is number-like, string-like, or `any`.
const booleanPlusNumber: string = b + n; // error[TK2365]: Operator '+' cannot be applied to types 'boolean' and 'number'
const numberPlusBoolean: string = n + b; // error[TK2365]: Operator '+' cannot be applied to types 'number' and 'boolean'
const booleanPlusBoolean: string = b + b; // error[TK2365]: Operator '+' cannot be applied to types 'boolean' and 'boolean'
const shapePlusShape: string = shape + shape; // error[TK2365]

// One string-like operand wins: the result is `string`, whatever the other side is.
const stringPlusShape: number = s + shape; // error[TK2322]: Type 'string' is not assignable to type 'number'
const shapePlusString: number = shape + s; // error[TK2322]: Type 'string' is not assignable to type 'number'
const stringPlusBoolean: number = s + b; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Both number-like: `number`.
const numberPlusNumber: string = n + n; // error[TK2322]: Type 'number' is not assignable to type 'string'

// `any` on either side keeps the whole expression `any` — no diagnostic.
const anyPlusShape: string = anyValue + shape;
const shapePlusAny: number = shape + anyValue;
