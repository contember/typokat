// M21 — optional properties on interfaces (`b?: T`).
//
// An optional property may be absent in a value; reading it yields `T | undefined`;
// `keyof` and indexed-access include it; and the excess-property check still fires on
// truly-unknown keys. Modeled with `exactOptionalPropertyTypes` OFF (the default): an
// explicit `undefined` is assignable to an optional property.

interface I {
  a: number;
  b?: string;
}

// Presence: a missing optional is fine; a missing required property is TK2741.
const ok: I = { a: 1 };                     // ok
const withBoth: I = { a: 1, b: "hi" };      // ok
const undefOpt: I = { a: 1, b: undefined }; // ok — undefined assignable to an optional
const missReq: I = { b: "hi" };             // error[TK2741]: Property 'a' is missing in type

// A present optional with the wrong type still errors (object target -> code-only).
const wrongOpt: I = { a: 1, b: 5 };         // error[TK2322]

// An unknown property is still excess, even though `b` is optional.
const excess: I = { a: 1, c: true };        // error[TK2353]: 'c' does not exist in type

// Reading an optional property yields `T | undefined` (union source -> code-only);
// reading a required property is exact; assigning the read into a `T | undefined`
// context is fine.
function readOpt(i: I): string { return i.b; }              // error[TK2322]
function readReq(i: I): number { return i.a; }              // ok
function readWide(i: I): string | undefined { return i.b; } // ok

// `keyof` includes the optional key.
let k: keyof I = "a"; // ok — keyof I is "a" | "b"
k = "b";              // ok
k = "c";              // error[TK2322]

// Indexed access of an optional property yields `T | undefined`; of a required one
// it is exact.
const idxOpt: I["b"] = undefined; // ok — I["b"] is string | undefined
const idxOptStr: I["b"] = "hi";   // ok
const idxReq: I["a"] = undefined; // error[TK2322]: Type 'undefined' is not assignable to type 'number'
