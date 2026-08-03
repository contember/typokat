// tsc 6.0.3 --strict --target es2025, files in 00/99 order: TS2322 x4 below.

interface B14PublishedSymbolOverloaded<T> {
  [Symbol.iterator](kind: "many"): B14PublishedSymbolItem<T[]>;
}

interface B14PublishedSymbolBoth<T> {
  [Symbol.iterator](): B14PublishedSymbolItem<T>;
  [Symbol.asyncIterator](): B14PublishedSymbolItem<number>;
}

interface B14PublishedSymbolDerived<T> extends B14PublishedSymbolBase<T> {
  own: T;
}

interface B14PublishedSymbolAugmented<T> {
  [Symbol.iterator](): B14PublishedSymbolItem<T>;
}

interface B14PublishedSingleDemand<T> {
  [Symbol.iterator](kind: "single"): B14PublishedSymbolItem<T>;
}

interface B14PublishedManyDemand<T> {
  [Symbol.iterator](kind: "many"): B14PublishedSymbolItem<T[]>;
}

interface B14PublishedSyncDemand<T> {
  [Symbol.iterator](): B14PublishedSymbolItem<T>;
}

interface B14PublishedAsyncDemand<T> {
  [Symbol.asyncIterator](): B14PublishedSymbolItem<T>;
}

declare function b14PublishedTakeSync<T>(value: B14PublishedSyncDemand<T>): T;
declare function b14PublishedTakeAsync<T>(value: B14PublishedAsyncDemand<T>): T;

declare const b14PublishedOverloadedStrings: B14PublishedSymbolOverloaded<string>;
const b14PublishedSingleOverload: B14PublishedSingleDemand<string> = b14PublishedOverloadedStrings;
const b14PublishedManyOverload: B14PublishedManyDemand<string> = b14PublishedOverloadedStrings;

declare const b14PublishedBothStrings: B14PublishedSymbolBoth<string>;
const b14PublishedSyncClean: string = b14PublishedTakeSync(b14PublishedBothStrings);
const b14PublishedSyncWrong: number = b14PublishedTakeSync(b14PublishedBothStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'
const b14PublishedAsyncClean: number = b14PublishedTakeAsync(b14PublishedBothStrings);
const b14PublishedAsyncWrong: string = b14PublishedTakeAsync(b14PublishedBothStrings); // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const b14PublishedDerivedStrings: B14PublishedSymbolDerived<string>;
const b14PublishedHeritageClean: string = b14PublishedTakeSync(b14PublishedDerivedStrings);
const b14PublishedHeritageWrong: number = b14PublishedTakeSync(b14PublishedDerivedStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'

declare const b14PublishedAugmentedStrings: B14PublishedSymbolAugmented<string>;
const b14PublishedAugmentedClean: string = b14PublishedTakeSync(b14PublishedAugmentedStrings);
const b14PublishedAugmentedWrong: number = b14PublishedTakeSync(b14PublishedAugmentedStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'
