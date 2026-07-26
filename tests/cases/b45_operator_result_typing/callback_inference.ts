// Backlog 45 — the inference half of the defect. When the operator result was the
// error type a callback returning `value * 2` inferred `U = any`, so assigning the
// mapped array to a wrongly-typed array passed silently. With the real `number`
// result the mismatch is reported.
// tsc 6.0.3 --strict --target es2025: TS2322 x2, TS2345 x1, TS2362 x1.

declare function mapArray<T, U>(items: T[], project: (value: T) => U): U[];
declare function apply<T, R>(value: T, project: (value: T) => R): R;
declare function wantsStrings(values: string[]): void;

const numbers: number[] = [1, 2, 3];

const doubled: number[] = mapArray(numbers, (value) => value * 2);
const wrongDoubled: string[] = mapArray(numbers, (value) => value * 2); // error[TK2322]
wantsStrings(mapArray(numbers, (value) => value % 2)); // error[TK2345]

const shifted: string = apply(4, (value) => value << 1); // error[TK2322]: Type 'number' is not assignable to type 'string'

// The operand rule still applies inside the callback body.
const wrongOperand: number[] = mapArray(["a", "b"], (value) => value * 2); // error[TK2362]: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
