// M23 (review round 1): a destructuring ASSIGNMENT must reset the bound variable
// to its declared type — the flow may neither keep a stale narrow (dropped error)
// nor pretend the assignment didn't happen. tsc narrows precisely to the assigned
// element type; typokat resets to the declared type (same error, wider source —
// safe direction). tsc 6.0.3 cross-checked.

function viaArrayPattern() {
  let x: string | number = 5;
  x = 3;
  [x] = ["s"];
  const bad: number = x; // error[TK2322]
}

function viaObjectPattern() {
  let x: string | number = 5;
  x = 3;
  ({ x } = { x: "s" });
  const bad: number = x; // error[TK2322]
}

function afterGuard(x: string | number) {
  if (typeof x === "string") return;
  [x] = ["s"];
  const bad: number = x; // error[TK2322]
}
