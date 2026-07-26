// Backlog 45 — the mirror of `arithmetic_operand_left.ts`: a non-numeric RIGHT
// operand reports TK2363 at that operand.
// tsc 6.0.3 --strict --target es2025: TS2363 x11 and TS2322 x11.

declare const s: string;
declare const n: number;

const subtract: string = n - s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const multiply: string = n * s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const divide: string = n / s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const remainder: string = n % s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const exponent: string = n ** s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitAnd: string = n & s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitOr: string = n | s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitXor: string = n ^ s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftLeft: string = n << s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftRight: string = n >> s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftUnsigned: string = n >>> s; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
