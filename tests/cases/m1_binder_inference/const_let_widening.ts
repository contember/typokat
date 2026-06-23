// M1 — `const` keeps the literal type; `let` widens to the base type.
// `let` reassignment is checked against the (widened) declared type.

const c_num = 1;   // type: 1   (literal)
const c_str = "a"; // type: "a"

let l_num = 1;     // type: number (widened)
l_num = 2;         // ok
l_num = "two";     // error[TK2322]: Type 'string' is not assignable to type 'number'

let l_str = "a";   // type: string
l_str = "b";       // ok
l_str = 3;         // error[TK2322]: Type 'number' is not assignable to type 'string'
