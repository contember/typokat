// M21 — optional properties on object type literals / aliases (`{ b?: T }`).
//
// Same semantics as interface optionals, but exercising the object-type-annotation
// lowering path (`type O = { … }`, inline `{ … }` parameter/variable annotations),
// which previously dropped any annotation that contained an optional member (the whole
// annotation degraded to the error type).

type O = { a: number; b?: string };

const ok: O = { a: 1 };                     // ok
const withBoth: O = { a: 1, b: "hi" };      // ok
const undefOpt: O = { a: 1, b: undefined }; // ok
const missReq: O = { b: "hi" };             // error[TK2741]: Property 'a' is missing in type
const wrongOpt: O = { a: 1, b: 5 };         // error[TK2322]
const excess: O = { a: 1, c: 0 };           // error[TK2353]: 'c' does not exist in type

// Inline optional members in an annotation position lower correctly.
const empty: { x?: number } = {};                                    // ok — optional may be absent
function exact(p: { x: number; y?: number }): number { return p.x; } // ok

// Reading an inline optional member yields `T | undefined`.
function readOpt(p: { y?: number }): number { return p.y; } // error[TK2322]
