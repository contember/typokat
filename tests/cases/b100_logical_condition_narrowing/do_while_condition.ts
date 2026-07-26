// backlog 100 — `do…while`. The body runs once BEFORE the test, so its entry state is
// the join of "not yet tested" and "test held": a composed test may never narrow the
// body, and `!(a && b)` never determines the exit either. typokat agrees with tsc on
// every line here today (it models no `do…while` flow at all, so it narrows nothing);
// the file is the regression net that keeps a composed-condition fix from leaking a
// first-iteration narrow into the body.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines — the whole file.

function composedTestDoesNotNarrowTheBody(nn: number | null, flag: boolean) {
  do {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } while (nn !== null && flag);
}

function orTestDoesNotNarrowTheBody(nn: number | null, flag: boolean) {
  do {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } while (nn === null || flag);
}

// The single-guard control: even an unambiguous test cannot narrow the first iteration.
function singleGuardDoesNotNarrowTheBody(nn: number | null) {
  do {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } while (nn !== null);
}

function nonGuardTestControl(nn: number | null, flag: boolean) {
  do {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } while (flag);
}

// After the loop `!(a && b)` holds, which determines neither conjunct.
function exitEdgeOfComposedTest(nn: number | null, flag: boolean) {
  do {
  } while (nn !== null && flag);
  const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}
