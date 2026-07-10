// Deferred ledger / backlog 60 — fresh object literals against UNION targets skip
// both the excess-property check and part of the assignability check, so several
// tsc errors are silently dropped. This corpus stays DISABLED beyond this sprint
// (until backlog 60 models tsc's union rules for fresh literals). Cross-checked
// vs tsc 6.0.3 --strict. Asserted code-only (object/union targets).

interface Sa {
  a: string;
}
interface Sb {
  b: number;
}

// excess property against a multi-shape union — tsc: TS2353.
const excessUnion: Sa | Sb = { a: "s", extra: 1 }; // error[TK2353]

// excess property against `| null` (only `| undefined` is unwrapped today) —
// tsc: TS2353.
const excessNull: Sa | null = { a: "s", extra: 1 }; // error[TK2353]

// wrong-typed known property vacuously "absorbed" by an optional-member union
// member — tsc: TS2322.
const optUnion: { a?: number } | { b?: string } = { a: "s" }; // error[TK2322]

// --- controls: a fresh literal that exactly matches one union member stays
// clean; a discriminated union still narrows. ---
const okMatch: Sa | Sb = { a: "s" };
const okUndef: Sa | undefined = { a: "s" };
