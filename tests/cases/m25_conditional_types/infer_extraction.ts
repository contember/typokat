// M25 — `infer` declarations: bound during the extends match, substituted into
// the true branch. Same-name infers in two covariant positions union their
// candidates. An `infer` name is only in scope in the true branch (TK2304 in
// the false branch — tsc behavior). Rest elements / rest params are out of
// scope (not in the type model yet). tsc 6.0.3 --strict cross-checked.

// Array element position.
type ElementOf<T> = T extends (infer U)[] ? U : never;
const i1: ElementOf<string[]> = "x";
const i1b: ElementOf<string[]> = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
const i1c: ElementOf<number> = 1; // error[TK2322]: not assignable to type 'never'

// Object property position.
type PropA<T> = T extends { a: infer U } ? U : never;
const i2: PropA<{ a: number; b: string }> = 1;
const i2b: PropA<{ a: number; b: string }> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Two covariant occurrences of the same infer name -> union of candidates.
type Both<T> = T extends { a: infer U; b: infer U } ? U : never;
const i3: Both<{ a: string; b: number }> = "x";
const i3b: Both<{ a: string; b: number }> = 1;
const i3c: Both<{ a: string; b: number }> = true; // error[TK2322]

// Distinct infer names extract independently.
type Pair<T> = T extends { a: infer A; b: infer B } ? [A, B] : never;
const i4: Pair<{ a: string; b: number }> = ["x", 1];
const i4b: Pair<{ a: string; b: number }> = [1, 1]; // error[TK2322]

// Fixed tuple positions (no rest elements).
type First<T> = T extends [infer H, infer R] ? H : never;
const i5: First<[string, number]> = "x";
const i5b: First<[string, number]> = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Function return position.
type Ret<T> = T extends () => infer R ? R : never;
const i6: Ret<() => number> = 1;
const i6b: Ret<() => number> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Function parameter position.
type ParamOf<T> = T extends (a: infer P) => unknown ? P : never;
const i7: ParamOf<(a: string) => void> = "x";
const i7b: ParamOf<(a: string) => void> = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'

// An infer name is not in scope in the false branch.
type BadScope<T> = T extends (infer U)[] ? U : U; // error[TK2304]: Cannot find name 'U'
