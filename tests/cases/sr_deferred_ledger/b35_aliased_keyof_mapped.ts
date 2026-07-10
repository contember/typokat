// Deferred ledger / backlog 35 — an ALIASED keyof used as a NON-homomorphic mapped
// key source (`type Keys = keyof Obj; { [K in Keys]: ... }`) collapses the key
// source to `never`, and the alias `Keys` itself then resolves to `never` at its
// other use sites — an OVER-report rejecting valid code (tsc 6.0.3 --strict is
// clean on this whole file). This corpus stays DISABLED until backlog 35 ships;
// enabling it at HEAD shows spurious TK2322 ("not assignable to type 'never'") on
// the two assignments below. Found during the completeness-accounting sprint
// (2026-07-10, WU5); divergences.md `mapped/aliased-keyof-key-source`.

type Obj = { a: number; b: string };
type Keys = keyof Obj;
type M = { [K in Keys]: Obj[K] };

// witness (over-report): both lines are valid TS — typokat currently rejects them.
let k: Keys = "a";
let m: M = { a: 1, b: "x" };

// --- control: the inline (homomorphic) keyof form works in both. ---
type M2 = { [K in keyof Obj]: Obj[K] };
let m2: M2 = { a: 1, b: "x" };
