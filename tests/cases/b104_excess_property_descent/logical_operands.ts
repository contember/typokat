// Backlog 104 — the same descent through `&&` / `||` / `??`, reaching exactly the
// operands tsc's `getContextualTypeForBinaryOperand` shapes: BOTH operands of
// `||` and `??`, and only the RIGHT operand of `&&` (its left is a condition, not
// a shaped value — the negative control lives in clean_controls.ts).
//
// Two of tsc's diagnostics here belong to families typokat does not implement and
// carry no marker: TS2872 ("This kind of expression is always truthy") on an
// object literal in `||`'s left, and TS2869 ("Right operand of ?? is unreachable")
// on one in `??`'s left. The excess verdict on those same operands is tsc's own
// standalone TS2353, which typokat matches.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x6, TS2353 x3,
// TS2872 x2, TS2869 x1.

interface Shape {
  kind: string;
}

declare const flag: boolean;
declare const maybeShape: Shape | undefined;
declare const shapeValue: Shape;

// `&&` — the right operand is shaped, so its literal is fresh against `Shape`.
// The target admits the left's falsy part (`undefined`), so the row's only
// diagnostic is the excess one.
const andRight: Shape | undefined = maybeShape && { kind: "circle", extra: 1 }; // error[TK2353]

// `||` — the right operand.
const orRight: Shape = maybeShape || { kind: "circle", extra: 1 }; // error[TK2353]

// `||` — the left operand. tsc additionally reports TS2872 here.
const orLeft: Shape = { kind: "circle", extra: 1 } || shapeValue; // error[TK2353]

// `??` — the right operand.
const coalesceRight: Shape = maybeShape ?? { kind: "circle", extra: 1 }; // error[TK2353]

// `??` — the left operand. tsc additionally reports TS2869 here.
const coalesceLeft: Shape = { kind: "circle", extra: 1 } ?? shapeValue; // error[TK2353]

// Both `||` operands are fresh literals. A literal left is always truthy, so tsc
// short-circuits the right operand away and never relates it — one TS2353, on the
// left. The excess walk is syntax-directed and reaches both, so typokat reports
// two. Dead code, safe direction, ledgered as `objects/excess-dead-logical-operand`.
const orBoth: Shape = { kind: "circle", extra: 1 } || { kind: "square", other: 2 }; // error[TK2353] | error[TK2353]

// Parentheses around an operand are transparent.
const parenthesizedOperand: Shape = maybeShape ?? ({ kind: "circle", extra: 1 }); // error[TK2353]

// An array/tuple operand carries the element target down the same way.
declare const maybeShapes: Shape[] | undefined;
const arrayOperand: Shape[] = maybeShapes ?? [{ kind: "circle", extra: 1 }]; // error[TK2353]
const tupleOperand: [Shape] | undefined = maybeShape && [{ kind: "circle", extra: 1 }]; // error[TK2353]

// A logical whose operands are not literals stays untouched by the descent.
const plainLogical: Shape = maybeShape ?? shapeValue;
const plainAnd: Shape | undefined = flag ? maybeShape : shapeValue;
