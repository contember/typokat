// Backlog 45 — negative controls. Every operator applied to well-typed operands and
// consumed at the correct type must stay clean; `number` is the result on every row.
// tsc 6.0.3 --strict --target es2025: 0 diagnostics.

declare const n: number;
declare const m: number;
declare const anyValue: any;
declare const literalUnion: 1 | 2;
declare function wantsNumber(value: number): void;

const subtract: number = n - m;
const multiply: number = n * m;
const divide: number = n / m;
const remainder: number = n % m;
const exponent: number = n ** m;
const bitAnd: number = n & m;
const bitOr: number = n | m;
const bitXor: number = n ^ m;
const shiftLeft: number = n << m;
const shiftRight: number = n >> m;
const shiftUnsigned: number = n >>> m;
const addition: number = n + m;

const fromAny: number = anyValue * m;
const fromLiteralUnion: number = literalUnion & 1;
const fromLiterals: number = 2 ** 8;

wantsNumber(n * m);
wantsNumber(n % 2 + 1);

function returnsNumber(): number {
  return n / 2;
}

const compared: boolean = n * 2 > m - 1;
const concatenated: string = "total: " + n * 2;
