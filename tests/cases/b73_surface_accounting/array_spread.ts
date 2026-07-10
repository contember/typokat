// Surface-accounting spec (backlog 73). ENABLED by WU3: the array-literal walkers now
// record the incomplete spread-element child slot before `continue`-ing. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: the array element helper records `expr-infer/array-literal/spread-element`
// on a spread/elision, so a bad call inside a spread operand is no longer a false-clean.
// tsc 6.0.3 --strict: TS2345 on the spread `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the spread-element child slot is not visited — currently 0 diagnostics.
const a: number[] = [...[need("bad")]]; // incomplete[expr-infer/array-literal/spread-element]

// CONTROL (supported): a plain array element IS walked, so a bad element reports.
const b: number[] = [need("bad")]; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
