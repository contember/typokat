// M28 — TK2344 on utility/intrinsic type ARGUMENTS (review round 3): a
// decidable argument composition EVALUATES before the constraint check
// (never gated on its raw written form); a still-deferred argument checks
// conservatively — tsc-exact on unprovable shapes, a documented over-report
// on provable ones (tsc's constraint approximation — backlog 37).
// tsc 6.0.3 --strict cross-checked; the one divergence is annotated.

type P = { a: number; b?: string };

// Decidable compositions: evaluate, then check.
type B1 = Pick<P, Exclude<"a" | 1, "a">>; // error[TK2344]
type B2 = Pick<P, Exclude<"a" | "b" | 1, 1>>;
type A1 = Uppercase<Exclude<"a" | 1, "a">>; // error[TK2344]
type A2 = Uppercase<Exclude<"a" | 1, 1>>;

// Unprovable deferred argument: tsc also errors at the declaration.
type MyExclude<T, U> = T extends U ? never : T;
type S1<K> = Uppercase<MyExclude<K, "a">>; // error[TK2344]

// Provable deferred argument: tsc proves Extract<K, string> <: string and
// stays clean — typokat checks conservatively (tsc-clean divergence).
type UE<K> = Uppercase<Extract<K, string>>; // error[TK2344]

// Constraint-side keyof gate untouched: the canonical Omit idiom stays
// clean at declaration (constraint `keyof T` is deferred; the check lands
// at concrete instantiation).
type MyOmit<T, K extends keyof T> = Pick<T, Exclude<keyof T, K>>;
const mo1: MyOmit<P, "a"> = { b: "s" };
const mo2: MyOmit<P, "a"> = { a: 1 }; // error[TK2353]
