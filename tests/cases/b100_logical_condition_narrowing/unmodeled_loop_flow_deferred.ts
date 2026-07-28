// backlog 100, deferred tail — condition positions the flow pre-pass does not model at
// all. `build_flow_stmt` (crates/typokat-check/src/check/checker/flowgraph/mod.rs) falls through to `_ => {}`
// for `for` / `for-in` / `for-of` / `do…while`, so a reference inside a `for` body — or
// after a `do…while` — never reaches `reference_flow` and reads its declared type. That
// is a wider gap than backlog 100: it is not about composition (the single-guard cases
// here fail identically), and it is owned by backlog `51`, not by the composed-condition
// fix.
//
// The first three cases are therefore pinned as **over-reports**, not as the tsc verdict:
// their markers record what typokat produces, and the `tsc` oracle below is the record of
// what it should eventually be. Ledger entry: `narrowing/unmodeled-loop-condition-flow`
// in docs/reference/divergences.md, owner backlog `51`.
//
// `tsc 6.0.3 --strict --noEmit` oracle for this file: CLEAN on `forComposedTest`,
// `forSingleGuard`, and `doWhileExitEdge`; the four remaining lines below are reported by
// both checkers.

// The composed condition, i.e. the backlog-100 shape in a `for` test.
function forComposedTest(nn: number | null, flag: boolean) {
  for (; nn !== null && flag; ) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// The SINGLE-guard control fails the same way — proof the `for` gap is independent of
// the composition defect.
function forSingleGuard(nn: number | null) {
  for (; nn !== null; ) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// The exit edge of a plain `do…while` narrows in tsc (the loop ended because the test
// failed); typokat models no `do…while` edge, so it reads the declared type.
function doWhileExitEdge(nn: number | null) {
  do {
  } while (nn !== null);
  const a: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
}

// Controls that already agree: an `||` test determines nothing in the body, and a
// non-guard test narrows nothing anywhere.
function forOrTestControl(nn: number | null, flag: boolean) {
  for (; nn === null || flag; ) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function forNonGuardTestControl(nn: number | null, flag: boolean) {
  for (; flag; ) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function forExitEdgeOfAndControl(nn: number | null, flag: boolean) {
  for (; nn !== null && flag; ) {
  }
  const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}
