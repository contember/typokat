// M32 - optional/default parameters in calls, constructors, and default
// initializer checks. Cross-checked with tsc 6.0.3 --strict.

function optionalOne(a: number, b?: string): string { return "ok"; }
optionalOne(1);
optionalOne(1, "x");
optionalOne(); // error[TK2554]: Expected 1-2 arguments, but got 0
optionalOne(1, "x", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3
optionalOne(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

function defaultOne(a: number, b: string = "x"): string { return b; }
defaultOne(1);
defaultOne(); // error[TK2554]: Expected 1-2 arguments, but got 0
defaultOne(1, 2); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

function badDefault(a: number = "s"): void {} // error[TK2322]: Type 'string' is not assignable to type 'number'

class Base {
  constructor(a: number, b?: string) {}
}
new Base(1);
new Base(); // error[TK2554]: Expected 1-2 arguments, but got 0
new Base(1, "x", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3

class Child extends Base {
  constructor() {
    super(); // error[TK2554]: Expected 1-2 arguments, but got 0
  }
}
