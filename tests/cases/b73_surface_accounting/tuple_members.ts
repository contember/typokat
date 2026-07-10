// Surface-accounting spec (backlog 73). ENABLED by WU5: tuple lowering records the
// incomplete surface for a named or optional tuple member before aborting the whole
// tuple annotation. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_tuple_annotation` returned `None` on the first named/optional
// member, mis-shaping (dropping) the tuple silently. WU5 records the member first.

// INCOMPLETE: a named tuple member aborts lowering at the first named element.
type Named = [first: number, second: string]; // incomplete[annotation-lower/named-tuple-member/self]

// INCOMPLETE: an optional tuple element aborts lowering.
type Opt = [number, string?]; // incomplete[annotation-lower/tuple-optional-element/self]

// CONTROL (supported): a rest tuple element is lowered (M18) — no incomplete, clean.
type Rest = [number, ...string[]];
let r: Rest = [1, "a", "b"];
