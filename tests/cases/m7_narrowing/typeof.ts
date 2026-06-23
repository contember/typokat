// M7 — typeof narrowing of a union in if / else branches.
// Narrowing is observed through EXISTING codes: a reference that is the wide
// union fails assignment (TK2322), but inside a guarded branch it is narrowed
// and the same assignment is clean. Union-typed sources -> code-only markers.

function f(x: string | number) {
  const wide: string = x; // error[TK2322]
  if (typeof x === "string") {
    const s: string = x; // ok — narrowed to string
  } else {
    const n: number = x; // ok — narrowed to number (complement)
  }
  const after: string = x; // error[TK2322]
  // ^ narrowing does not escape the if
}

// typeof === "number" keeps the number member in the then-branch.
function g(y: string | number) {
  if (typeof y === "number") {
    const n: number = y; // ok
  }
}
