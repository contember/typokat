// b29 — alias cycles: a genuine SURFACE cycle (the alias's own resolution
// re-enters itself, directly or mutually, including through union members)
// reports TK2456 at the declaration and error-types (silent downstream, the
// M22 discipline — which presumes exactly this primary diagnostic). LEGAL
// recursion through members resolves correctly — member reads must not
// silently degrade to the error type. tsc 6.0.3 --strict cross-checked.

type Y = Y | null; // error[TK2456]: circularly references itself
type Z = Z; // error[TK2456]: circularly references itself
type Mut1 = Mut2 | null; // error[TK2456]: circularly references itself
type Mut2 = Mut1; // error[TK2456]: circularly references itself
declare const y1: Y;
declare const z1: Z;

// Legal member recursion: reads see the real type.
type X = { a: X | null };
declare const x: X;
const s: string = x.a; // error[TK2322]
const ok1: X | null = x.a;
const q = x.q; // error[TK2339]: Property 'q' does not exist
declare const xa: X | null;
const w: X = { a: xa };

// Legal mutual recursion through members.
type Even = { next: Odd | null };
type Odd = { next: Even | null };
declare const ev: Even;
const n1: Odd | null = ev.next;
const n2: string = ev.next; // error[TK2322]

// Legal recursion through INDEX-SIGNATURE values and MAPPED value templates
// (the canonical JSON shape) — no TK2456, and the alias resolves to a REAL
// type (the undefined line proves it isn't silently error-typed).
type Json = string | number | boolean | null | Json[] | { [k: string]: Json };
declare const j: Json;
const j1: Json = { a: 1 };
const j2: Json = undefined; // error[TK2322]
type MapRec = string | { [K in "a" | "b"]: MapRec };
declare const mr: MapRec;
