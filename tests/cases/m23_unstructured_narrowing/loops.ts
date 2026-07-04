// M23 — loops: the `while` condition narrows the body; assignments flow around
// the back edge (a naive "declared type + condition" model is unsound); the exit
// edge narrows the code after the loop; a `break` edge un-narrows it.
// tsc 6.0.3 --strict cross-checked.

function maybe(): string | null { return null; }

function whileCond(x: string | null) {
  while (x !== null) {
    const ok: string = x;
    x = maybe();
  }
}

function loopBackEdge(cond: boolean) {
  let x: string | null = "a";
  while (cond) {
    const bad: string = x; // error[TK2322]: not assignable to type 'string'
    x = null;
  }
}

function afterLoop(x: string | null) {
  while (x !== null) {
    x = maybe();
  }
  const isNull: null = x;
}

function breakTrap(x: string | null) {
  while (x !== null) {
    break;
  }
  const bad: null = x; // error[TK2322]
}

function continueRechecks(x: string | null) {
  while (x !== null) {
    continue;
  }
  const isNull: null = x;
}
