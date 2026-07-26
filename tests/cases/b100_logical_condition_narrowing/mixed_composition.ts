// backlog 100 — nested and mixed `&&`/`||` compositions. A fact may only reach a
// branch when EVERY path into that branch establishes it, so these cases separate a
// real composed narrow from an over-eager one. Parenthesized and precedence-implied
// forms must behave identically.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

type Box = { size: number };

// `(a && b) || c` — `c` alone can make the condition true, so the then-branch knows
// nothing; the else knows only that the disjunction failed, which does not determine
// which conjunct did.
function andInsideOr(nn: number | null, flag: boolean, other: boolean) {
  if ((nn !== null && flag) || other) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// The same shape without parentheses — `&&` binds tighter than `||`.
function andInsideOrByPrecedence(
  nn: number | null,
  flag: boolean,
  other: boolean,
) {
  if (nn !== null && flag || other) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// `a && (b || c)` — `a` held on every path into the then-branch.
function orInsideAnd(nn: number | null, flag: boolean, other: boolean) {
  if (nn !== null && (flag || other)) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// `(a || b) && c` — the guard is the right conjunct of an `&&` whose left is composed.
function orInsideAndOnTheRight(nn: number | null, flag: boolean, other: boolean) {
  if ((flag || other) && nn !== null) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// A redundantly parenthesized guard is transparent.
function parenthesizedGuard(nn: number | null, flag: boolean) {
  if ((nn !== null) && flag) {
    const a: number = nn;
  }
}

// A composition nested in the LEFT operand of another `&&`: both guards hold.
function nestedAndChain(su: string | undefined, nn: number | null, flag: boolean) {
  if ((nn !== null && flag) && su !== undefined) {
    const a: number = nn;
    const b: string = su;
  }
}

// `a || (b && c)` — the else knows the whole disjunction failed, so `a` failed.
function andInsideOrElseNarrows(
  nn: number | null,
  flag: boolean,
  other: boolean,
) {
  if (nn === null || (flag && other)) {
  } else {
    const a: number = nn;
  }
}

// One composed operand narrows, the other does not: the `su` guard is a plain conjunct
// while `nn` is only guarded on one side of an inner `||`.
function partiallyDetermined(su: string | undefined, nn: number | null, flag: boolean) {
  if (su !== undefined && (nn !== null || flag)) {
    const a: string = su;
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// The mirror: `nn` is undetermined in the then-branch of a composed `||` even when the
// other disjunct is a real guard on a different symbol.
function orMixesSymbols(su: string | undefined, nn: number | null, flag: boolean) {
  if ((nn === null && flag) || su === undefined) {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const c: string = su;
    const d: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// A guard in the right operand that only type-checks because the left held (shipped
// M23 RHS narrowing) composed with a third conjunct.
function rightOperandDependsOnTheLeft(box: Box | null, flag: boolean) {
  if (box !== null && box.size > 0 && flag) {
    const a: Box = box;
  }
}
