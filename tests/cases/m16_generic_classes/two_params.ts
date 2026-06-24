// M16 — multi-parameter generic class; each type parameter substitutes independently.

class Pair<A, B> {
  first: A;
  second: B;
  constructor(a: A, b: B) {
    this.first = a;
    this.second = b;
  }
}

const p = new Pair<number, string>(1, "x");
const a: number = p.first;   // ok
const b: string = p.second;  // ok
const bad: string = p.first; // error[TK2322]

const e = new Pair<number, string>("x", "y"); // error[TK2345]
// ^ only the first arg mismatches: "x" (string) is not assignable to number; the
//   second arg "y" (string) matches string. A single mismatch, so typokat and tsc
//   agree exactly (typokat reports one error per mismatched argument — see README).

const inferred = new Pair(1, "x"); // infer A = number, B = string
const c: string = inferred.second; // ok
const d: number = inferred.second; // error[TK2322]
