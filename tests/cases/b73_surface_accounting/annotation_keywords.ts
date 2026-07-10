// Surface-accounting spec (backlog 73). ENABLED by WU5: annotation lowering records the
// incomplete surface for unmodeled keyword/literal `TSType` variants before degrading to
// the error type. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_annotation_inner` / `lower_literal_type` had no arm for these
// variants, so each lowered to `None` → the error type silently. WU5 records the surface
// first. These stay owned by backlog 73 (a real type model needs lib.d.ts symbols).

// INCOMPLETE: the `symbol` keyword type is not modeled — the initializer degrades silently.
let s: symbol = 0; // incomplete[annotation-lower/symbol-keyword/self]

// INCOMPLETE: the `bigint` keyword type is not modeled.
let b: bigint = 0; // incomplete[annotation-lower/bigint-keyword/self]

// INCOMPLETE: the `object` keyword type is not modeled.
let o: object = {}; // incomplete[annotation-lower/object-keyword/self]

// INCOMPLETE: a `bigint` literal type aborts lowering.
type Big = 1n; // incomplete[annotation-lower/literal-type/bigint]

// INCOMPLETE: the `intrinsic` keyword type is not modeled.
type Intr = intrinsic; // incomplete[annotation-lower/intrinsic-keyword/self]
