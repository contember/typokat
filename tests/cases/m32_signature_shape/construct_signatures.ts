// M32 - object/interface construct signatures with optional and rest parameters.
// Cross-checked with tsc 6.0.3 --strict.

interface OptionalCtor {
  new (a: number, b?: string): { kind: "optional" };
}
declare const optionalCtor: OptionalCtor;
const oc1: { kind: "optional" } = new optionalCtor(1);
new optionalCtor(); // error[TK2554]: Expected 1-2 arguments, but got 0
new optionalCtor(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

interface RestCtor {
  new (a: number, ...rest: string[]): { kind: "rest" };
}
declare const restCtor: RestCtor;
const rc1: { kind: "rest" } = new restCtor(1, "x");
new restCtor(); // error[TK2555]: Expected at least 1 arguments, but got 0
new restCtor(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
