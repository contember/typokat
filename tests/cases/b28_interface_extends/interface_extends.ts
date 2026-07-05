// b28 — interface `extends` composition: inherited members are part of the
// interface's type (assignability, member access, keyof, mapped sources).
// Own members override by name. Before the fix, inherited members were
// invisible: valid literals reported excess properties (false positives)
// while missing inherited members passed silently (false negatives).
// tsc 6.0.3 --strict cross-checked.

interface Base { x: number }
interface Derived extends Base { y: string }

const ok: Derived = { x: 1, y: "s" };
const bad: Derived = { y: "s" }; // error[TK2741]: 'x' is missing

declare const dv: Derived;
const r1: number = dv.x;
const r2: string = dv.x; // error[TK2322]: Type 'number' is not assignable to type 'string'

// keyof sees inherited keys.
const k1: keyof Derived = "x";
const k2: keyof Derived = "q"; // error[TK2322]

// Multiple bases.
interface B2 { z: boolean }
interface Multi extends Base, B2 { w: string }
const m1: Multi = { x: 1, z: true, w: "a" };
const m2: Multi = { x: 1, w: "a" }; // error[TK2741]: 'z' is missing

// Deep chains compose transitively.
interface Deep extends Derived { d: number }
const d1: Deep = { x: 1, y: "s", d: 2 };
const d2: Deep = { y: "s", d: 2 }; // error[TK2741]: 'x' is missing

// An own member overrides the inherited one by name (checked via READS —
// object-literal assignment against literal-typed members is the documented
// contextual-typing deferral, backlog 31: '{ x: 5 }' widens to number).
interface Over extends Base { x: 5 }
declare const ov: Over;
const o1: 5 = ov.x;
const o2: 6 = ov.x; // error[TK2322]: Type '5' is not assignable to type '6'

// Mapped types over a derived interface see the full key set (M26 interplay).
type Ident<T> = { [K in keyof T]: T[K] };
const md1: Ident<Derived> = { x: 1, y: "s" };
const md2: Ident<Derived> = { y: "s" }; // error[TK2741]: 'x' is missing

// A base that is a (non-generic) object-literal type ALIAS composes too.
type ObjAlias = { p: number };
interface IA extends ObjAlias { q: string }
const ia1: IA = { p: 1, q: "s" };
const ia2: IA = { q: "s" }; // error[TK2741]: 'p' is missing
declare const iav: IA;
const iar: number = iav.p;

// A base that is a CLASS composes (tsc allows extending classes).
class CBase { cx: number = 1 }
interface IC extends CBase { q: string }
const ic1: IC = { cx: 1, q: "s" };
const ic2: IC = { q: "s" }; // error[TK2741]: 'cx' is missing
