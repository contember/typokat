// Backlog 41 — class substitution rewrites method-local constraints and defaults
// before method instantiation. Cross-checked with tsc 6.0.3 --strict.

class Box<T> {
  map<U extends T>(value: U): U {
    return value;
  }
}

declare const numericBox: Box<number>;

const explicitNumber: number = numericBox.map<number>(1);
numericBox.map<string>("value"); // error[TK2344]: Type 'string' does not satisfy the constraint 'number'
numericBox.map("value"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'

declare class Defaults<T> {
  value<U = T>(): U;
}

const defaultWithoutContext = new Defaults<number>().value();
const defaultIsOuterNumber: number = defaultWithoutContext;
const defaultIsNotString: string = defaultWithoutContext; // error[TK2322]: Type 'number' is not assignable to type 'string'
