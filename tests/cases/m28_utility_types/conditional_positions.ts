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

// Nested deferred nodes INSIDE composite conditional operands (object /
// tuple / array / function positions): the no-false-on-undecidable walk is
// DEEP, so the conditional stays deferred and rejects BOTH branch
// assignments. tsc resolves these structurally with mixed results (its
// eager-false shortcut fires for some shapes and not others — leader
// arbitration probe, review round 2); the "tsc-clean" lines are documented
// sound-direction over-reports (see tests/cases/README.md).

// Object-wrapped keyof (tsc: "y" — tsc-clean divergence on nk1).
type WK<T> = { v: keyof T } extends { v: "a" } ? "y" : "n";
const nk1: WK<{ a: 1 }> = "y"; // error[TK2322]
const nk2: WK<{ a: 1 }> = "n"; // error[TK2322]

// Function-return keyof (tsc: "y" — tsc-clean divergence on nf1).
type WF<T> = (() => keyof T) extends () => "a" ? "y" : "n";
const nf1: WF<{ a: 1 }> = "y"; // error[TK2322]
const nf2: WF<{ a: 1 }> = "n"; // error[TK2322]

// Object-wrapped intrinsic (tsc: "n" — tsc-clean divergence on no1).
type WO<S extends string> = { v: Uppercase<S> } extends { v: "ABC" } ? "y" : "n";
const no1: WO<"abc"> = "n"; // error[TK2322]
const no2: WO<"abc"> = "y"; // error[TK2322]

// Tuple-wrapped intrinsic (tsc: "y" — tsc-clean divergence on nt1).
type WT<S extends string> = [Uppercase<S>] extends ["ABC"] ? "y" : "n";
const nt1: WT<"abc"> = "y"; // error[TK2322]
const nt2: WT<"abc"> = "n"; // error[TK2322]

// Array-wrapped intrinsic (tsc: "n" — tsc-clean divergence on na1).
type WA<S extends string> = Uppercase<S>[] extends "ABC"[] ? "y" : "n";
const na1: WA<"abc"> = "n"; // error[TK2322]
const na2: WA<"abc"> = "y"; // error[TK2322]
