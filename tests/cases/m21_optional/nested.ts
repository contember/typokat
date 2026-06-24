// M21 — an optional member whose type is itself an object: freshness (the excess
// check) must still recurse THROUGH the optional container, and absence stays fine.
//
// The optional member's effective type is `{ … } | undefined`; the excess check has
// to descend into the object part of that union (the M19 "freshness recurses through
// a new container" rule) rather than bail on the union.

interface Outer {
  p?: { q: number };
}

const good: Outer = { p: { q: 1 } };      // ok
const empty: Outer = {};                  // ok — `p` is optional
const bad: Outer = { p: { q: 1, r: 2 } }; // error[TK2353]: 'r' does not exist in type
const wrong: Outer = { p: { q: "x" } };   // error[TK2322]
