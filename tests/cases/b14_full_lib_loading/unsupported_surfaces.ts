// tsc 6.0.3 --strict --target es2025: clean. Typokat must remain non-permissive while
// these model gaps are owned by backlogs 50/75; loading declarations cannot erase them.

let symbolValue: symbol; // incomplete[annotation-lower/symbol-keyword/self]
const bigintValue: bigint = 1n; // incomplete[annotation-lower/bigint-keyword/self] | incomplete[expr-infer/bigint-literal/self]
const objectValue: object = {};

function b14IsString(value: unknown): value is string { // incomplete[annotation-lower/type-predicate/self]
  return typeof value === "string";
}

const objectControl: Object = {};
