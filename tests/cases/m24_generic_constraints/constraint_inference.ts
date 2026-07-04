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

// A NON-FRESH structural candidate that violates the constraint clamps too —
// a typed value cannot be contextually reshaped, so the argument check must
// report TK2345 against the (substituted) constraint, matching tsc.
declare function pick<T extends HasX>(t: T): T;
declare const ny: { y: number };
pick(ny); // error[TK2345]
declare function nums<T extends number[]>(t: T): T;
declare const strs: string[];
nums(strs); // error[TK2345]

// A FRESH object/array literal argument is the exception: tsc contextually
// retypes it against the constraint (here TS2353 excess property) — that
// contextual-typing pass is the documented out-of-scope deferral, so typokat
// stays silent (documented divergence, tests/cases/README.md).
pick({ y: 1 });
