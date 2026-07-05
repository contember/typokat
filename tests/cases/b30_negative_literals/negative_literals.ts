// b30 — negative numeric literal types are literal types, not `any`: the
// unary-minus type expression lowers to a proper negative literal, so it
// relates like any literal (M27 review FN-1: it lowered to the error type
// and accepted everything, both directions). tsc 6.0.3 --strict
// cross-checked.

const a: -1 = -1;
const b: -1 = 5; // error[TK2322]: Type '5' is not assignable to type '-1'
const c: -1 = 0; // error[TK2322]: Type '0' is not assignable to type '-1'

type NU = -1 | -2;
const d: NU = -3; // error[TK2322]
const e: NU = -2;

// A negative literal is not assignable to unrelated types…
declare const neg: -1;
const f: { a: 1 } = neg; // error[TK2322]
// …but flows into number.
const g: number = neg;

// Template holes stringify negatives (the M27 blast radius of the old bug).
type V = `v${-1}`;
const h: V = "v-1";
const i: V = "vX"; // error[TK2322]

// Conditional extends sees the literal.
type IsNeg<T> = T extends -1 ? "yes" : "no";
const j: IsNeg<-1> = "yes";
const k: IsNeg<-1> = "no"; // error[TK2322]: Type '"no"' is not assignable to type '"yes"'
const l: IsNeg<1> = "no";
