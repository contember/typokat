// Backlog 56 — a cyclic instantiation must not poison a terminating sibling of
// the same conditional alias in the pass-wide evaluator memo.

type MaybeLoop<T> = T extends string ? MaybeLoop<T> : "done";

const cycleFirst: MaybeLoop<string> = 42; // error[TK2589]
const terminatingAfter: MaybeLoop<number> = "wrong"; // error[TK2322]
