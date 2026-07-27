// Backlog 87 — the regression net for the acceptance bar: every chain the checker
// actually produces today is far below the cap (the deepest measured across the whole
// conformance corpus and the official-suite corpus is 6), so the cap must be invisible
// here. One fixture per wrapper variant that recurses — property, parameter, return
// type, array element — each pinned on the line the cap would first alter.

const property: { a: { b: number } } = { a: { b: "x" } }; // error[TK2322]: Types of property 'b' are incompatible.

declare const fn: (cb: (value: string) => void) => void;
const parameter: (cb: (value: number) => void) => void = fn; // error[TK2322]: Types of parameters 'value' are incompatible.

declare const ret: () => { a: string };
const returnType: () => { a: number } = ret; // error[TK2322]: Call signature return types are incompatible.

declare const arrays: string[][][];
const arrayElement: number[][][] = arrays; // error[TK2322]: Type 'string[]' is not assignable to type 'number[]'.
