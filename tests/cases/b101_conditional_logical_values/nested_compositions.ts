// Backlog 101 — the value types compose: a ternary arm may be another ternary or a
// logical expression, and a logical operand may be a ternary. The union flattens
// through every level rather than collapsing to the error type at the first one.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x6.

declare const flag: boolean;
declare const other: boolean;
declare const n: number;
declare const s: string;
declare const nn: number | null;
declare const nu: number | undefined;

// `a ? b : (c ? d : e)`.
const nestedAlternate: string = flag ? n : other ? n : n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const nestedMixed: boolean = flag ? n : other ? n : s; // error[TK2322]

// `a && b || c` parses as `(a && b) || c`.
const andOr: string = flag && other || n; // error[TK2322]

// Chained `??` is right-associative: `a ?? (b ?? c)`.
const chainedCoalesce: string = nn ?? nu ?? n; // error[TK2322]: Type 'number' is not assignable to type 'string'

// A logical inside a ternary arm and a ternary inside a logical operand.
const logicalInArm: string = flag ? nn ?? n : n; // error[TK2322]: Type 'number' is not assignable to type 'string'
const ternaryInCoalesce: string = (flag ? nn : null) ?? n; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Clean controls for the same shapes at a target that admits the whole union.
const nestedClean: number = flag ? n : other ? n : n;
const andOrClean: boolean | number = flag && other || n;
const chainedClean: number = nn ?? nu ?? n;
const ternaryInCoalesceClean: number = (flag ? nn : null) ?? n;
