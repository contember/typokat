// Backlog 104 — the descent is recursive, so a fresh literal keeps its freshness
// through any number of ternary / logical layers as long as every layer passes the
// same contextual target down.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x6.

interface Shape {
  kind: string;
}

declare const flag: boolean;
declare const other: boolean;
declare const maybeShape: Shape | undefined;

// An arm that is itself a ternary — in the consequent…
const nestedConsequent: Shape = flag ? (other ? { kind: "circle", extra: 1 } : { kind: "square" }) : { kind: "wedge" }; // error[TK2353]

// …and in the alternate (the chained `a ? x : b ? y : z` shape).
const nestedAlternate: Shape = flag ? { kind: "wedge" } : other ? { kind: "circle" } : { kind: "square", extra: 1 }; // error[TK2353]

// `a && b || c` — `&&`'s right operand feeds `||`'s left, and both are shaped.
const andThenOr: Shape = (maybeShape && { kind: "circle", extra: 1 }) || { kind: "square" }; // error[TK2353]

// A ternary inside a logical operand.
const ternaryInLogical: Shape = maybeShape ?? (flag ? { kind: "circle", extra: 1 } : { kind: "square" }); // error[TK2353]

// A logical inside a ternary arm.
const logicalInTernary: Shape = flag ? (maybeShape ?? { kind: "circle", extra: 1 }) : { kind: "square" }; // error[TK2353]

// Three layers deep, through a parenthesized chain.
const threeLayers: Shape = flag ? (maybeShape ?? (other ? { kind: "circle", extra: 1 } : { kind: "square" })) : { kind: "wedge" }; // error[TK2353]
