// Surface-accounting spec (backlog 73). ENABLED by WU3: the `TemplateLiteral` arm in
// `infer_expr` now records the incomplete interpolation child slot before dropping.
// See tests/cases/README.md ("Surface-accounting corpus") and docs/reference/scope.md.
//
// Skip accounted: `infer_expr`'s `TemplateLiteral` arm records
// `expr-infer/template-literal/interpolation` instead of silently returning `None`, so
// the bad call inside a `${...}` interpolation is no longer a false-clean.
// tsc 6.0.3 --strict: TS2345 on the interpolated `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the interpolation child slot is not visited — currently 0 diagnostics.
const s: string = `x${need("bad")}y`; // incomplete[expr-infer/template-literal/interpolation]

// CONTROL (supported): the same bad call in a checked initializer position reports.
const c: number = need("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
