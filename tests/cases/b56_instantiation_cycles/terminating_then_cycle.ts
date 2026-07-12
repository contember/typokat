// Backlog 56 — reversing query order preserves both the exact terminating result
// and the later cycle diagnostic.

type MaybeLoop<T> = T extends string ? MaybeLoop<T> : "done";

const terminatingFirst: MaybeLoop<number> = "wrong"; // error[TK2322]
const cycleAfter: MaybeLoop<string> = 42; // error[TK2589]
