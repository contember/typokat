// backlog 100 — `!` over a composed condition. De Morgan must come out right in BOTH
// branches: `!(a || b)` narrows its then-branch (both operands failed) and nothing in
// its else; `!(a && b)` narrows nothing in its then-branch and narrows its else.
// `analyze_guard` already flips polarity for a leading `!`, so the whole defect is the
// missing `LogicalExpression` arm underneath it.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

function notOverOr(nn: number | null, flag: boolean) {
  if (!(nn === null || flag)) {
    const a: number = nn;
    const b: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  } else {
    const c: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const d: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  }
}

function notOverAnd(nn: number | null, flag: boolean) {
  if (!(nn !== null && flag)) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const b: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  } else {
    const c: number = nn;
    const d: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  }
}

function doubleNegation(nn: number | null, flag: boolean) {
  if (!!(nn !== null && flag)) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function notOverOrTypeof(sn: string | number, flag: boolean) {
  if (!(typeof sn !== "string" || flag)) {
    const a: string = sn;
  } else {
    const b: string = sn; // error[TK2322]: Type 'number' is not assignable to type 'string'
  }
}

function notOverOrTruthiness(su: string | undefined, flag: boolean) {
  if (!(!su || flag)) {
    const a: string = su;
  } else {
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

// `!` on a single operand of the composition, not on the composition itself.
function notOnAnOperand(nn: number | null, flag: boolean) {
  if (!flag && nn !== null) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function trailingNotOperand(nn: number | null, flag: boolean) {
  if (nn !== null && !flag) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// `!` over a three-way disjunction: every operand failed in the then-branch.
function notOverThreeDisjuncts(nn: number | null, flag: boolean, other: boolean) {
  if (!(nn === null || flag || other)) {
    const a: number = nn;
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// `!` over a nested composition: `!((a && b) || c)` leaves `a` undetermined, so
// NEITHER branch may narrow.
function notOverNestedComposition(
  nn: number | null,
  flag: boolean,
  other: boolean,
) {
  if (!((nn !== null && flag) || other)) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}
