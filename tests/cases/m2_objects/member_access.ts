// M2 — member access. Unknown members are TK2339; the result type feeds assignability.

const obj = { a: 1, b: "x" }; // type: { a: number; b: string }

const ma: number = obj.a; // ok
const mb: string = obj.b; // ok

const mc = obj.c; // error[TK2339]: Property 'c' does not exist on type

// access resolves to number, assigned to string -> mismatch (code-only via member result)
const mismatch: string = obj.a; // error[TK2322]: Type 'number' is not assignable to type 'string'
