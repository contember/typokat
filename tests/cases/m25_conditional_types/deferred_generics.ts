// M25 — deferred conditionals: with an unresolved type-parameter check type the
// conditional does not resolve. Assignability is conservative: a deferred
// conditional is assignable to itself (identical target) and to anything both
// branches are assignable to; nothing else is assignable *into* it. Call-site
// instantiation resolves it. tsc 6.0.3 --strict cross-checked. Conditional-type
// displays are unstable — code-only markers where they appear.

function ret1<T>(t: T): T extends string ? "yes" : "no" {
  return "yes"; // error[TK2322]
}

function ret2<T>(t: T, c: T extends string ? "yes" : "no"): T extends string ? "yes" : "no" {
  return c;
}

function toUnion<T>(t: T, c: T extends string ? "yes" : "no"): "yes" | "no" {
  return c;
}

function toSuper<T>(t: T, c: T extends string ? "yes" : "no"): string {
  return c;
}

function toWrong<T>(t: T, c: T extends string ? "yes" : "no"): "yes" {
  return c; // error[TK2322]
}

// Call sites resolve the conditional with the instantiated check type.
declare function m<T>(t: T): T extends string ? "yes" : "no";
const m1: "yes" = m("abc");
const m2: "no" = m(42);
const m3: "no" = m("abc"); // error[TK2322]: Type '"yes"' is not assignable to type '"no"'
const m4: "yes" = m<string>("abc");
const m5: "yes" = m<number>(42); // error[TK2322]: Type '"no"' is not assignable to type '"yes"'
