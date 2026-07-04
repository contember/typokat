// M23 — narrowing through unstructured flow: an early `return`/`throw` in an
// else-less `if` narrows the code AFTER the guard. tsc 6.0.3 --strict cross-checked.

function earlyReturnEq(x: string | null) {
  if (x === null) return;
  const ok: string = x;
}

function earlyReturnNeg(x: string | null) {
  if (x !== null) return;
  const bad: string = x; // error[TK2322]: Type 'null' is not assignable to type 'string'
}

function earlyThrow(x: number | undefined) {
  if (x === undefined) throw "missing";
  const ok: number = x;
}

function earlyReturnTypeof(x: string | number) {
  if (typeof x === "string") return;
  const ok: number = x;
}

function truthyEarly(x: string | undefined) {
  if (!x) return;
  const ok: string = x;
}

function elseIfChain(x: string | number | null) {
  if (x === null) return;
  if (typeof x === "string") return;
  const ok: number = x;
}

// Control: WITHOUT an early exit the branches re-join and nothing is narrowed.
function noEarlyExit(x: string | null) {
  if (x === null) {}
  const bad: string = x; // error[TK2322]
}
