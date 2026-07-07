// M31 — relation directions and reductions.
// Source `A & B & C`: a target is satisfied when the merge covers it (one member, or
// several members together). Target `A & B & C`: a value must satisfy every member.
// Disjoint primitives and conflicting object members describe an uninhabited value.

type Three = { a: number } & { b: string } & { c: boolean };
function pickOne(x: Three): { b: string } { return x; }
function spanTwo(x: Three): { a: number; c: boolean } { return x; }
function toEvery(x: { a: number; c: boolean }): Three { return x; }   // error[TK2741]: Property 'b' is missing

// disjoint primitive members — no value inhabits `string & number` (message code-only:
// typokat rejects per-member where tsc reduces to `never`; same verdict)
type SN = string & number;
const sn1: SN = "s";                               // error[TK2322]
const sn2: SN = 1;                                 // error[TK2322]

// conflicting object members — `x` is `number & string`, uninhabited (message code-only)
type Conflict = { x: number } & { x: string };
const conflict: Conflict = { x: 1 };               // error[TK2322]
