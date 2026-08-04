// tsc 6.0.3 --strict --target es2025: TS2322, TS7053 x3, TS2322 x3,
// TS18046, TS18047, TS18048, TS18047, then TS18048. The any and never controls are clean.
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

declare const b14ExactSymbolArray: number[];
const b14ExactArrayIteratorWrong: number = b14ExactSymbolArray[Symbol.iterator]; // incomplete[expr-infer/element-access/unsupported-symbol-receiver]

declare const b14ExactSymbolTuple: [number, string];
const b14ExactTupleIteratorWrong: number = b14ExactSymbolTuple[Symbol.iterator]; // incomplete[expr-infer/element-access/unsupported-symbol-receiver]

declare const b14ExactSymbolUnknown: unknown;
b14ExactSymbolUnknown[Symbol.iterator]; // incomplete[expr-infer/element-access/unknown-receiver]

declare const b14ExactSymbolNull: null;
b14ExactSymbolNull[Symbol.iterator]; // incomplete[expr-infer/element-access/nullish-receiver]

declare const b14ExactSymbolUndefined: undefined;
b14ExactSymbolUndefined[Symbol.iterator]; // incomplete[expr-infer/element-access/nullish-receiver]

declare const b14ExactSymbolMaybeNull: B14ExactSymbolIterator | null;
b14ExactSymbolMaybeNull[Symbol.iterator](); // incomplete[expr-infer/element-access/nullish-receiver]

declare const b14ExactSymbolMaybeUndefined: B14ExactSymbolIterator | undefined;
b14ExactSymbolMaybeUndefined[Symbol.iterator](); // incomplete[expr-infer/element-access/nullish-receiver]

declare const b14ExactSymbolAny: any;
b14ExactSymbolAny[Symbol.iterator];

declare const b14ExactSymbolNever: never;
b14ExactSymbolNever[Symbol.iterator];
