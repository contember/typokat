// M32 - function rest parameters and object/interface call-signature rest.
// Cross-checked with tsc 6.0.3 --strict.

function collect(a: number, ...parts: string[]): void {}
collect(1);
collect(1, "a", "b");
collect(); // error[TK2555]: Expected at least 1 arguments, but got 0
collect(1, "a", 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

function defaultBeforeRest(a: number = 1, ...parts: string[]): void {}
defaultBeforeRest();
defaultBeforeRest(1, "x");
defaultBeforeRest(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

interface Callable {
  (a: number, b?: string): string;
}
declare const callable: Callable;
const callableOk: string = callable(1);
callable(); // error[TK2554]: Expected 1-2 arguments, but got 0
callable(1, "x", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3

interface RestCallable {
  (a: number, ...rest: string[]): void;
}
declare const restCallable: RestCallable;
restCallable(1, "a");
restCallable(); // error[TK2555]: Expected at least 1 arguments, but got 0
restCallable(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
