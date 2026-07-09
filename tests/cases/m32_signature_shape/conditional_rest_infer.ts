// M32 - rest-based conditional infer patterns. These are the idiomatic forms
// avoided in M25 while rest elements were out of the type model. Cross-checked
// with tsc 6.0.3 --strict.

type Head<T> = T extends [infer H, ...unknown[]] ? H : never;
const h1: Head<[string, number]> = "x";
const h2: Head<[string, number]> = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

type Tail<T> = T extends [unknown, ...infer R] ? R : never;
const ta1: Tail<[string, number, boolean]> = [1, true];
const ta2: Tail<[string, number, boolean]> = [true, true]; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

type Args<T> = T extends (...args: infer A) => unknown ? A : never;
const a1: Args<(x: string, y: number) => void> = ["x", 1];
const a2: Args<(x: string, y: number) => void> = [1, 1]; // error[TK2322]: Type 'number' is not assignable to type 'string'

type RestReturn<T> = T extends (...args: never[]) => infer R ? R : never;
const r1: RestReturn<(x: string) => number> = 1;
const r2: RestReturn<(x: string) => number> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'
