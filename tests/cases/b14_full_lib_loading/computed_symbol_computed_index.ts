// tsc 6.0.3 --strict --target es2025: TS2322 x5 below. Computed and dotted
// access to the certified Symbol binding name the same exact well-known keys.

interface B14ComputedSymbolIterator {
  [Symbol.iterator](): number;
  [Symbol.asyncIterator](): string;
}

declare const b14ComputedSymbolIterator: B14ComputedSymbolIterator;

const b14ComputedSymbolIteratorWrong: string = b14ComputedSymbolIterator[Symbol["iterator"]](); // error[TK2322]
const b14ComputedAsyncIteratorWrong: number = b14ComputedSymbolIterator[Symbol["asyncIterator"]](); // error[TK2322]
const b14ParenthesizedSymbolIteratorWrong: string = b14ComputedSymbolIterator[Symbol[("iterator")]](); // error[TK2322]
const b14ParenthesizedSymbolObjectWrong: string = b14ComputedSymbolIterator[(Symbol)["iterator"]](); // error[TK2322]
const b14ParenthesizedSymbolKeyWrong: string = b14ComputedSymbolIterator[(Symbol["iterator"])](); // error[TK2322]
