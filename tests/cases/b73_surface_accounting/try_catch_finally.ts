// Surface-accounting spec (backlog 73). ENABLED by WU4: `check_stmt` now walks the
// try / catch / finally blocks through the existing block walker, so a bad assignment
// in any of the three blocks reports normally. See tests/cases/README.md
// ("Surface-accounting corpus").
//
// Traversed (option a): the try block, catch handler body, and finalizer are ordinary
// statement lists, checked in their own block scopes. The catch parameter's type is
// still unmodeled (tsc types it `unknown`), so the catch clause records the incomplete
// catch-param surface. Flow narrowing does not cross into try blocks (conservative;
// recorded as flow/try-statement/self). tsc 6.0.3 --strict: TS2322 on each assignment.

try {
  const a: number = "bad1"; // error[TK2322]: Type 'string' is not assignable to type 'number'
} catch (e) { // incomplete[stmt-check/try-statement/catch-param]
  const b: number = "bad2"; // error[TK2322]: Type 'string' is not assignable to type 'number'
} finally {
  const c: number = "bad3"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

// CONTROL (supported): a bad assignment in an ordinary block IS checked.
{
  const d: number = "bad4"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}
