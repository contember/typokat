// M8 — discriminated-union narrowing via a literal discriminant property
// (`x.kind === "lit"`). The then-branch keeps members whose discriminant matches;
// the else-branch keeps the complement.

type Shape = { kind: "circle"; radius: number } | { kind: "square"; side: number };

function area(s: Shape) {
  const wide = s.radius; // error[TK2339]: Property 'radius' does not exist
  if (s.kind === "circle") {
    const r: number = s.radius; // ok — narrowed to the circle member
  } else {
    const d: number = s.side; // ok — complement narrows to the square member
  }
}

// `!==` flips the branches.
function area2(s: Shape) {
  if (s.kind !== "circle") {
    const d: number = s.side; // ok — then-branch is the square member
  }
}
