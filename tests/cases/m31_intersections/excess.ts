// M31 — excess-property checking against an intersection target uses the MERGED key
// set: a property present in ANY member is allowed; only a property in NO member is
// excess. The clean line guards against spuriously flagging a member-covered key.

type AB = { a: number } & { b: string };
const ok: AB = { a: 1, b: "s" };                   // `a` and `b` each covered by one member
const excess: AB = { a: 1, b: "s", c: true };      // error[TK2353]: Object literal may only specify known properties
