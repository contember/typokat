// M0 — intrinsic target types with literal sources.
// any: accepts anything. unknown: everything is assignable TO it.
// never: nothing (but never) is assignable to it. void: only undefined.

const a_any: any = 1;
const a_unknown: unknown = 1;
const a_void_ok: void = undefined;

const bad_never: never = 1; // error[TK2322]: Type 'number' is not assignable to type 'never'
const bad_void: void = 1;   // error[TK2322]: Type 'number' is not assignable to type 'void'
