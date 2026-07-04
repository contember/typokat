// M25 — recursive conditionals: terminating recursion resolves (memoized,
// work-stack — no host-stack overflow); a directly self-referential alias is
// TK2456 at its declaration; runaway growth trips the instantiation depth
// limit (TK2589). tsc 6.0.3 --strict cross-checked. Span divergence
// (documented): tsc attributes TS2589 to the recursive reference inside the
// alias body; typokat reports it at the annotation that demanded evaluation.

// Terminating object-descent recursion.
type Unwrap<T> = T extends { v: infer U } ? Unwrap<U> : T;
const w1: Unwrap<{ v: { v: { v: number } } }> = 1;
const w2: Unwrap<{ v: { v: { v: number } } }> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Deeper (16 levels) — still resolves; guards against an over-tight limit.
type Deep16 = { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: { v: number } } } } } } } } } } } } } } } };
const w3: Unwrap<Deep16> = 1;
const w4: Unwrap<Deep16> = "s"; // error[TK2322]: Type 'string' is not assignable to type 'number'

// A directly self-referential alias cannot resolve.
type Self = Self extends string ? 1 : 2; // error[TK2456]: circularly references itself
declare const s1: Self;

// Runaway growth: every step wraps the check type, always matches.
type Inf<T> = T extends {} ? Inf<{ v: T }> : never;
declare const x1: Inf<{}>; // error[TK2589]: excessively deep
