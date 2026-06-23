// M1 — intrinsic types as assignment SOURCES (requires variable references).
// any flows both ways; unknown flows only into unknown/any.

let anyVal: any = 1;
let unknownVal: unknown = 1;

const fromAny: number = anyVal;         // any -> number: ok
const toUnknown: unknown = anyVal;      // anything -> unknown: ok
const fromUnknown: number = unknownVal; // error[TK2322]: Type 'unknown' is not assignable to type 'number'
