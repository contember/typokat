// tsc 6.0.3 --strict --target es2025: TS7053 x1 below. Backlog 48 owns the
// diagnostic; until it ships, typokat must fail closed instead of returning error.

interface B14SymbolIndexedExpression {
  [Symbol.iterator](): number;
}

declare const b14SymbolIndexedExpression: B14SymbolIndexedExpression;
const b14ExactSymbolIndex: number = b14SymbolIndexedExpression[Symbol.iterator]();

declare const b14ArbitrarySymbolIndex: symbol;
const b14ArbitrarySymbolFalseClean: string = b14SymbolIndexedExpression[b14ArbitrarySymbolIndex](); // incomplete[expr-infer/element-access/implicit-any-index]
