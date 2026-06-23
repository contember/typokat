// M2 — structural object assignability: width (extra props ok) + depth (prop types checked).

const p1: { a: number; b: string } = { a: 1, b: "x" }; // ok — exact

// width subtyping THROUGH a variable: extra 'b' is fine (no excess-property check on non-fresh)
const wide = { a: 1, b: 2 };
const p3: { a: number } = wide; // ok

// missing required property
const p4: { a: number; b: string } = { a: 1 }; // error[TK2741]: Property 'b' is missing in type

// depth: property present but wrong type (code-only — target is an object type)
const p5: { a: number } = { a: "x" }; // error[TK2322]
