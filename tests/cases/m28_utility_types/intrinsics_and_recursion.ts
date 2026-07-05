// M28 — string-manipulation INTRINSICS (Uppercase / Lowercase / Capitalize /
// Uncapitalize) evaluate natively on string literals and distribute over
// unions; on a pattern/`string` they stay symbolic and relate conservatively.
// Plus the backlog-12 recursion acceptance: a deep utility-style recursive
// alias exercises the shared memo/budget/work-stack guardrails.
// tsc 6.0.3 --strict cross-checked.

const u1: Uppercase<"abc"> = "ABC";
const u2: Uppercase<"abc"> = "abc"; // error[TK2322]
const l1: Lowercase<"ABC"> = "abc";
const l2: Lowercase<"ABC"> = "ABC"; // error[TK2322]
const c1: Capitalize<"abc"> = "Abc";
const c2: Capitalize<"abc"> = "abc"; // error[TK2322]
const n1: Uncapitalize<"ABC"> = "aBC";
const n2: Uncapitalize<"ABC"> = "ABC"; // error[TK2322]

// Distribution over unions.
const d1: Uppercase<"a" | "b"> = "A";
const d2: Uppercase<"a" | "b"> = "B";
const d3: Uppercase<"a" | "b"> = "x"; // error[TK2322]

// Composition with template construction.
type Greet<T extends string> = `HELLO ${Uppercase<T>}`;
const g1: Greet<"world"> = "HELLO WORLD";
const g2: Greet<"world"> = "HELLO world"; // error[TK2322]

// Recursive utility-style alias (deep but terminating): the shared
// memo/budget machinery must resolve it, not overflow or misreport.
type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> };
type Nest = { a: { b: { c: { d: number } } } };
const dp1: DeepPartial<Nest> = {};
const dp2: DeepPartial<Nest> = { a: { b: {} } };
const dp3: DeepPartial<Nest> = { a: { b: { c: { d: "s" } } } }; // error[TK2322]
