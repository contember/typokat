// M27 — template literal type construction: all-literal holes collapse to a
// string literal; union holes distribute (cartesian product across holes);
// boolean expands to true|false; never short-circuits to never; number holes
// stay patterns. tsc 6.0.3 --strict cross-checked.

type Exact = `a-${"b" | "c"}`;
const e1: Exact = "a-b";
const e2: Exact = "a-c";
const e3: Exact = "a-d"; // error[TK2322]

// Cartesian product across two union holes.
type Two = `${"a" | "b"}-${"1" | "2"}`;
const t1: Two = "a-1";
const t2: Two = "b-2";
const t3: Two = "a-3"; // error[TK2322]

// boolean hole expands to "false" | "true".
type BH = `is:${boolean}`;
const b1: BH = "is:true";
const b2: BH = "is:false";
const b3: BH = "is:maybe"; // error[TK2322]

// never hole -> the whole template is never.
type NH = `x${never}`;
declare const nv: never;
const nh1: NH = nv;
const nh2: NH = "x"; // error[TK2322]: not assignable to type 'never'

// Numeric literal holes stringify.
type Ver = `v${1 | 2}`;
const v1: Ver = "v1";
const v2: Ver = "v2";
const v3: Ver = "v3"; // error[TK2322]

// Generic concatenation collapses at instantiation.
type Concat<A extends string, B extends string> = `${A}${B}`;
const c1: Concat<"foo", "bar"> = "foobar";
const c2: Concat<"foo", "bar"> = "foobaz"; // error[TK2322]: Type '"foobaz"' is not assignable to type '"foobar"'
