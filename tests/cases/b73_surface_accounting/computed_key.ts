// Surface-accounting spec (backlog 73). ENABLED by WU3: the object-literal walkers now
// record the incomplete computed-key child slot before `continue`-ing. See
// tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `infer_object_literal` records `expr-infer/object-literal/computed-key`
// when `prop.key.static_name()` is `None`, so a computed key expression is no longer a
// false-clean. tsc 6.0.3 --strict: TS2345 on the computed-key `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the computed-key child slot is not visited — currently 0 diagnostics.
const o = { [need("bad")]: 1 }; // incomplete[expr-infer/object-literal/computed-key]

// CONTROL (supported): the property VALUE slot is walked, so a bad value reports.
const p: { a: number } = { a: need("bad") }; // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
