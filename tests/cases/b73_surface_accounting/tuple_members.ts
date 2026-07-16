// Surface-accounting spec (backlog 73). Named tuple labels are erased transparently;
// an optional tuple member remains explicitly unavailable. See tests/cases/README.md
// ("Surface-accounting corpus").
//
// Skip accounted: `lower_tuple_annotation` returns `None` on an optional member rather
// than mis-shaping the tuple. Named members lower their inner element normally.

// CONTROL (supported): labels do not affect tuple identity or element types.
type Named = [first: number, second: string];

// INCOMPLETE: an optional tuple element aborts lowering.
type Opt = [number, string?]; // incomplete[annotation-lower/tuple-optional-element/self]

// CONTROL (supported): a rest tuple element is lowered (M18) — no incomplete, clean.
type Rest = [number, ...string[]];
let r: Rest = [1, "a", "b"];
