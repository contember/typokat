// F4 / backlog 02 — private/protected access control runs through an OBJECT
// destructuring pattern in a variable declaration. Before this fix the bindings
// were unchecked, so a destructured private/protected member was silently allowed
// (false negative). Members are annotated so F3's collection isn't a prerequisite.
// Cross-checked against tsc 6.0.3 (TS2341 / TS2445).

class K {
  private priv: number = 1;
  protected prot: number = 2;
  pub: number = 3;
  m(): void {
    let { priv, prot } = this;   // ok — inside the declaring class
    let { priv: a } = new K();   // ok — same class
  }
}

const k = new K();
let { pub } = k;                 // ok — public
let { priv } = k;                // error[TK2341]: Property 'priv' is private
let { prot } = k;                // error[TK2445]: Property 'prot' is protected
let { priv: rp } = k;            // error[TK2341]: Property 'priv' is private
let { priv: x, prot: y } = k;    // error[TK2341]: Property 'priv' is private | error[TK2445]: Property 'prot' is protected
