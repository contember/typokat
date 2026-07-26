// backlog 100 — a composed `while` test. `build_flow_while` asks `analyze_guard` for a
// fact exactly the way `build_flow_if` does, so the body edge and the exit edge are
// broken by the same missing `LogicalExpression` arm. `while` is the one loop form the
// flow pre-pass models (see `unmodeled_loop_flow_deferred.ts` for the rest).
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

function bodyNarrowsThroughAnd(nn: number | null, flag: boolean) {
  while (nn !== null && flag) {
    const a: number = nn;
  }
}

function bodyNarrowsWithGuardOnTheRight(nn: number | null, flag: boolean) {
  while (flag && nn !== null) {
    const a: number = nn;
  }
}

function bodyNarrowsThroughNegatedOr(nn: number | null, flag: boolean) {
  while (!(nn === null || flag)) {
    const a: number = nn;
  }
}

function bodyNarrowsTwoSymbols(su: string | undefined, nn: number | null) {
  while (su !== undefined && nn !== null) {
    const a: string = su;
    const b: number = nn;
  }
}

// An `||` test determines nothing inside the body.
function orBodyStaysWide(nn: number | null, flag: boolean) {
  while (nn === null || flag) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// The exit edge of an `||` test: the loop ended because the whole disjunction failed,
// so `nn === null` failed too.
function exitEdgeOfOr(nn: number | null, flag: boolean) {
  while (nn === null || flag) {
  }
  const a: number = nn;
}

// The exit edge of an `&&` test determines nothing — either conjunct may have failed.
function exitEdgeOfAnd(nn: number | null, flag: boolean) {
  while (nn !== null && flag) {
  }
  const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}

// Controls: the single-guard `while` already works in both directions (M23) and must
// keep working.
function singleGuardControl(nn: number | null) {
  while (nn !== null) {
    const a: number = nn;
  }
}

function singleGuardExitControl(nn: number | null) {
  while (nn !== null) {
  }
  const a: null = nn;
  const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}
