// M26 — mapped types evaluate at every demand site: alias instantiation,
// nesting (mapped of mapped), generic-call return instantiation, and the
// empty-object edge. tsc 6.0.3 --strict cross-checked.

type Ident<T> = { [K in keyof T]: T[K] };
type RO<T> = { readonly [K in keyof T]: T[K] };

// Mapped of mapped composes (modifiers accumulate).
type RORO = RO<Ident<{ x: number }>>;
declare const roro: RORO;
const c1: number = roro.x;
function wr(): void {
  roro.x = 1; // error[TK2540]: read-only
}

// Generic-call return instantiation evaluates the mapped result.
declare function mk<T>(t: T): Ident<T>;
const g1: { a: number } = mk({ a: 1 });
const g2: { a: string } = mk({ a: 1 }); // error[TK2322]

// Empty object maps to an empty object.
type E = Ident<{}>;
declare const ev: E;
const e1: {} = ev;

// Mapped over an interface source.
interface Q { m: number; n: string; }
const q1: Ident<Q> = { m: 1, n: "s" };
const q2: Ident<Q> = { m: 1, n: 2 }; // error[TK2322]: Type 'number' is not assignable to type 'string'

// A directly self-referential mapped alias cannot resolve (tsc also adds a
// TS2313 for K's circular constraint — secondary code omitted, documented).
type M = { [K in keyof M]: number }; // error[TK2456]: circularly references itself
declare const sm: M;

// An index-signature source is out of scope (no `K in string` production):
// the mapped type stays DEFERRED and relates conservatively — tsc resolves
// it homomorphically (documented over-report divergence). It must never
// evaluate to a permissive `{}`.
type IS = { [k: string]: number };
declare const isv: IS;
const s1: Ident<IS> = isv; // error[TK2322]
