// Backlog 45 — which operand types satisfy the arithmetic operand rule. Every line
// keeps the `number` result, so a wrong annotation is reported on the accepted rows
// too. tsc 6.0.3 --strict --target es2025 for this file: TS2322 x17, TS2362 x9, plus
// the deferred strict-null / unknown family (TS18047 x2, TS18046 x1) noted inline.

declare const anyValue: any;
declare const neverValue: never;
declare const numberValue: number;
declare const literalUnion: 1 | 2;
declare const numberOrString: number | string;
declare const booleanValue: boolean;
declare const objectValue: { size: number };
declare const templateValue: `id-${number}`;
declare const unknownValue: unknown;
declare const numberOrNull: number | null;
declare const stringOrNull: string | null;
declare function returnsVoid(): void;

class Box {
  size = 1;
}
declare const box: Box;

// Accepted operands — no TK2362/TK2363, but the `number` result is still checked.
const fromAny: string = anyValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const fromNever: string = neverValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const fromNumber: string = numberValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const fromLiteral: string = 2 * 3; // error[TK2322]: Type 'number' is not assignable to type 'string'
const fromLiteralUnion: string = literalUnion * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Rejected operands.
const fromNumberOrString: string = numberOrString * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const fromBoolean: string = booleanValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const fromObject: string = objectValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const fromClass: string = box * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const fromTemplate: string = templateValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
const fromVoid: string = returnsVoid() * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.

// `null`/`undefined` are stripped from the operand before the numeric test — the
// same shape as tsc's `checkNonNullType`. Reporting the nullish operand itself is
// the deferred strict-null-receiver family (tsc: TS18047 here, not implemented),
// so the `number | null` row carries only the result mismatch.
const fromNumberOrNull: string = numberOrNull * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const fromStringOrNull: string = stringOrNull * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.

// An `unknown` operand is owned by the deferred unknown-receiver family (tsc:
// TS18046, not implemented), so no operand diagnostic is emitted here.
const fromUnknown: string = unknownValue * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'

function constrained<T extends number>(value: T): string {
  return value * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
}
function looselyConstrained<T extends number | string>(value: T): string {
  return value * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
}
function unconstrained<T>(value: T): string {
  return value * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
}
