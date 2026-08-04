// tsc 6.0.3 --strict --target es2025: TS7053 x3 below. Backlog 48 owns the
// diagnostic; until it ships, typokat must fail closed instead of returning error.

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
