// M31 — a source intersection relates through its WHOLE merged object, not just one
// member: a member's property that conflicts with the target's optional property or
// index signature must be caught (no unsound some-member shortcut — `A <: B` does not
// imply `A & C <: B` when the target penalizes a present property).

type Config = { a: number } & { b: string };
declare const c: Config;

const okWide: { a: number; b: string } = c;        // clean — the merge covers exactly
const okSub: { b: string } = c;                     // clean — the merge covers; extra `a` ignored
const badOptional: { b: string; a?: string } = c;   // error[TK2322]
const badIndex: { [k: string]: string } = c;        // error[TK2322]
function want(p: { b: string; a?: string }): void {}
want(c);                                            // error[TK2345]
