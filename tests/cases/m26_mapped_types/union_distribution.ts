// M26 — a HOMOMORPHIC mapped type distributes over a union type argument:
// Ident<A | B> = Ident<A> | Ident<B> (tsc behavior; review finding F1 — the
// permissive `{}` result silently accepted everything). A DIRECT map over a
// union type (`keyof (A | B)` = never) is genuinely `{}` — the two must not
// be conflated. tsc 6.0.3 --strict cross-checked.

type Ident<T> = { [K in keyof T]: T[K] };
type Part<T> = { [K in keyof T]?: T[K] };
type A = { a: number };
type B = { b: string };

const d1: Ident<A | B> = { a: 1 };
const d2: Ident<A | B> = { b: "s" };
const d3: Ident<A | B> = {}; // error[TK2322]
declare const num: number;
const d4: Ident<A | B> = num; // error[TK2322]

// Distribution composes with modifiers.
const d5: Part<A | B> = {};

// Through generic-call instantiation.
declare function mk<T>(t: T): Ident<T>;
declare const ab: A | B;
const d6: Ident<A | B> = mk(ab);

// Direct union source: keyof (A | B) has no common key -> {}.
type Direct = { [K in keyof (A | B)]: number };
const d7: Direct = {};
