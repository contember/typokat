// backlog 100 — an `&&` condition must narrow the branch it guards.
// `analyze_guard` returns `None` for a `LogicalExpression`, and `build_flow_logical`
// joins both senses of the left guard back together before `build_flow_if` asks for a
// fact, so at HEAD the then-branch sees the DECLARED union. The branch type is
// **un-narrowed**, not inverted: `if (nn !== null && flag) { const g: null = nn; }`
// reports `Type 'number' is not assignable to type 'null'` (the offending constituent
// of `number | null`), which is what made this look like an inversion.
// Every case is its own function, so one case's flow can never reach the next.
// `tsc 6.0.3 --strict --noEmit`: this file is CLEAN. Every line below is a HEAD
// false positive.

type Tagged = { kind: "a"; a: number } | { kind: "b"; b: string };
type Keyed = { p: number } | { r: string };
type Box = { size: number };

function guardOnTheLeft(nn: number | null, flag: boolean) {
  if (nn !== null && flag) {
    const a: number = nn;
  }
}

function guardOnTheRight(nn: number | null, flag: boolean) {
  if (flag && nn !== null) {
    const a: number = nn;
  }
}

function twoGuardsSameSymbol(nn: number | null) {
  if (nn !== null && nn > 0) {
    const a: number = nn;
  }
}

function twoGuardsDifferentSymbols(su: string | undefined, nn: number | null) {
  if (su !== undefined && nn !== null) {
    const a: string = su;
    const b: number = nn;
  }
}

function threeConjuncts(su: string | undefined, nn: number | null, flag: boolean) {
  if (su !== undefined && nn !== null && flag) {
    const a: string = su;
    const b: number = nn;
  }
}

function undefinedGuard(su: string | undefined, flag: boolean) {
  if (su !== undefined && flag) {
    const a: string = su;
  }
}

function typeofGuard(sn: string | number, flag: boolean) {
  if (typeof sn === "string" && flag) {
    const a: string = sn;
  }
}

function truthinessGuard(su: string | undefined, flag: boolean) {
  if (su && flag) {
    const a: string = su;
  }
}

function discriminantGuard(o: Tagged, flag: boolean) {
  if (o.kind === "a" && flag) {
    const a: number = o.a;
  }
}

function inGuard(q: Keyed, flag: boolean) {
  if ("p" in q && flag) {
    const a: number = q.p;
  }
}

// Both operands must hold, so the *inverted* guard narrows too: `nn === null` keeps
// `null`, it does not remove it.
function invertedGuardOnTheRight(nn: number | null, flag: boolean) {
  if (flag && nn === null) {
    const a: null = nn;
  }
}

// The right operand already narrows through M23 (`box.size` resolves); the branch it
// guards must keep the same narrow.
function rightOperandDependsOnTheLeft(box: Box | null) {
  if (box !== null && box.size > 0) {
    const a: Box = box;
  }
}

// A guard reached through a nested `if` is the shipped M23 path and stays clean — the
// control that proves the defect is in the composition, not the individual guards.
function nestedIfControl(nn: number | null, flag: boolean) {
  if (nn !== null) {
    if (flag) {
      const a: number = nn;
    }
  }
}
