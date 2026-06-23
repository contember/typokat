// M2 — excess-property checks fire ONLY on fresh object literals assigned to a typed target.

const direct: { a: number } = { a: 1, x: 0 }; // error[TK2353]: 'x' does not exist in type

// not fresh -> no excess check, plain width subtyping applies
const viaVar = { a: 1, x: 0 };
const indirect: { a: number } = viaVar; // ok

// freshness recurses into nested literals
const nested: { a: { b: number } } = { a: { b: 1, c: 2 } }; // error[TK2353]: 'c' does not exist in type
