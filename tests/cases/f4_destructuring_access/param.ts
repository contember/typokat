// F4 / backlog 02 — access control through an OBJECT destructuring pattern in a
// FUNCTION PARAMETER, and through a subclass (private not accessible, protected
// is). Cross-checked against tsc 6.0.3.

class K {
  private priv: number = 1;
  protected prot: number = 2;
}

class C extends K {
  m2(): void {
    let { prot } = this; // ok — protected is accessible in a subclass
    let { priv } = this; // error[TK2341]: Property 'priv' is private
  }
}

function f({ priv, prot }: K): void {} // error[TK2341]: Property 'priv' is private | error[TK2445]: Property 'prot' is protected
