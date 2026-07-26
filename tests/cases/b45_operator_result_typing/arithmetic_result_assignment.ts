// Backlog 45 — every arithmetic/bitwise/shift binary operator produces `number`,
// so an ordinary annotation mismatch is reported instead of being absorbed by the
// error type. tsc 6.0.3 --strict --target es2025: TS2322 x13.

declare const n: number;

const subtract: string = n - 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const multiply: string = n * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const divide: string = n / 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const remainder: string = n % 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const exponent: string = n ** 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const bitAnd: string = n & 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const bitOr: string = n | 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const bitXor: string = n ^ 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const shiftLeft: string = n << 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const shiftRight: string = n >> 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const shiftUnsigned: string = n >>> 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const addition: string = n + 2; // error[TK2322]: Type 'number' is not assignable to type 'string'

// The result is the widened `number`, never a folded literal.
const folded: 6 = 2 * 3; // error[TK2322]: Type 'number' is not assignable to type '6'
