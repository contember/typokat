// backlog 54: labeled statements are transparent for checking, but their
// `break` / `continue` targets still participate in the flow graph.
// Cross-checked with tsc 6.0.3 --strict.

declare const cond: boolean;

checkedBlock: {
  const bad: number = "label body"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

checkedLoop: while (cond) {
  const bad: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
  break checkedLoop;
}

function labeledBreakCarriesState() {
  let x: string | number = "start";
  outerBlock: {
    x = 1;
    break outerBlock;
    x = "unreachable";
  }
  const n: number = x;
}

function labeledLoopNarrowing(x: string | null) {
  loopLabel: while (x !== null) {
    const s: string = x;
    x = null;
    continue loopLabel;
  }
  const done: null = x;
}

function labeledContinueTargetsOuter(y: string | null) {
  if (y === null) return;
  let x: string | null = "start";
  outerLoop: while (x !== null) {
    x = null;
    while (y !== null) {
      y = null;
      continue outerLoop;
    }
    x = "skipped";
  }
  const done: null = x;
}
