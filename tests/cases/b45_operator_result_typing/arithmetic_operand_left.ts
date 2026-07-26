// Backlog 45 — a non-numeric LEFT operand reports TK2362 at the operand, for every
// arithmetic/bitwise/shift operator. The result stays `number`, so the annotation
// mismatch is still reported alongside.
// tsc 6.0.3 --strict --target es2025: TS2362 x11 and TS2322 x11.

declare const s: string;
declare const n: number;

const subtract: string = s - n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const multiply: string = s * n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const divide: string = s / n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const remainder: string = s % n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const exponent: string = s ** n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitAnd: string = s & n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitOr: string = s | n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const bitXor: string = s ^ n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftLeft: string = s << n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftRight: string = s >> n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shiftUnsigned: string = s >>> n; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
