// M11 — class fields + constructor + `new` + instance member access.
// A class is both a TYPE (the instance type = its fields/methods) and a VALUE
// (the constructor). `new C(args)` checks the constructor signature and yields
// the instance type.

class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}

const p = new Point(1, 2);
const a: number = p.x;       // ok
const b: string = p.x;       // error[TK2322]
const c = p.z;               // error[TK2339]: Property 'z' does not exist
const d = new Point(1);      // error[TK2554]: Expected 2 arguments, but got 1
const e = new Point(1, "s"); // error[TK2345]

class OptionalCtor {
  constructor(x: number, y?: string) {}
}
new OptionalCtor(1);
new OptionalCtor();                // error[TK2554]: Expected 1-2 arguments, but got 0
new OptionalCtor(1, "s", "extra"); // error[TK2554]: Expected 1-2 arguments, but got 3
new OptionalCtor(1, 2);            // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

class RestCtor {
  constructor(x: number, ...parts: string[]) {}
}
new RestCtor(1, "a");
new RestCtor();                    // error[TK2555]: Expected at least 1 arguments, but got 0
new RestCtor(1, 2);                // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'
