// M4 — union assignability.
//   into a union: source must match SOME member.
//   out of a union: union assignable to T only if EVERY member is.
// Union-typed targets render in canonical (not source) order -> code-only markers there.

const toU_ok1: number | string = 1;    // ok
const toU_ok2: number | string = "a";  // ok
const toU_bad: number | string = true; // error[TK2322]

let n: number = 0;
let u: number | string = 1;
n = u; // error[TK2322]: Type 'string' is not assignable to type 'number'
u = n; // ok — number is a member of number | string
