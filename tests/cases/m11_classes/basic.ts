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
