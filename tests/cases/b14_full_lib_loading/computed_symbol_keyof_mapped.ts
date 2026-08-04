// tsc 6.0.3 --strict --target es2025: TS2741 then TS2322 x2 below.

interface B14SymbolMappedSource {
  label: string;
  [Symbol.iterator](): void;
}

type B14SymbolMappedCopy<T> = { [K in keyof T]: T[K] };

declare const b14SymbolMappedCopy: B14SymbolMappedCopy<B14SymbolMappedSource>;
const b14SymbolMappedLabel: string = b14SymbolMappedCopy.label;
b14SymbolMappedCopy[Symbol.iterator]();

const b14SymbolMappedMissing: B14SymbolMappedCopy<B14SymbolMappedSource> = { // error[TK2741]
  label: "missing symbol",
};

declare const b14SymbolKey: keyof B14SymbolMappedSource;
const b14SymbolKeyImpossible: never = b14SymbolKey; // error[TK2322]: not assignable to type 'never'

declare const b14ArbitrarySymbolKey: symbol;
const b14SymbolKeyMustStayExact: keyof B14SymbolMappedSource = b14ArbitrarySymbolKey; // error[TK2322]
