// M30 — contextual typing preserves literal members of fresh object literals
// when a concrete target type is known.

type Shape = { kind: "circle"; radius: number };

const shapeOk: Shape = { kind: "circle", radius: 1 };
const shapeBad: Shape = { kind: "square", radius: 1 }; // error[TK2322]

type Choice = { flag: "on" | "off"; count: 1 | 2 };

const choiceOk: Choice = { flag: "on", count: 1 };
const choiceBadFlag: Choice = { flag: "maybe", count: 1 }; // error[TK2322]
const choiceBadCount: Choice = { flag: "off", count: 3 }; // error[TK2322]

type Nested = { outer: { tag: "a"; value: 1 } };

const nestedOk: Nested = { outer: { tag: "a", value: 1 } };
const nestedBad: Nested = { outer: { tag: "a", value: 2 } }; // error[TK2322]

// No target context: object-literal members still widen, even under `const`.
const inferredObject = { kind: "circle" };
const inferredObjectKind: "circle" = inferredObject.kind; // error[TK2322]
