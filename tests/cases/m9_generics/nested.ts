// M9 — generic types nested in generic functions; substitution flows through.

interface Box<T> { value: T; }

function unwrap<T>(b: Box<T>): T { return b.value; }

const n: number = unwrap<number>({ value: 1 });   // ok
const m: number = unwrap<string>({ value: "s" }); // error[TK2322]
// ^ unwrap<string> returns string
const bad = unwrap<number>({ value: "s" });       // error[TK2345]
// ^ argument { value: "s" } is not a Box<number>

// nested instantiation: Box<Box<number>>
const nn: Box<Box<number>> = { value: { value: 1 } };   // ok
const mm: Box<Box<number>> = { value: { value: "s" } }; // error[TK2322]
