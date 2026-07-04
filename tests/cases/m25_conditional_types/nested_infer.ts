// M25 — a nested conditional that references an OUTER conditional's infer
// binder is NOT evaluated (cross-binder de Bruijn shifting is deferred —
// backlog 26). The whole poisoned conditional stays a deferred node and
// relates conservatively: nothing assignable into it, both-branches-or-No as
// a source. tsc RESOLVES these — every marker below on a line tsc leaves
// clean is a documented OVER-REPORT divergence (safe direction), see
// tests/cases/README.md. No spurious TK2304 for the outer-binder reference.

// Outer infer used as the nested conditional's CHECK type (probe p1 shape).
type OuterCheck<T> = T extends { a: infer U } ? (U extends string ? "s" : "n") : never;
// tsc: resolves to "s", both lines clean — typokat: conservative deferral.
const x1: OuterCheck<{ a: string }> = "s"; // error[TK2322]
declare const oc: OuterCheck<{ a: string }>;
const x2: "s" = oc; // error[TK2322]

// Outer infer used inside the nested conditional's BRANCH (probe p2 shape) —
// same conservative deferral, and no TK2304 at the alias.
type OuterBranch<T> = T extends { a: infer U } ? (T extends { b: number } ? U : "z") : never;
declare const ob: OuterBranch<{ a: string; b: number }>;
const x3: string = ob; // error[TK2322]

// Regression guard: a nested conditional whose infers are its OWN still
// evaluates normally (no cross-binder reference — T is a declaration param,
// rewritten by ordinary substitution).
type OwnInfer<T> = T extends unknown[] ? (T extends (infer W)[] ? W : never) : "no";
declare const own: OwnInfer<string[]>;
const y1: string = own;
const y2: number = own; // error[TK2322]: Type 'string' is not assignable to type 'number'
