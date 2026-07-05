// M28 — deferred nodes (string intrinsics, keyof) appearing as a
// conditional's check/extends OPERANDS must demand-evaluate before the
// extends test runs; a concrete-argument intrinsic or keyof must never
// force the false branch (review round 1, HIGH-1/HIGH-2).
// tsc 6.0.3 --strict cross-checked.

// Intrinsic in the check operand.
type IsUpper<S extends string> = Uppercase<S> extends S ? "yes" : "no";
const iu1: IsUpper<"ABC"> = "yes";
const iu2: IsUpper<"ABC"> = "no"; // error[TK2322]
const iu3: IsUpper<"abc"> = "no";
const iu4: IsUpper<"abc"> = "yes"; // error[TK2322]

// Intrinsic in the extends operand.
type Norm<S extends string> = S extends Uppercase<S> ? "same" : "diff";
const nm1: Norm<"ABC"> = "same";
const nm2: Norm<"ABC"> = "diff"; // error[TK2322]
const nm3: Norm<"abc"> = "diff";
const nm4: Norm<"abc"> = "same"; // error[TK2322]

// Deferred keyof in the extends operand.
type Has<T, K> = K extends keyof T ? "y" : "n";
const h1: Has<{ a: 1 }, "a"> = "y";
const h2: Has<{ a: 1 }, "a"> = "n"; // error[TK2322]
const h3: Has<{ a: 1 }, "z"> = "n";
const h4: Has<{ a: 1 }, "z"> = "y"; // error[TK2322]

// Deferred keyof in the check operand (non-distributive: not a naked param).
type KC<T> = keyof T extends "a" ? "only-a" : "more";
const kc1: KC<{ a: 1 }> = "only-a";
const kc2: KC<{ a: 1 }> = "more"; // error[TK2322]
const kc3: KC<{ a: 1; b: 2 }> = "more";
const kc4: KC<{ a: 1; b: 2 }> = "only-a"; // error[TK2322]
