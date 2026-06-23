// M5 — recursive & mutually recursive types. These MUST terminate: relating a recursive
// type to itself re-enters the same (src, tgt, kind) and relies on the assume-true cycle
// stack (§6.3). Without it, the relation engine loops forever.

interface List { head: number; tail: List | null; }

const l1: List = { head: 1, tail: null };                    // ok
const l2: List = { head: 1, tail: { head: 2, tail: null } }; // ok — nested

let a: List = l1;
let b: List = a; // ok — relating List <: List re-enters on 'tail' (the cycle)

// deep error: must recurse through the cycle and still report the leaf mismatch
const bad: List = { head: 1, tail: { head: "x", tail: null } }; // error[TK2322]

// mutually recursive
interface Ping { pong: Pong | null; }
interface Pong { ping: Ping | null; }

const p: Ping = { pong: { ping: null } }; // ok
const q: Pong = { ping: { pong: null } }; // ok
