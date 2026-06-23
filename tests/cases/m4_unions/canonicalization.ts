// M4 — union canonicalization (§3.3): order-independent, deduped, never absorbed.
// All clean here; the interner UNIT test asserts identical TypeIds for these. The last
// case observes the collapse through an assignability error.

let ab: number | string = 1;
let ba: string | number = "x";
ab = ba; // ok — same canonical type as ab
ba = ab; // ok

const dup: number | number = 1;      // ok — collapses to number
const withNever: number | never = 1; // ok — never absorbed -> number

let onlyNum: number | never = 0;
onlyNum = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'
