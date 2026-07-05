// M27 — `infer` inside template patterns (conditional extends): holes
// separated by NON-EMPTY literal text match non-greedily on the anchors —
// prefix, suffix, and middle extraction are in scope. ADJACENT holes (no
// literal separator) are out of scope: the conditional is poisoned and stays
// deferred (conservative over-report; tsc resolves them — documented
// divergence, tests/cases/README.md). tsc 6.0.3 --strict cross-checked.

// Suffix extraction (literal prefix anchor).
type After<T> = T extends `a-${infer R}` ? R : never;
const a1: After<"a-foo"> = "foo";
const a2: After<"a-foo"> = "bar"; // error[TK2322]: Type '"bar"' is not assignable to type '"foo"'
const a3: After<"b-foo"> = nv1; // no match -> never
declare const nv1: never;
const a4: After<"b-foo"> = "x"; // error[TK2322]: not assignable to type 'never'

// Prefix extraction (literal suffix anchor).
type Before<T> = T extends `${infer L}-z` ? L : never;
const b1: Before<"foo-z"> = "foo";
const b2: Before<"foo-z"> = "bar"; // error[TK2322]

// Middle extraction (two infers, literal separator between them).
type Split<T> = T extends `${infer L}:${infer R}` ? [L, R] : never;
const m1: Split<"k:v"> = ["k", "v"];
const m2: Split<"k:v"> = ["k", "k"]; // error[TK2322]

// Non-greedy: the FIRST separator anchors.
const m3: Split<"a:b:c"> = ["a", "b:c"];
const m4: Split<"a:b:c"> = ["a", "c"]; // error[TK2322]

// Distribution composes with template extraction.
const d1: After<"a-x" | "a-y"> = "x";
const d2: After<"a-x" | "a-y"> = "y";
const d3: After<"a-x" | "a-y"> = "z"; // error[TK2322]

// ADJACENT infer holes: poisoned, stays deferred (tsc resolves A="a", B="bc"
// — documented over-report divergence; both directions conservative).
type Adj<T> = T extends `${infer A}${infer B}` ? A : never;
declare const adj: Adj<"abc">;
const p1: string = adj; // error[TK2322]
