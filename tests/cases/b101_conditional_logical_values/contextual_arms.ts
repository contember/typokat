// Backlog 101 — the assignment target contextually types the operands that carry
// the value, through the ordinary contextual-initializer path: both ternary arms,
// both `||`/`??` operands, and `&&`'s right operand (tsc's
// `getContextualTypeForBinaryOperand`). Without it a fresh object arm widens its
// literal property to `string` and every clean row below would false-error.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x2.

interface Shape {
  kind: "circle" | "square";
}

declare const flag: boolean;
declare const maybeShape: Shape | undefined;
declare const shapeValue: Shape;

// Both arms shaped by the annotation: `kind` keeps its literal type.
const fromTernary: Shape = flag ? { kind: "circle" } : { kind: "square" };

// A wrong literal in one arm is still reported.
const wrongArm: Shape = flag ? { kind: "circle" } : { kind: "triangle" }; // error[TK2322]

// `??` shapes its right operand; `||` shapes both.
const fromCoalesce: Shape = maybeShape ?? { kind: "square" };
const fromOr: Shape = maybeShape || { kind: "square" };

// `&&`'s right operand is shaped; its left is a condition, not a shaped value. The
// target has to admit the left's falsy part too, and `T | undefined` is the one
// such union shape typokat can already shape a literal against.
const fromAnd: Shape | undefined = maybeShape && { kind: "circle" };

// A ternary nested inside a `??` keeps the context all the way down.
const nestedContext: Shape = (flag ? { kind: "circle" } : undefined) ?? { kind: "square" };

// Array and tuple contexts reach the arms the same way.
const numbers: number[] = flag ? [1, 2] : [];
const pair: [number, string] = flag ? [1, "a"] : [2, "b"];
const wrongPair: [number, string] = flag ? [1, "a"] : ["b", 2]; // error[TK2322]

// A non-fresh operand is unaffected by the context.
const passthrough: Shape = flag ? shapeValue : { kind: "square" };

// The same context reaches a call argument (through the contextual re-walk gate),
// an array element, and a nested object property.
declare function wantsShape(value: Shape): void;
declare function wantsShapes(values: Shape[]): void;
wantsShape(flag ? { kind: "circle" } : { kind: "square" });
wantsShape(maybeShape ?? { kind: "square" });
wantsShapes([flag ? { kind: "circle" } : { kind: "square" }]);
const nestedProperty: { inner: Shape } = { inner: flag ? { kind: "circle" } : { kind: "square" } };
