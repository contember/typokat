// Backlog 104 — the excess-property check descends into BOTH ternary arms, each
// against the same contextual target the arm is typed by. Backlog 101 gave the
// arms real value types; the excess walk is the one consumer that did not come
// with it, so every row below is silent at that HEAD.
//
// Framing: tsc nests its excess elaboration under one TS2322 for the whole
// assignment (the ternary's value is the arm union, so the union fails first);
// typokat emits the freestanding TK2353 at the offending key. Same verdict,
// different framing, so markers are code-only per tests/cases/README.md. Where
// two keys are excess, typokat reports one per key / per arm while tsc stops at
// the first — ledgered as `objects/excess-per-arm-count`.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x8.

interface Shape {
  kind: string;
}

interface Inner {
  v: number;
}

interface Outer {
  inner: Inner;
}

declare const flag: boolean;

// Excess in the consequent only.
const consequentOnly: Shape = flag ? { kind: "circle", extra: 1 } : { kind: "square" }; // error[TK2353]

// Excess in the alternate only.
const alternateOnly: Shape = flag ? { kind: "circle" } : { kind: "square", extra: 1 }; // error[TK2353]

// Both arms — one marker per arm (tsc stops at the first).
const bothArms: Shape = flag ? { kind: "circle", extra: 1 } : { kind: "square", other: 2 }; // error[TK2353] | error[TK2353]

// Two excess keys in ONE arm — one marker per key (the same pre-existing count
// rule a directly assigned literal already follows).
const twoKeysOneArm: Shape = flag ? { kind: "circle", extra: 1, other: 2 } : { kind: "square" }; // error[TK2353] | error[TK2353]

// Parentheses around an arm are transparent, exactly as in the direct position.
const parenthesizedArm: Shape = flag ? ({ kind: "circle", extra: 1 }) : ({ kind: "square" }); // error[TK2353]

// A declared (interface) child target nested inside an arm is still resolved, so
// the nested literal is checked against `Inner`, not skipped.
const declaredChild: Outer = flag ? { inner: { v: 1, extra: 1 } } : { inner: { v: 2 } }; // error[TK2353]

// Array and tuple arms carry the element target down the same way.
const arrayArm: Shape[] = flag ? [{ kind: "circle", extra: 1 }] : []; // error[TK2353]
const tupleArm: [Shape] = flag ? [{ kind: "circle", extra: 1 }] : [{ kind: "square" }]; // error[TK2353]
