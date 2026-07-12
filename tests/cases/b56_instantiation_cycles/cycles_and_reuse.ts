// Backlog 56 — genuine direct and mutual instantiation cycles diagnose instead
// of degrading to the permissive error type. Cross-checked with tsc 6.0.3.

type Loop<T> = T extends string ? Loop<T> : never;
const direct: Loop<string> = 42; // error[TK2589]

type Ping<T> = T extends string ? Pong<T> : never;
type Pong<T> = T extends string ? Ping<T> : never;
const mutual: Ping<string> = 42; // error[TK2589]

// A non-cyclic conditional evaluated after both failures remains exact.
type Fine<T> = T extends string ? "s" : "n";
const fineOk: Fine<string> = "s";
const fineBad: Fine<string> = "n"; // error[TK2322]
