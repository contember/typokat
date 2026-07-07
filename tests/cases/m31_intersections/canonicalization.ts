// M31 — intersection canonicalization: `X & unknown` collapses to `X`, `X & never`
// collapses to `never`, and nested/duplicate members flatten and dedup.

type S = string & unknown;                         // collapses to `string`
const sOk: S = "hi";
const sBad: S = 1;                                 // error[TK2322]: is not assignable to type 'string'

type Nev = string & never;                         // collapses to `never`
const nev: Nev = "hi";                             // error[TK2322]: not assignable to type 'never'

type A0 = { a: number };
type B0 = { b: string };
type Nest = (A0 & B0) & A0;                         // flattens + dedups to `A0 & B0`
const nestOk: Nest = { a: 1, b: "s" };
const nestBad: Nest = { a: 1 };                    // error[TK2741]: Property 'b' is missing
