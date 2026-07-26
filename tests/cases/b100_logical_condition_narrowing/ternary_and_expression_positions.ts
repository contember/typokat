// backlog 100 — a composed condition outside an `if`: the ternary test and ordinary
// expression positions.
// NOTE on what is (not) asserted here: typokat types both a `LogicalExpression` and a
// `ConditionalExpression` as the ERROR type on purpose (`infer_logical` /
// `infer_expr`'s ternary arm — "coarse, never an assignment source in the subset"), so
// an assertion on the VALUE of `cond ? a : b` or of `a && b` would be vacuous. Every
// case below observes the narrow through a call argument inside the arm / operand
// instead, which is a real relation check.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

declare function takesNumber(n: number): void;
declare function takesString(s: string): void;
declare function takesNumberReturningBool(n: number): boolean;
declare function takesBoolAndNumber(b: boolean, n: number): void;

// The consequent runs under the composed test's true sense.
function ternaryConsequent(nn: number | null, flag: boolean) {
  (nn !== null && flag) ? takesNumber(nn) : 0;
}

// The alternate runs under its false sense, which determines nothing for `&&`.
function ternaryAlternateOfAnd(nn: number | null, flag: boolean) {
  (nn !== null && flag) ? 0 : takesNumber(nn); // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'number'
}

// `||`: the alternate is the determined arm, the consequent is not.
function ternaryAlternateOfOr(nn: number | null, flag: boolean) {
  (nn === null || flag) ? 0 : takesNumber(nn);
}

function ternaryConsequentOfOr(nn: number | null, flag: boolean) {
  (nn === null || flag) ? takesNumber(nn) : 0; // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'number'
}

// `!` over a composition in a ternary test: De Morgan applies here too.
function ternaryNegatedTest(nn: number | null, flag: boolean) {
  !(nn === null || flag) ? takesNumber(nn) : 0;
}

// Two symbols, one composed test.
function ternaryTwoSymbols(su: string | undefined, nn: number | null) {
  (su !== undefined && nn !== null) ? takesString(su) : 0;
  (su !== undefined && nn !== null) ? takesNumber(nn) : 0;
}

// An ordinary expression position: the composed condition is still walked (its right
// operand keeps the shipped M23 narrow) and the narrow must NOT survive the statement.
function expressionPosition(nn: number | null) {
  const ok = nn !== null && takesNumberReturningBool(nn);
  const leak: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}

// The negative control for the same walk: under `nn === null` the right operand sees
// `null`, so the argument check must still fire.
function expressionPositionNegative(nn: number | null) {
  const bad = nn === null && takesNumberReturningBool(nn); // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'number'
}

// A composed condition passed as an argument must not narrow a sibling argument.
function argumentPosition(nn: number | null, flag: boolean) {
  takesBoolAndNumber(nn !== null && flag, nn); // error[TK2345]: Argument of type 'null' is not assignable to parameter of type 'number'
}

// A composed condition in return position: it is an ordinary expression, and it must
// not report anything by itself.
function returnPosition(nn: number | null, flag: boolean): boolean {
  return nn !== null && flag;
}

// The same in return position, with the right operand consuming the left guard's
// narrow (the shipped M23 RHS path) — it must stay clean.
function returnPositionWithGuardedRhs(nn: number | null): boolean {
  return nn !== null && takesNumberReturningBool(nn);
}
