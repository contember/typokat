// M21 — optional vs required in type-to-type assignability (both directions).
//
// An optional member's effective type is `T | undefined`, and it may be absent. So a
// REQUIRED source satisfies an OPTIONAL target, but an OPTIONAL source does NOT satisfy
// a REQUIRED target (the member might be absent / undefined). All targets are object
// types -> code-only TK2322.

let req: { a: number } = { a: 1 };
let opt: { a?: number } = {};
let both: { a?: number } = {};

opt = req;  // ok — a required `a` satisfies an optional `a`
both = opt; // ok — both optional
opt = both; // ok
req = opt;  // error[TK2322] — an optional `a` may be absent in a required target

// Presence is independent of the value type: an optional source does NOT satisfy a
// required target even when the target's own property type already admits `undefined`
// (the source may OMIT the property entirely, which a required target forbids).
type OptB = { b?: string };
type ReqMaybe = { b: string | undefined };
let reqM: ReqMaybe = { b: undefined };
let optB: OptB = {};
reqM = optB; // error[TK2322] — `b` may be absent in the source
optB = reqM; // ok — a present (if undefined) `b` satisfies the optional target
