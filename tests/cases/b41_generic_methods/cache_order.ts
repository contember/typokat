// Backlog 41 — recursive generic method relation must be independent of prior
// incompatible relation/cache entries and of declaration/query order.
// Cross-checked with tsc 6.0.3 --strict.

interface RecursiveLeft {
  next: RecursiveLeft;
  map<T>(value: T): T;
}

interface RecursiveRight {
  next: RecursiveRight;
  map<U>(value: U): U;
}

interface RecursiveNumber {
  next: RecursiveNumber;
  map<T extends number>(value: T): T;
}

interface RecursiveString {
  next: RecursiveString;
  map<U extends string>(value: U): U;
}

declare const recursiveLeft: RecursiveLeft;
declare const recursiveRight: RecursiveRight;
declare const recursiveNumber: RecursiveNumber;
declare const recursiveString: RecursiveString;

const cacheFailureFirst: RecursiveNumber = recursiveString; // error[TK2322]
const cacheSuccessAfterFailure: RecursiveRight = recursiveLeft;
const cacheSuccessReverse: RecursiveLeft = recursiveRight;
const cacheFailureLast: RecursiveString = recursiveNumber; // error[TK2322]
