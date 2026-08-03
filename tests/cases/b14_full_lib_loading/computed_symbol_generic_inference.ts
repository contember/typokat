// tsc 6.0.3 --strict --target es2025: TS2322 x2 and TS2345 x3 below.

interface B14NamedSourceIter<T> {
  value: T;
}

interface B14NamedTargetIter<T> {
  value: T;
}

interface B14NamedSource<T> {
  iterator(): B14NamedSourceIter<T>;
}

interface B14NamedTarget<T> {
  iterator(): B14NamedTargetIter<T>;
}

declare const b14NamedStrings: B14NamedSource<string>;
declare function b14NamedTake<U>(source: B14NamedTarget<U>): U;
const b14NamedTakeClean: string = b14NamedTake(b14NamedStrings);
const b14NamedTakeWrong: number = b14NamedTake(b14NamedStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'

interface B14ComputedSourceIter<T> {
  value: T;
}

interface B14ComputedTargetIter<T> {
  value: T;
}

interface B14ComputedSource<T> {
  [Symbol.iterator](): B14ComputedSourceIter<T>;
}

interface B14ComputedTarget<T> {
  [Symbol.iterator](): B14ComputedTargetIter<T>;
}

declare const b14ComputedStrings: B14ComputedSource<string>;
declare function b14ComputedTake<U>(source: B14ComputedTarget<U>): U;
declare function b14RequireNumber(value: number): void;
const b14ComputedTakeClean: string = b14ComputedTake(b14ComputedStrings);
const b14ComputedTakeWrong: number = b14ComputedTake(b14ComputedStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'
b14RequireNumber(b14ComputedTake(b14ComputedStrings)); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

interface B14AsyncSourceIter<T> {
  value: T;
}

interface B14AsyncSource<T> {
  [Symbol.asyncIterator](): B14AsyncSourceIter<T>;
}

declare const b14AsyncStrings: B14AsyncSource<string>;
b14ComputedTake(b14AsyncStrings); // error[TK2345]

interface B14AsyncTargetIter<T> {
  value: T;
}

interface B14AsyncTarget<T> {
  [Symbol.asyncIterator](): B14AsyncTargetIter<T>;
}

declare function b14AsyncTake<U>(source: B14AsyncTarget<U>): U;
b14AsyncTake(b14ComputedStrings); // error[TK2345]
