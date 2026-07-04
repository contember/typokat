// M24 — the constraint is the type parameter's APPARENT type: member access on a
// `T extends {…}` resolves through it, and `T` is assignable TO its constraint.
// The reverse direction (constraint → T) stays an error (tsc TS2322 with the
// "could be instantiated with a different subtype" elaboration).

interface HasX { x: number; }

function readX<T extends HasX>(t: T): number {
  return t.x;
}

function readBad<T extends HasX>(t: T): string {
  return t.y; // error[TK2339]: Property 'y' does not exist on type 'T'
}

function widen<T extends HasX>(t: T): HasX {
  return t;
}

function narrow<T extends HasX>(t: T): T {
  return { x: 1 }; // error[TK2322]
}

function inline<T extends { s: string }>(t: T): string {
  const v: string = t.s;
  return v;
}

// The apparent type governs EVERY structural consumer, not just member READS
// (review findings F1–F3): writes, element access, and calls all resolve
// through the constraint.

// Member WRITE through the constraint.
function writeX<T extends HasX>(t: T): void {
  t.x = 1;
  t.x = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'
}

// Element/computed READ through the constraint (literal key and array index).
function elemKey<T extends HasX>(t: T): string {
  return t["x"]; // error[TK2322]: Type 'number' is not assignable to type 'string'
}
function elemIndex<T extends number[]>(t: T): string {
  return t[0]; // error[TK2322]: Type 'number' is not assignable to type 'string'
}

// CALLING a value whose type is a constrained parameter.
function callIt<T extends (a: number) => number>(t: T): string {
  t("s"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
  return t(1); // error[TK2322]: Type 'number' is not assignable to type 'string'
}
