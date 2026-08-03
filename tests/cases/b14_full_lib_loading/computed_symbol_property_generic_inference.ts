// tsc 6.0.3 --strict --target es2025: TS2322 x1 and TS2345 x1 below.
// Property signatures must use the same certified Symbol key as method signatures.

interface B14ComputedPropertySourceIter<T> {
  value: T;
}

interface B14ComputedPropertyTargetIter<T> {
  value: T;
}

interface B14ComputedPropertySource<T> {
  [Symbol.iterator]: () => B14ComputedPropertySourceIter<T>;
}

interface B14ComputedPropertyTarget<T> {
  [Symbol.iterator]: () => B14ComputedPropertyTargetIter<T>;
}

declare const b14ComputedPropertyStrings: B14ComputedPropertySource<string>;
declare function b14ComputedPropertyTake<U>(source: B14ComputedPropertyTarget<U>): U;
declare function b14ComputedPropertyRequireNumber(value: number): void;

const b14ComputedPropertyClean: string = b14ComputedPropertyTake(b14ComputedPropertyStrings);
const b14ComputedPropertyWrong: number = b14ComputedPropertyTake(b14ComputedPropertyStrings); // error[TK2322]: Type 'string' is not assignable to type 'number'
b14ComputedPropertyRequireNumber(b14ComputedPropertyTake(b14ComputedPropertyStrings)); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
