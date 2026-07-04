// M26 — mapped-type modifiers: `?` / `+?` add optionality, `-?` strips it,
// `readonly` / `-readonly` add/strip readonly; a homomorphic map (over
// `keyof T`) PRESERVES the source's `?` and `readonly` when no modifier says
// otherwise. tsc 6.0.3 --strict cross-checked.

type P = { a: number; b?: string; readonly c: boolean };

// Homomorphic identity preserves ? and readonly.
type Ident<T> = { [K in keyof T]: T[K] };
declare const idp: Ident<P>;
const h1: string | undefined = idp.b;
const h2: Ident<P> = { a: 1, c: true };
function wr1(v: Ident<P>): void {
  v.c = true; // error[TK2540]: read-only
}

// `?` / `+?` add optionality.
type Part<T> = { [K in keyof T]?: T[K] };
const p1: Part<P> = {};
const p2: Part<P> = { a: 1 };
const p3: Part<P> = { a: "s" }; // error[TK2322]

// `-?` strips optionality (Required) — including `undefined` from the VALUE
// type (tsc Required semantics), so writing undefined errors and reads are
// exact.
type Req<T> = { [K in keyof T]-?: T[K] };
const q1: Req<P> = { a: 1, b: "s", c: true };
const q2: Req<P> = { a: 1, c: true }; // error[TK2741]: 'b' is missing
declare const und: undefined;
const q3: Req<P> = { a: 1, b: und, c: true }; // error[TK2322]: 'undefined' is not assignable to type 'string'
declare const rq: Req<P>;
const q4: string = rq.b;

// `readonly` adds it everywhere.
type RO<T> = { readonly [K in keyof T]: T[K] };
declare const rop: RO<P>;
function wr2(): void {
  rop.a = 5; // error[TK2540]: read-only
}

// `-readonly` strips it.
type Mut<T> = { -readonly [K in keyof T]: T[K] };
declare const mu: Mut<P>;
function wr3(): void {
  mu.c = false;
}
