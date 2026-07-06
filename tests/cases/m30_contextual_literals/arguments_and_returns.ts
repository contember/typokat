// M30 — fresh literals are contextually typed in argument and declared-return
// positions, not only declaration initializers.

type Shape = { kind: "circle"; radius: number };

declare function takesShape(shape: Shape): void;

takesShape({ kind: "circle", radius: 1 });
takesShape({ kind: "square", radius: 1 }); // error[TK2345]

function makeShape(): Shape {
  return { kind: "circle", radius: 1 };
}

function badShape(): Shape {
  return { kind: "square", radius: 1 }; // error[TK2322]
}

const makeArrow = (): Shape => ({ kind: "circle", radius: 1 });
const badArrow = (): Shape => ({ kind: "square", radius: 1 }); // error[TK2322]
