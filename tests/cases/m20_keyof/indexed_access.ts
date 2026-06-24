// M20 — indexed-access types: `T[K]` is the type of T's property/properties named
// by the key type K (a literal, a union of literals, or keyof T). Evaluated eagerly.

interface Rec {
  a: number;
  b: string;
}

const r1: Rec["a"] = 1;     // ok — number
const r2: Rec["b"] = "s";   // ok — string
const rBad: Rec["a"] = "s"; // error[TK2322]
// ^ Rec["a"] is number

const u: Rec["a" | "b"] = 1;       // ok — number | string
const u2: Rec["a" | "b"] = "s";    // ok
const uBad: Rec["a" | "b"] = true; // error[TK2322]
// ^ boolean is not in number | string

interface Point {
  x: number;
  y: number;
}

const all: Point[keyof Point] = 1;      // ok — number (both props number)
const allBad: Point[keyof Point] = "s"; // error[TK2322]
