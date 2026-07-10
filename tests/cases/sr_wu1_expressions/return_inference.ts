// WU1 / finding 2 — an inferred function return type is taken from the FIRST
// value return only (statements.rs:159); later returns are type-checked but do
// not widen the inferred return. So a function that returns `1` then `"x"`
// infers `number`, and a downstream `const n: number = f()` misses the error
// that the real return type `string | number` would raise. DISABLED at HEAD;
// enabling exposes the dropped TK2322. Cross-checked vs tsc 6.0.3 --strict.

function mixed(cond: boolean) {
  if (cond) return 1;
  return "x";
}
// tsc: `mixed` returns `string | number`; `string` is not assignable to number.
const n: number = mixed(true); // error[TK2322]: not assignable to type 'number'

// Order must not matter: the string return comes first here.
function mixedReordered(cond: boolean) {
  if (cond) return "x";
  return 1;
}
const m: number = mixedReordered(true); // error[TK2322]: not assignable to type 'number'

// --- controls: single-return and same-type-return inference stay correct. ---
function one() {
  return 1;
}
const okOne: number = one();

function twoSame(cond: boolean) {
  if (cond) return 1;
  return 2;
}
const okTwo: number = twoSame(true);
