// backlog 100 — an `||` condition narrows its **else** branch (`!(a || b)` is
// `!a && !b`, so every operand is known to have failed there) and must narrow NOTHING
// in the then-branch (any single operand may be what made the condition true).
// The unmarked `else` lines are the HEAD false positives; the marked then-branch lines
// are the dropped-error mirror — a fix that narrows the wrong branch deletes them.
// `tsc 6.0.3 --strict --noEmit` reports exactly the marked lines.

type Tagged = { kind: "a"; a: number } | { kind: "b"; b: string };
type Keyed = { p: number } | { r: string };

function nullGuardOnTheLeft(nn: number | null, flag: boolean) {
  if (nn === null || flag) {
    const a: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
    const b: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const c: number = nn;
    const d: null = nn; // error[TK2322]: Type 'number' is not assignable to type 'null'
  }
}

function nullGuardOnTheRight(nn: number | null, flag: boolean) {
  if (flag || nn === null) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: number = nn;
  }
}

// The inverted disjunct: the `else` knows `nn === null` held, so it narrows TO `null`.
function invertedGuard(nn: number | null, flag: boolean) {
  if (nn !== null || flag) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: null = nn;
    const c: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  }
}

function twoGuardsDifferentSymbols(su: string | undefined, nn: number | null) {
  if (nn === null || su === undefined) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
    const b: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  } else {
    const c: number = nn;
    const d: string = su;
  }
}

function threeDisjuncts(su: string | undefined, nn: number | null, flag: boolean) {
  if (nn === null || su === undefined || flag) {
    const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
  } else {
    const b: number = nn;
    const c: string = su;
  }
}

function typeofGuard(sn: string | number, flag: boolean) {
  if (typeof sn !== "string" || flag) {
    const a: string = sn; // error[TK2322]: Type 'number' is not assignable to type 'string'
  } else {
    const b: string = sn;
  }
}

function truthinessGuard(su: string | undefined, flag: boolean) {
  if (!su || flag) {
    const a: string = su; // error[TK2322]: Type 'undefined' is not assignable to type 'string'
  } else {
    const b: string = su;
  }
}

function discriminantGuard(o: Tagged, flag: boolean) {
  if (o.kind !== "a" || flag) {
    const a: number = o.a; // error[TK2339]
  } else {
    const b: number = o.a;
  }
}

function inGuard(q: Keyed, flag: boolean) {
  if (!("p" in q) || flag) {
    const a: number = q.p; // error[TK2339]
  } else {
    const b: number = q.p;
  }
}

// After the `if`, the join of both branches is the declared type again.
function afterTheIf(nn: number | null, flag: boolean) {
  if (nn === null || flag) {
  } else {
    const inside: number = nn;
  }
  const a: number = nn; // error[TK2322]: Type 'null' is not assignable to type 'number'
}
