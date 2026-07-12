// Backlog 41 — generic function and constructor type annotations lower to
// callable signatures. Cross-checked with tsc 6.0.3 --strict.

interface Box<T> {
  value: T;
}

declare const identity: <T>(value: T) => T;

const functionInferred: number = identity(1);
const functionExplicit: string = identity<string>("value");
const functionWrongReturn: string = identity(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
identity<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
identity<number, string>(1); // error[TK2558]: Expected 1 type arguments, but got 2

declare const GenericBox: new <T>(value: T) => Box<T>;

const constructorInferred: Box<number> = new GenericBox(1);
const constructorExplicit: Box<string> = new GenericBox<string>("value");
const constructorWrongReturn: Box<string> = new GenericBox(1); // error[TK2322]
new GenericBox<string>(1); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
new GenericBox<number, string>(1); // error[TK2558]: Expected 1 type arguments, but got 2
