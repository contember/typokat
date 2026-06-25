// F3 / backlog 01 — an initializer-inferred field whose initializer references
// `this` must infer the REAL member type (not collapse to `any`). Regression
// witness from the WU1 adversarial review: collecting the member before `this`
// is bound froze such a field as `any`, silently swallowing downstream errors —
// a false negative (worse than the pre-fix over-report). Cross-checked: tsc 6.0.3
// infers `b`/`c` as `number`, so both reads below are TS2322.

class A {
  a = 1;
  b = this.a;   // backward this-reference → number
  m() { return 2; }
  c = this.m(); // this-method call → number
}

const x = new A();
const e1: string = x.b; // error[TK2322]: not assignable to type 'string'
const e2: string = x.c; // error[TK2322]: not assignable to type 'string'
