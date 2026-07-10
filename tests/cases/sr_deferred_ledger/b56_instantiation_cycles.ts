// Deferred ledger / backlog 56 — an in-flight instantiation cycle short-circuits
// to the error type with no flag and no diagnostic, so a genuine cycle produces
// ZERO diagnostics and every use is silently accepted (tsc: TS2589). Both the
// direct-recursive and mutual-recursive conditional-alias forms are affected.
// This corpus stays DISABLED beyond this sprint (until backlog 56 surfaces the
// cycle). Cross-checked vs tsc 6.0.3 --strict. Asserted code-only.

// direct cycle — tsc: TS2589 on the annotation.
type Loop<T> = T extends string ? Loop<T> : never;
const direct: Loop<string> = 42; // error[TK2589]

// mutual cycle — tsc: TS2589.
type Ping<T> = T extends string ? Pong<T> : never;
type Pong<T> = T extends string ? Ping<T> : never;
const mutual: Ping<string> = 42; // error[TK2589]

// --- control: a legitimate (non-cyclic) conditional alias resolves and reuses
// cleanly — no poisoning from the cycles above. ---
type Fine<T> = T extends string ? "s" : "n";
const okS: Fine<string> = "s";
