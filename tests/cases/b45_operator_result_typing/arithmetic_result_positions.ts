// Backlog 45 — the operator result must reach every consuming position, not just a
// variable annotation. tsc 6.0.3 --strict --target es2025: TS2345 x2, TS2322 x5.

declare const n: number;
declare function wantsString(value: string): void;

wantsString(n * 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
wantsString(n << 1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

function fromReturn(): string {
  return n - 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

// The result flows on into a further operator: `*` yields `number`, so `+` sees
// number + number and yields `number` rather than absorbing into the error type.
const chained: string = n * 2 + 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

// ... and into a string concatenation, which yields `string`.
const concatenated: number = "n=" + n * 2; // error[TK2322]: Type 'string' is not assignable to type 'number'

interface Holder {
  value: string;
}
const held: Holder = { value: n % 3 }; // error[TK2322]

const elements: string[] = [n / 2]; // error[TK2322]
