// tsc 6.0.3 --strict --target es2025: TS7053 x3, then TS18046/TS18047/TS18048.
// Backlogs 48, 75, and 49 own the diagnostics; until they ship, typokat must fail
// closed instead of returning error.

interface B14SymbolIndexedExpression {
  [Symbol.iterator](): number;
}

declare const b14SymbolIndexedExpression: B14SymbolIndexedExpression;
const b14ExactSymbolIndex: number = b14SymbolIndexedExpression[Symbol.iterator]();

declare const b14ArbitrarySymbolIndex: symbol;
const b14ArbitrarySymbolFalseClean: string = b14SymbolIndexedExpression[b14ArbitrarySymbolIndex](); // incomplete[expr-infer/element-access/implicit-any-index]

declare const b14MixedSymbolIndexedExpression: B14SymbolIndexedExpression | string;
const b14MixedSymbolFalseClean: boolean = b14MixedSymbolIndexedExpression[b14ArbitrarySymbolIndex](); // incomplete[expr-infer/element-access/implicit-any-index]

declare const b14IntersectionSymbolIndexedExpression: B14SymbolIndexedExpression & {
  readonly tag: "intersection";
};
const b14IntersectionSymbolFalseClean: boolean = b14IntersectionSymbolIndexedExpression[b14ArbitrarySymbolIndex](); // incomplete[expr-infer/element-access/implicit-any-index]

declare const b14UnknownSymbolIndexedExpression: unknown;
b14UnknownSymbolIndexedExpression[b14ArbitrarySymbolIndex]; // incomplete[expr-infer/element-access/unknown-receiver]

declare const b14NullSymbolIndexedExpression: null;
b14NullSymbolIndexedExpression[b14ArbitrarySymbolIndex]; // incomplete[expr-infer/element-access/nullish-receiver]

declare const b14UndefinedSymbolIndexedExpression: undefined;
b14UndefinedSymbolIndexedExpression[b14ArbitrarySymbolIndex]; // incomplete[expr-infer/element-access/nullish-receiver]
