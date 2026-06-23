// M9 — generic functions with EXPLICIT type arguments (inference is M10).
// A generic function is instantiated by substituting its type parameters; the
// instantiated parameter/return types then drive the usual relation checks.

function identity<T>(x: T): T { return x; }

const a: number = identity<number>(5);   // ok
const b: number = identity<string>("s"); // error[TK2322]
// ^ instantiated return is string, not number
const c = identity<number>("s");         // error[TK2345]
// ^ argument "s" not assignable to instantiated parameter number

function pick<A, B>(a: A, b: B): A { return a; }

const d: number = pick<number, string>(1, "x"); // ok
const e: number = pick<string, number>("x", 1); // error[TK2322]
// ^ instantiated return is string
