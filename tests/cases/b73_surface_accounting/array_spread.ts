// Surface-accounting spec (backlog 73). DISABLED until WU2/WU3 land. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Silent skip: `infer_array_literal` reads `element.as_expression()` and `continue`s
// on a spread/elision (src/check/checker/expr.rs:290), so a bad call inside a spread
// operand is never walked. tsc 6.0.3 --strict: TS2345 on the spread `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the spread-element child slot is not visited — currently 0 diagnostics.
const a: number[] = [...[need("bad")]]; // incomplete[expr-infer/array-literal/spread-element]

// CONTROL (supported): a plain array element IS walked, so a bad element reports.
const b: number[] = [need("bad")]; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
