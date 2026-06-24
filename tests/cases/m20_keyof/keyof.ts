// M20 — keyof: `keyof T` is the union of T's property names as string-literal types.
// Evaluated eagerly on concrete object types (generic keyof -> VM phase, deferred).

interface Point {
  x: number;
  y: number;
}

let k: keyof Point = "x"; // ok — keyof Point is "x" | "y"
k = "y";                  // ok
k = "z";                  // error[TK2322]
// ^ "z" is not in "x" | "y"

interface Rec {
  a: number;
  b: string;
}

let rk: keyof Rec = "a"; // ok — "a" | "b"
rk = "b";                // ok
rk = "c";                // error[TK2322]
