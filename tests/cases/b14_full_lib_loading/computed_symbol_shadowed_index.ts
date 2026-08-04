// tsc 6.0.3 --strict --target es2025: TS2322 x3 below. A shadowed Symbol binding
// is ordinary user code: its literal-valued member must drive ordinary indexing.

export {};

declare const Symbol: {
  iterator: "local";
};

declare const b14ShadowedSymbolIndexed: {
  local: number;
};

const b14ShadowedSymbolIndexClean: number = b14ShadowedSymbolIndexed[Symbol.iterator];
const b14ShadowedSymbolIndexWrong: string = b14ShadowedSymbolIndexed[Symbol.iterator]; // error[TK2322]
const b14ShadowedComputedSymbolIndexClean: number = b14ShadowedSymbolIndexed[Symbol["iterator"]];
const b14ShadowedComputedSymbolIndexWrong: string = b14ShadowedSymbolIndexed[Symbol["iterator"]]; // error[TK2322]
const b14ShadowedParenthesizedSymbolIndexClean: number = b14ShadowedSymbolIndexed[Symbol[("iterator")]];
const b14ShadowedParenthesizedSymbolIndexWrong: string = b14ShadowedSymbolIndexed[Symbol[("iterator")]]; // error[TK2322]
