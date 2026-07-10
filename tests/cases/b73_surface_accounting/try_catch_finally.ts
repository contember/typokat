// Surface-accounting spec (backlog 73). DISABLED until WU2 and statement-container
// wiring (WU4) land. See tests/cases/README.md ("Surface-accounting corpus").
//
// Silent skip: `check_stmt` drops `TryStatement` (src/check/checker/statements.rs:145
// `_ => {}`); the binder (src/binder/bind.rs:364) and flow pre-pass
// (src/check/checker/flowgraph/mod.rs:107) drop it too. The try/catch/finally blocks
// are never checked. tsc 6.0.3 --strict: TS2322 on each of the three assignments.

// INCOMPLETE: none of the three blocks are visited — currently 0 diagnostics.
try {
  const a: number = "bad1"; // incomplete[stmt-check/try-statement/block]
} catch (e) {
  const b: number = "bad2"; // incomplete[stmt-check/try-statement/handler]
} finally {
  const c: number = "bad3"; // incomplete[stmt-check/try-statement/finalizer]
}

// CONTROL (supported): a bad assignment in an ordinary block IS checked.
{
  const d: number = "bad4"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}
