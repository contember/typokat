// M23 — an assignment narrows the variable to the assigned value's type in the
// straight-line flow that follows (until the next assignment / join).

function takesString(s: string): void {}

function assignNarrow() {
  let x: string | number = 5;
  x = "s";
  const ok: string = x;
  x = 7;
  const bad: string = x; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

function assignThenGuard(x: string | null) {
  x = "now a string";
  takesString(x);
}

// Control: a join of two assignment states widens back to the union of both.
function assignJoin(cond: boolean) {
  let x: string | number = 5;
  if (cond) {
    x = "s";
  }
  const bad: string = x; // error[TK2322]
}
