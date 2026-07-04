// M24 — constraint-aware inference: when the inferred candidate violates the
// constraint, tsc falls back to the constraint and the ARGUMENT check reports
// TK2345 (not TK2344). tsc 6.0.3 --strict cross-checked.

interface HasX { x: number; }

function g<T extends string>(v: T): T { return v; }
g("lit");
g(5); // error[TK2345]: Argument of type 'number' is not assignable to parameter of type 'string'

class Box<T extends HasX> {
  constructor(public val: T) {}
}
new Box({ x: 1 });
new Box("s"); // error[TK2345]: Argument of type 'string'
