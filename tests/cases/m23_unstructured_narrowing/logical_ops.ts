// M23 — `&&` / `||` / ternary narrow their right-hand side / arms.
// tsc 6.0.3 --strict cross-checked.

function takesString(s: string): void {}

function andRhs(x: string | null) {
  x !== null && takesString(x);
  x === null && takesString(x); // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'string'
}

function orRhs(x: string | null) {
  x === null || takesString(x);
  x !== null || takesString(x); // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'string'
}

function ternaryArms(x: string | null) {
  x !== null ? takesString(x) : undefined;
  x === null ? takesString(x) : undefined; // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'string'
}

function typeofAnd(x: string | number) {
  typeof x === "string" && takesString(x);
}
