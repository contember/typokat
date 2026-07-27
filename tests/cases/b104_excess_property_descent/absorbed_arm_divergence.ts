// Backlog 104 — DOCUMENTED DIVERGENCE (over-report). tsc's excess check is driven
// by the RELATION on the ternary's value, and that value is a *subtype-reduced*
// union: an arm is dropped when it is a subtype of a sibling arm, and the excess
// check participates in that subtype test. So a fresh literal whose excess key is
// admitted by a sibling arm's type is absorbed before anything is related, and
// tsc never sees a fresh literal at all.
//
// typokat's excess check is a separate syntax-directed walk with no operand types,
// so it checks the literal regardless. Same direction as every other divergence in
// this file — it over-reports on a literal that already names a key its annotation
// does not declare — but it is a false positive against tsc, not a gap.
//
// Closing it means teaching `Interner::union` tsc's subtype reduction (and its
// freshness-aware subtype relation), which is a change to the VALUE model backlog
// `101` shipped, not to the freshness rule. Not attempted here.
//
// Ledgered in docs/reference/divergences.md as `objects/excess-absorbed-arm`.
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x2 (the two control
// rows; the three divergent rows are clean there).

interface Shape {
  kind: string;
}

interface Wide {
  kind: string;
  extra: number;
}

interface Wider {
  kind: string;
  extra: number;
  more?: string;
}

declare const flag: boolean;
declare const wide: Wide;
declare const wider: Wider;
declare const shapeValue: Shape;
declare const maybeWide: Wide | undefined;
declare const maybeShape: Shape | undefined;

// The sibling arm's type is exactly the literal's widened type, so tsc reduces the
// union to `Wide` and reports nothing.
const absorbedIdentical: Shape = flag ? { kind: "circle", extra: 1 } : wide; // error[TK2353]

// The sibling arm merely admits the excess key, which is enough to absorb.
const absorbedSupertype: Shape = flag ? { kind: "circle", extra: 1 } : wider; // error[TK2353]

// Same rule through a logical operand.
const absorbedOperand: Shape = maybeWide ?? { kind: "circle", extra: 1 }; // error[TK2353]

// --- controls: a sibling arm that does NOT admit the excess key absorbs nothing,
// so tsc reports too and the rows agree. ---
const notAbsorbed: Shape = flag ? { kind: "circle", extra: 1 } : shapeValue; // error[TK2353]
const notAbsorbedOperand: Shape = maybeShape ?? { kind: "circle", extra: 1 }; // error[TK2353]
