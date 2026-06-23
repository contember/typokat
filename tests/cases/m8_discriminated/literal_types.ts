// M8 — literal type annotations (TSLiteralType) and literal assignability.
// A literal type accepts only its own literal; a union of literals accepts each.
// Literal-typed targets render with quotes -> code-only markers.

const a: "hello" = "hello"; // ok
const b: "hello" = "world"; // error[TK2322]
const n: 42 = 42;           // ok
const m: 42 = 43;           // error[TK2322]

let u: "a" | "b" = "a"; // ok — union of literal types
u = "b";                // ok
u = "c";                // error[TK2322]
