// backlog 100 — the complementary branch of an `&&` condition. `!(a && b)` is
// `!a || !b`: neither operand is known to have failed, so NOTHING may be narrowed in
// the `else`. These lines are the dropped-error mirror of `and_then_branch.ts` — a fix
// that pushes the then-branch fact into the wrong branch deletes every diagnostic here.
// HEAD already reports all of them (it narrows nothing anywhere), so this file is the
// regression net rather than a red-to-green target.
// `tsc 6.0.3 --strict --noEmit` reports the same verdict on every marked line.

type Tagged = { kind: "a"; a: number } | { kind: "b"; b: string };
type Keyed = { p: number } | { r: string };

function nullGuardOnTheLeft(nn: number | null, flag: boolean) {
  if (nn !== null && flag) {
  } else {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const b: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  }
}

function nullGuardOnTheRight(nn: number | null, flag: boolean) {
  if (flag && nn !== null) {
  } else {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const b: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  }
}

function undefinedGuard(su: string | undefined, flag: boolean) {
  if (su !== undefined && flag) {
  } else {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

function typeofGuard(sn: string | number, flag: boolean) {
  if (typeof sn === "string" && flag) {
  } else {
    const a: string = sn; // error[TK2322]: Type 'number' is not assignable to type 'string'
    const b: number = sn; // error[TK2322]: Type 'string' is not assignable to type 'number'
  }
}

function truthinessGuard(su: string | undefined, flag: boolean) {
  if (su && flag) {
  } else {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  }
}

function discriminantGuard(o: Tagged, flag: boolean) {
  if (o.kind === "a" && flag) {
  } else {
    const a: number = o.a; // error[TK2339]
  }
}

function inGuard(q: Keyed, flag: boolean) {
  if ("p" in q && flag) {
  } else {
    const a: number = q.p; // error[TK2339]
  }
}

// The inverted guard's complement is equally undetermined: `nn` may still be either.
function invertedGuardOnTheRight(nn: number | null, flag: boolean) {
  if (flag && nn === null) {
  } else {
    const a: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// Two guards on two symbols: the `else` may not narrow EITHER of them.
function twoGuardsDifferentSymbols(su: string | undefined, nn: number | null) {
  if (su !== undefined && nn !== null) {
  } else {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function threeConjuncts(su: string | undefined, nn: number | null, flag: boolean) {
  if (su !== undefined && nn !== null && flag) {
  } else {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

// After the `if`, the join of both branches is the declared type again.
function afterTheIf(nn: number | null, flag: boolean) {
  if (nn !== null && flag) {
    const inside: number = nn;
  }
  const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}
