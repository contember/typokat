// M5 — type aliases live in the type space and resolve transparently.

type Num = number;
type Pair = { a: number; b: string };

const n: Num = 1;     // ok
const nBad: Num = "x"; // error[TK2322]: Type 'string' is not assignable

const pr: Pair = { a: 1, b: "y" }; // ok
const prBad: Pair = { a: 1 };      // error[TK2741]: Property 'b' is missing in type
