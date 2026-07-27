// Backlog 101 — the value must reach every consuming position, not just a variable
// annotation: call arguments, `return`, nested initializers, and a further operator.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2345 x3, TS2322 x7, TS2362 x1
// — same lines, except that tsc reports the contextually typed `return` ternary
// once per ARM (two diagnostics on one line) where typokat reports one at the
// returned expression. Same verdict, one diagnostic fewer, under-reporting only in
// duplication.

declare const flag: boolean;
declare const n: number;
declare const m: number;
declare const s: string;
declare const nn: number | null;
declare function wantsString(value: string): void;

wantsString(flag ? n : m); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
wantsString(nn ?? n); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
wantsString(flag && n); // error[TK2345]

function fromTernaryReturn(): string {
  return flag ? n : m; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

function fromCoalesceReturn(): string {
  return nn ?? n; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

interface Holder {
  value: string;
}

const held: Holder = { value: flag ? n : m }; // error[TK2322]
const elements: string[] = [nn ?? n]; // error[TK2322]

// The result flows on into the arithmetic operand rule (backlog 45): a `number`
// ternary satisfies it, a `number | string` one does not.
const chained: string = (flag ? n : m) * 2; // error[TK2322]: Type 'number' is not assignable to type 'string'
const chainedBadOperand: string = (flag ? n : s) * 2; // error[TK2322]: Type 'number' is not assignable to type 'string' | error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
