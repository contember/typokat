// M24 — circular constraints: a constraint chain that revisits its own type
// parameter (following only bare type-parameter constraints) is TK2313 at the
// constraint annotation, and the parameter records NO constraint — so the
// degenerate cycle cannot make `T` assignable to everything through the
// relation engine's assume-true stack (that was a dropped-error bug).
// Structural self-reference (`<T extends { self: T }>`) is NOT circular.
// tsc 6.0.3 --strict cross-checked.

function f<T extends T>(t: T): number { // error[TK2313]: Type parameter 'T' has a circular constraint
  return t; // error[TK2322]: Type 'T' is not assignable to type 'number'
}

function g<T extends U, U extends T>(t: T): number { // error[TK2313]: circular constraint | error[TK2313]: circular constraint
  return t; // error[TK2322]
}

// Structural indirection breaks the chain: legal, terminates via the cycle
// stack, and the apparent type works.
function h<T extends { self: T }>(t: T): { self: T } {
  return t;
}

// COMPOSITE circularity: the chain walk follows union MEMBERS (a bare-param
// member continues the chain), so these are TK2313 too — the union-source
// assume-true rule must not turn them into assignable-to-anything.
// (Intersection composites, 'T extends T & X', are tsc-TS2313 as well, but
// intersection types are not in the type model yet — backlog 25.)
function ua<T extends T | number>(t: T): number { // error[TK2313]: Type parameter 'T' has a circular constraint
  return t; // error[TK2322]
}
function ud<T extends U | number, U extends T>(t: T): number { // error[TK2313]: circular constraint | error[TK2313]: circular constraint
  return t; // error[TK2322]
}

// A composite chain that terminates is legal — no TK2313; the return check
// still fails through the constraint (typokat names 'T', tsc renders the
// resolved constraint 'string | number' — message-only divergence, code-only
// marker).
function uc<T extends U | number, U extends string>(t: T): number {
  return t; // error[TK2322]
}
