// Surface-accounting spec (backlog 73). Annotation lowering models the `symbol` and `bigint`
// keyword types as intrinsics while retaining incomplete records for the unmodeled literal and
// compiler-intrinsic variants below. See tests/cases/README.md ("Surface-accounting corpus").
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 for both keyword
// assignments with the exact intrinsic names below.

let s: symbol = 0; // error[TK2322]: Type 'number' is not assignable to type 'symbol'

let b: bigint = 0; // error[TK2322]: Type 'number' is not assignable to type 'bigint'

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
