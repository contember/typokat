// M26 — mapped types over concrete sources: `{ [K in keyof T]: … }` iterates
// the resolved keys of a concrete T and builds an object type; a literal-union
// key source (`K in "x" | "y"`) builds one property per member. tsc 6.0.3
// --strict cross-checked.

type P = { a: number; b: string };

// Identity map.
type Ident<T> = { [K in keyof T]: T[K] };
const i1: Ident<P> = { a: 1, b: "s" };
const i2: Ident<P> = { a: 1, b: 2 }; // error[TK2322]: Type 'number' is not assignable to type 'string'
const i3: Ident<P> = { a: 1 }; // error[TK2741]: 'b' is missing

// Value transformation keeps the key set.
type Nullify<T> = { [K in keyof T]: T[K] | null };
const n1: Nullify<P> = { a: null, b: "s" };
const n2: Nullify<P> = { a: true, b: "s" }; // error[TK2322]

// Constant value type.
type AllNum<T> = { [K in keyof T]: number };
const a1: AllNum<P> = { a: 1, b: 2 };
const a2: AllNum<P> = { a: 1, b: "s" }; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Literal-union key source (non-homomorphic).
type FromUnion = { [K in "x" | "y"]: number };
const u1: FromUnion = { x: 1, y: 2 };
const u2: FromUnion = { x: 1 }; // error[TK2741]: 'y' is missing
const u3: FromUnion = { x: 1, y: "s" }; // error[TK2322]

// Mapped result feeds ordinary relations (member access, argument checks).
declare const ip: Ident<P>;
const r1: number = ip.a;
const r2: string = ip.a; // error[TK2322]: Type 'number' is not assignable to type 'string'
