// Backlog 45 — DOCUMENTED DIVERGENCE. For `&`/`|`/`^` with two boolean operands tsc
// replaces the operand diagnostics with the single suggestion TS2447 ("The '&'
// operator is not allowed for boolean types. Consider using '&&' instead."). TK2447
// is not in typokat's code range, so typokat reports the underlying operand
// violation on each side instead — the same verdict, one diagnostic more, in the
// over-reporting direction.
// tsc 6.0.3 --strict --target es2025: TS2447 x3 and TS2362 x1 (`b & n`), no TS2322
// (the result is `number` on every line).

declare const b: boolean;
declare const n: number;

const both: number = b & b; // error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const either: number = b | b; // error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const exclusive: number = b ^ b; // error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type. | error[TK2363]: The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.

// Only ONE boolean operand: tsc already reports the ordinary operand diagnostic, so
// this row is exact parity.
const mixed: number = b & n; // error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
