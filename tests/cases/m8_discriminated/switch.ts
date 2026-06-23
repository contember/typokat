// M8 — switch narrowing on a literal discriminant: each `case` narrows the
// discriminant to the matching member.

type Shape = { kind: "circle"; radius: number } | { kind: "square"; side: number };

function area(s: Shape): number {
  switch (s.kind) {
    case "circle": {
      return s.radius; // ok — narrowed to circle
    }
    case "square": {
      return s.side; // ok — narrowed to square
    }
  }
  return 0;
}

function bad(s: Shape) {
  switch (s.kind) {
    case "circle": {
      const w: number = s.side; // error[TK2339]: Property 'side' does not exist
      break;
    }
  }
}
