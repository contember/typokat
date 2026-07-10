// WU1 / finding 1 — assignment expressions are only checked when they form a
// top-level ExpressionStatement (statements.rs:66-74); `infer_expr` has no
// AssignmentExpression arm, so an assignment nested inside a ternary, an array
// literal, an `if` condition, or another assignment is silently unchecked →
// dropped TK2322. This corpus is DISABLED at HEAD; enabling it exposes the
// missing errors on the nested-assignment lines. Cross-checked vs tsc 6.0.3
// --strict (each nested assignment: one TS2322).

let a: number = 0;
let s: string = "";

// Assignment in a ternary arm — tsc: TS2322 on `(a = "bad")`.
const t = a > 0 ? (a = "bad") : 0; // error[TK2322]: not assignable to type 'number'

// Assignment inside an array literal — tsc: TS2322 on `a = "z"`.
const arr = [a = "z"]; // error[TK2322]: not assignable to type 'number'

// Inner assignment of an assignment (outer target accepts, only inner errors) —
// tsc: TS2322 on the inner `a = "wrong"`.
s = (a = "wrong"); // error[TK2322]: not assignable to type 'number'

// Assignment used as an `if` condition — tsc: TS2322 on `a = "cond"`.
if (a = "cond") {} // error[TK2322]: not assignable to type 'number'

// --- controls: the already-supported top-level assignment path still fires,
// and a well-typed nested assignment stays clean. ---
a = "top"; // error[TK2322]: not assignable to type 'number'
const good = (a = 5);
