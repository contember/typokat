// Backlog 104 — the must-stay-silent set. Freshness is a property of the object
// LITERAL, not of the position: an arm that is a variable reference carries no
// freshness and an ordinary width-subtyping assignment stays legal. Getting this
// boundary wrong turns the descent into a false-positive generator on ordinary
// code, which is strictly worse than the silence it replaces.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2872 x1 (the `&&`-left row
// below — an unimplemented family, so this file must check clean here).

interface Shape {
  kind: string;
}

interface Wide {
  kind: string;
  extra: number;
}

interface Optional {
  kind: string;
  note?: string;
}

interface Bag {
  [key: string]: unknown;
}

declare const flag: boolean;
declare const maybeShape: Shape | undefined;
declare const shapeValue: Shape;

// NOT fresh: a variable reference arm. `loose` is a `{ kind: string; extra: number }`
// value, which width-subtypes to `Shape`.
const loose = { kind: "circle", extra: 1 };
const varArm: Shape = flag ? loose : { kind: "square" };
const varAlternate: Shape = flag ? { kind: "square" } : loose;
const varOperand: Shape = maybeShape ?? loose;
const bothVars: Shape = flag ? loose : shapeValue;
const nestedVar: Shape = flag ? (maybeShape ?? loose) : shapeValue;

// The extra property IS in the target, so nothing is excess.
const admitted: Wide = flag ? { kind: "circle", extra: 1 } : { kind: "square", extra: 2 };

// No annotation, so no contextual target and no checked position at all.
const unannotated = flag ? { kind: "circle", extra: 1 } : { kind: "square" };
const unannotatedLogical = maybeShape ?? { kind: "circle", extra: 1 };
const unannotatedNested = flag ? (maybeShape ?? { kind: "circle", extra: 1 }) : { kind: "square" };

// `&&`'s LEFT operand is a condition, not a shaped value (tsc's
// `getContextualTypeForBinaryOperand` gives it no contextual type), so its literal
// is never excess-checked. tsc's only complaint here is TS2872 ("always truthy"),
// a family typokat does not implement.
const andLeft: Shape = { kind: "circle", extra: 1 } && shapeValue;

// An optional target member supplied by an arm is a known property.
const optionalMember: Optional = flag ? { kind: "circle", note: "n" } : { kind: "square" };

// An index-signature target accepts any key, in an arm as in the direct position.
const indexed: Bag = flag ? { kind: "circle", extra: 1 } : { kind: "square" };

// Empty-object and `unknown` targets impose no key set.
const emptyTarget: {} = flag ? { kind: "circle", extra: 1 } : { kind: "square" };
const unknownTarget: unknown = flag ? { kind: "circle", extra: 1 } : { kind: "square" };

// Clean arms and operands.
const cleanTernary: Shape = flag ? { kind: "circle" } : { kind: "square" };
const cleanCoalesce: Shape = maybeShape ?? { kind: "square" };
const cleanOr: Shape = maybeShape || { kind: "square" };
const cleanAnd: Shape | undefined = maybeShape && { kind: "square" };
const cleanArray: Shape[] = flag ? [{ kind: "circle" }] : [];
