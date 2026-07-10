// WU1 / finding 3 — `for`, `for-in`, `for-of`, and `do` statements fall into the
// `_ => {}` arm of the statement checker (statements.rs:106), so their entire
// bodies (nested declarations/assignments/calls) are unchecked; `throw` operands
// are used only for reachability and never type-checked. So the annotation
// mismatches in these bodies and the bad `throw` operand are all dropped.
// DISABLED at HEAD; enabling exposes the missing errors. Cross-checked vs
// tsc 6.0.3 --strict (each body: one TS2322; the throw: one TS2345).

declare function takesNumber(x: number): void;

for (let i = 0; i < 3; i++) {
  const bad: number = "x"; // error[TK2322]: not assignable to type 'number'
}

for (const k in { a: 1 }) {
  const bad2: number = "y"; // error[TK2322]: not assignable to type 'number'
}

for (const v of [1, 2]) {
  const bad3: number = "z"; // error[TK2322]: not assignable to type 'number'
}

do {
  const bad4: number = "w"; // error[TK2322]: not assignable to type 'number'
} while (false);

// --- control: a well-typed loop body (using the bound loop variable) is clean;
// this also proves the loop variable is actually bound. ---
for (const v of [1, 2]) {
  const okv: number = v;
}

// throw operand is type-checked — tsc: TS2345 (string not assignable to number).
// Kept last so it does not make the control loop unreachable.
throw takesNumber("oops"); // error[TK2345]: not assignable to parameter of type 'number'
