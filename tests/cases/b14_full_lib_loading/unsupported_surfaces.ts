// tsc 6.0.3 --strict --target es2025: clean. Typokat must remain non-permissive while
// the remaining model gaps are owned by backlogs 50/75; loading declarations cannot erase them.

let symbolValue: symbol;
const bigintValue: bigint = 1n; // incomplete[expr-infer/bigint-literal/self]
const objectValue: object = {};

function b14IsString(value: unknown): value is string { // incomplete[annotation-lower/type-predicate/self]
  return typeof value === "string";
}

const objectControl: Object = {};
