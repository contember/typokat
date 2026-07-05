// M27 — a template type over an UNRESOLVED type parameter stays deferred
// (symbolic) and relates conservatively (the M25/M26 model): identical node
// assignable, a deferred template flows into `string` (every instantiation
// is a string), nothing else in. Instantiation substitutes and evaluates.
// tsc 6.0.3 --strict cross-checked.

type Tag<T extends string> = `tag:${T}`;

function ret1<T extends string>(t: T): Tag<T> {
  return t; // error[TK2322]
}

function ret2<T extends string>(t: Tag<T>): Tag<T> {
  return t;
}

function ret3<T extends string>(t: Tag<T>): string {
  return t;
}

// Instantiation collapses (literal) or keeps a pattern (string).
const c1: Tag<"x"> = "tag:x";
const c2: Tag<"x"> = "tag:y"; // error[TK2322]: Type '"tag:y"' is not assignable to type '"tag:x"'
declare const someStr: string;
declare function mk<T extends string>(t: T): Tag<T>;
const c3: `tag:${string}` = mk(someStr);
const c3b: "tag:x" = mk(someStr); // error[TK2322]
const c4: "tag:x" = mk("x");
const c5: "tag:x" = mk("y"); // error[TK2322]
