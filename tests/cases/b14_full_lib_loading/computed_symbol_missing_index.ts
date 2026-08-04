// tsc 6.0.3 --strict --target es2025: TS2322, TS7053 x3, then TS2322.
// Backlogs 48, 49, and 75 own exact-key element-access diagnostics and primitive
// library members. Until they ship, typokat must fail closed instead of returning error.

interface B14ExactSymbolIterator {
  [Symbol.iterator](): number;
}

interface B14ExactSymbolPlain {
  value: number;
}

declare const b14ExactSymbolIterator: B14ExactSymbolIterator;
declare const b14ExactSymbolPlain: B14ExactSymbolPlain;
declare const b14ExactSymbolMixed: B14ExactSymbolIterator | B14ExactSymbolPlain;

const b14ExactSymbolPresentWrong: string = b14ExactSymbolIterator[Symbol.iterator](); // error[TK2322]
const b14ExactSymbolMissing: string = b14ExactSymbolPlain[Symbol.iterator](); // incomplete[expr-infer/element-access/missing-symbol-key]
const b14ExactOtherSymbolMissing: string = b14ExactSymbolIterator[Symbol.asyncIterator](); // incomplete[expr-infer/element-access/missing-symbol-key]
const b14ExactUnionSymbolMissing: string = b14ExactSymbolMixed[Symbol.iterator](); // incomplete[expr-infer/element-access/missing-symbol-key]

declare const b14ExactSymbolText: string;
const b14ExactStringIteratorWrong: number = b14ExactSymbolText[Symbol.iterator](); // incomplete[expr-infer/element-access/unsupported-symbol-receiver]
