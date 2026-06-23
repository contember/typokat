// M10 — type-argument inference from call arguments (no explicit type args).
// The inference engine matches each parameter type against its argument type to
// fix the type parameters, then the instantiated signature is checked.

function identity<T>(x: T): T { return x; }

const a: number = identity(5);   // ok — T inferred from the argument
const b: string = identity(5);   // error[TK2322]
const c: number = identity("s"); // error[TK2322]

function pick<A, B>(a: A, b: B): A { return a; }

const d: number = pick(1, "x"); // ok — A inferred number
const e: string = pick(1, "x"); // error[TK2322]
const g: string = pick("x", 1); // ok — A inferred string
