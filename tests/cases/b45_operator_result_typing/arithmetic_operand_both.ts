// Backlog 45 — when BOTH operands violate the arithmetic operand rule, tsc reports
// one diagnostic per side (TK2362 and TK2363), not a single combined TK2365. The
// result is still `number`.
// tsc 6.0.3 --strict --target es2025: TS2362 x4, TS2363 x4, TS2322 x4.

declare const s: string;
declare const other: string;

const subtract: string = s - other; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const multiply: string = s * other; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const shifted: string = s << other; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const exponent: string = s ** other; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
