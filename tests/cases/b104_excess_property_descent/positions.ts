// Backlog 104 — every position that already runs the excess-property check must
// run it through a ternary / logical too: the declaration initializer, a
// reassignment, an array element, an object property, a call argument, and a
// `return`. Each is a separate `check_excess_properties_for_target` call site, so
// a descent added at only one of them would leave the rest silent.
//
// tsc 6.0.3 --strict --target es2022 --lib es2022: TS2322 x7, TS2345 x2,
// TS2353 x3.

interface Shape {
  kind: string;
}

declare const flag: boolean;
declare const maybeShape: Shape | undefined;
declare function wantsShape(value: Shape): void;
declare function wantsShapes(values: Shape[]): void;
declare function isBig(value: Shape): boolean;

// Array element and tuple element.
const inArray: Shape[] = [flag ? { kind: "circle", extra: 1 } : { kind: "square" }]; // error[TK2353]
const inTuple: [Shape, Shape] = [{ kind: "circle" }, maybeShape ?? { kind: "square", extra: 1 }]; // error[TK2353]

// Object property, and a property one level further down.
const inProperty: { inner: Shape } = { inner: flag ? { kind: "circle", extra: 1 } : { kind: "square" } }; // error[TK2353]
const inNestedProperty: { outer: { inner: Shape } } = { outer: { inner: maybeShape ?? { kind: "circle", extra: 1 } } }; // error[TK2353]

// Call arguments (tsc frames these as TS2345).
wantsShape(flag ? { kind: "circle", extra: 1 } : { kind: "square" }); // error[TK2353]
wantsShape(maybeShape ?? { kind: "circle", extra: 1 }); // error[TK2353]
wantsShapes([flag ? { kind: "circle", extra: 1 } : { kind: "square" }]); // error[TK2353]

// `return` and a concise arrow body.
function returnsShape(): Shape {
  return flag ? { kind: "circle", extra: 1 } : { kind: "square" }; // error[TK2353]
}
const arrowShape = (): Shape => (flag ? { kind: "circle", extra: 1 } : { kind: "square" }); // error[TK2353]

// Reassignment.
let mutable: Shape = { kind: "circle" };
mutable = flag ? { kind: "circle", extra: 1 } : { kind: "square" }; // error[TK2353]

// The ternary TEST is not a shaped value position — the literal there is checked
// against its OWN target (the parameter), never against the outer annotation.
const testPosition: Shape = isBig({ kind: "circle", extra: 1 }) ? { kind: "square" } : { kind: "wedge" }; // error[TK2353]
