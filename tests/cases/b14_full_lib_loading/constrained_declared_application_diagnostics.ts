// WU5 - lazy constrained applications retain their occurrence-owned diagnostics.
// Cross-checked with tsc 6.0.3 --strict: only the four TS2344 diagnostics below.

interface ConstrainedBox<T extends number> {
  value: T;
}

declare const validBox: ConstrainedBox<1>;
declare const firstInvalidBox: ConstrainedBox<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
declare const secondInvalidBox: ConstrainedBox<string>; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'

declare function readBox<T extends number>(value: ConstrainedBox<T>): T;
const validValue: 1 = readBox(validBox);

declare function invalidParameter(value: ConstrainedBox<string>): void; // error[TK2344]: Type 'string' does not satisfy the constraint 'number'

interface DependentPair<A, B extends A> {
  first: A;
  second: B;
}

declare const validPair: DependentPair<number, 1>;
declare const invalidPair: DependentPair<string, number>; // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
