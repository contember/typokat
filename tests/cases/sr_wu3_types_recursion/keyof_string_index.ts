// WU3 / finding 10 — `keyof { [k: string]: T }` yields `string`, but tsc yields
// `string | number` (a string index signature implicitly covers numeric keys).
// The missing `number` member flips an assignability verdict in BOTH directions.
// DISABLED at HEAD; enabling exposes the missing error (line marked TK2322) and
// the over-report (the `fromNum` line, clean under tsc, errors at HEAD).
// Cross-checked vs tsc 6.0.3 --strict.

type S = { [k: string]: number };
type K = keyof S; // tsc: string | number

declare const k: K;

// witness (assignable-to): `K` includes `number`, not assignable to `string` —
// tsc: TS2322. typokat (K = string) misses this today.
const toStr: string = k; // error[TK2322]: not assignable to type 'string'

// witness (assignable-from): `0` is a `number`, assignable to `K` — tsc: clean.
// typokat (K = string) OVER-reports today; must be clean after the fix.
const fromNum: K = 0;

// control: assignable to the full key domain either way.
const toBoth: string | number = k;

// --- control: a numeric index signature's `keyof` is `number`, already correct. ---
type SN = { [i: number]: string };
type KN = keyof SN;
declare const kn: KN;
const numKey: number = kn;
