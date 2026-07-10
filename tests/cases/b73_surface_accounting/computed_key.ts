// Surface-accounting spec (backlog 73). DISABLED until WU2/WU3 land. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Silent skip: `infer_object_literal` reads `prop.key.static_name()` and `continue`s
// when it is `None` (src/check/checker/expr.rs:265), so a computed key expression is
// never walked. tsc 6.0.3 --strict: TS2345 on the computed-key `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the computed-key child slot is not visited — currently 0 diagnostics.
const o = { [need("bad")]: 1 }; // incomplete[expr-infer/object-literal/computed-key]

// CONTROL (supported): the property VALUE slot is walked, so a bad value reports.
const p: { a: number } = { a: need("bad") }; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
