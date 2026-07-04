// M24 — explicit type arguments are checked against the parameter's constraint
// (TK2344 on the offending type argument). tsc 6.0.3 --strict cross-checked.
// Note: a bad type argument still instantiates (tsc behavior), so the value
// check against the instantiated type does not double-report.

interface HasX { x: number; }

function f<T extends HasX>(t: T): T { return t; }

f<{ x: number }>({ x: 1 });
f<{ x: number; y: string }>({ x: 1, y: "s" });
f<{ y: string }>({ y: "s" }); // error[TK2344]
f<string>("s"); // error[TK2344]: Type 'string' does not satisfy the constraint

function g<T extends string>(v: T): T { return v; }
g<number>(5); // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
g<"lit">("lit");

class Box<T extends HasX> {
  constructor(public val: T) {}
}
new Box<{ x: number }>({ x: 1 });
new Box<string>("s"); // error[TK2344]

interface IBox<T extends HasX> { val: T; }
type TA<T extends HasX> = { val: T };
const okI: IBox<{ x: number }> = { val: { x: 1 } };
const badI: IBox<string> = { val: "s" }; // error[TK2344]
const badT: TA<number> = { val: 5 }; // error[TK2344]

// An unresolvable constraint reports at the annotation (same as any other
// unresolved name — tsc behavior) and records NO constraint: explicit args
// are then unchecked (no TK2344 cascade), inference proceeds.
function h<T extends Bogus>(t: T): T { return t; } // error[TK2304]: Cannot find name 'Bogus'
h<number>(5);
const hv = h("s");
