// M4 — property access on a union: the property must exist on EVERY member; its type is
// the union of the per-member property types.

let shape: { kind: number; a: number } | { kind: number; b: string } = { kind: 1, a: 2 };

const k: number = shape.kind; // ok — 'kind' is on both members

const miss = shape.a; // error[TK2339]: Property 'a' does not exist on type
