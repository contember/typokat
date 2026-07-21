// Surface-accounting spec (backlog 73). ENABLED by WU5: annotation lowering records the
// incomplete surface for unmodeled keyword/literal `TSType` variants before degrading to
// the error type. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_annotation_inner` / `lower_literal_type` had no arm for these
// variants, so each lowered to `None` → the error type silently. WU5 records the surface
// first. Their remaining semantic model is owned by backlog 75.

// INCOMPLETE: the `symbol` keyword type is not modeled — the initializer degrades silently.
let s: symbol = 0; // incomplete[annotation-lower/symbol-keyword/self]

// INCOMPLETE: the `bigint` keyword type is not modeled.
let b: bigint = 0; // incomplete[annotation-lower/bigint-keyword/self]

let o: object = {};
o = { value: 1 };
o = [1, 2, 3];
o = () => 1;
o = 1; // error[TK2322]: Type 'number' is not assignable to type 'object'
o = "text"; // error[TK2322]: Type 'string' is not assignable to type 'object'
o = true; // error[TK2322]: Type 'boolean' is not assignable to type 'object'
o = null; // error[TK2322]: Type 'null' is not assignable to type 'object'
o = undefined; // error[TK2322]: Type 'undefined' is not assignable to type 'object'

// `{}` is deliberately wider than the `object` keyword: it accepts every
// represented non-nullish value, including primitives.
let nonNullish: {} = 1;
nonNullish = "text";
nonNullish = true;
nonNullish = { value: 1 };
nonNullish = [1, 2, 3];
nonNullish = () => 1;
nonNullish = null; // error[TK2322]: Type 'null' is not assignable to type '{}'
nonNullish = undefined; // error[TK2322]: Type 'undefined' is not assignable to type '{}'

// INCOMPLETE: a `bigint` literal type aborts lowering.
type Big = 1n; // incomplete[annotation-lower/literal-type/bigint]

// INCOMPLETE: the `intrinsic` keyword type is not modeled.
type Intr = intrinsic; // incomplete[annotation-lower/intrinsic-keyword/self]
