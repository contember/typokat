// M31 — object intersection `A & B`.
// Target rule: a value must satisfy EVERY member. Source rule: `A & B` satisfies a
// target its MERGED property set covers (a single member, or several members together).
// Member access resolves against the merged apparent object.

type AB = { a: number } & { b: string };

const ok1: AB = { a: 1, b: "s" };
const missing: AB = { a: 1 };                     // error[TK2741]: Property 'b' is missing
const wrongValue: AB = { a: 1, b: 2 };            // error[TK2322]: is not assignable to type 'string'

function reads(x: AB) {
  const b: string = x.b;                          // a member read, correctly typed
  const bad: string = x.a;                        // error[TK2322]: is not assignable to type 'string'
  const c = x.c;                                  // error[TK2339]: Property 'c' does not exist
}

function toOneMember(x: AB): { a: number } { return x; }
function toMerge(x: AB): { a: number; b: string } { return x; }
function fromPartial(x: { a: number }): AB { return x; }   // error[TK2741]: Property 'b' is missing
