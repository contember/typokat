// Union method calls must retain arity, receiver, generic, and overload semantics.

declare const tupleRestUnion:
  | { method(...args: [number]): string }
  | { method(...args: [number]): number };
const tupleRestValue: string | number = tupleRestUnion.method(1);
tupleRestUnion.method(); // error[TK2554]: Expected 1 arguments, but got 0

declare const optionalUnion:
  | { method(value?: number): string }
  | { method(value?: number): number };
const optionalValue: string | number = optionalUnion.method(1);

declare const arrayRestUnion:
  | { method(...values: number[]): string }
  | { method(...values: number[]): number };
const arrayRestValue: string | number = arrayRestUnion.method(1, 2);

declare const receiverUnion:
  | { a: 1; method(this: { a: 1 }): string }
  | { b: 1; method(this: { b: 1 }): number };
receiverUnion.method(); // error[TK2684]

declare const genericUnion:
  | { method<T>(): T }
  | { method<T>(): T };
const genericBad: boolean = genericUnion.method<number>(); // error[TK2322]

type FirstOverloads = {
  method(): string;
  method(value: number): string;
};
type SecondOverloads = {
  method(): number;
  method(value: number): number;
};
declare const overloadUnion: FirstOverloads | SecondOverloads;
const overloadBad: boolean = overloadUnion.method(); // error[TK2322]: Type 'string | number' is not assignable to type 'boolean'
const overloadStringArm: number = overloadUnion.method(); // error[TK2322]: Type 'string | number' is not assignable to type 'number'
