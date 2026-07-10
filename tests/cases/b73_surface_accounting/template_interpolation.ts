// Surface-accounting spec (backlog 73). DISABLED until the incomplete outcome
// (WU2) and expression child-slot wiring (WU3) land. See tests/cases/README.md
// ("Surface-accounting corpus") and docs/reference/scope.md.
//
// Silent skip: `infer_expr` has no `TemplateLiteral` arm (src/check/checker/expr.rs:162
// `_ => None`), so a bad call inside a `${...}` interpolation is never checked.
// tsc 6.0.3 --strict: TS2345 on the interpolated `need("bad")` call.

function need(n: number): number {
  return n;
}

// INCOMPLETE: the interpolation child slot is not visited — currently 0 diagnostics.
const s: string = `x${need("bad")}y`; // incomplete[expr-infer/template-literal/interpolation]

// CONTROL (supported): the same bad call in a checked initializer position reports.
const c: number = need("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
