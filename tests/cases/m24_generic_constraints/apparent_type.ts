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
