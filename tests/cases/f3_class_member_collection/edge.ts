// F3 / backlog 01 — two boundary cases:
//  - an inferred field may still carry a modifier (`private g = 2`): the member is
//    collected AND the access-control check runs on it (TK2341, not TK2339).
//  - a constructor parameter WITHOUT a modifier stays a plain parameter — it does
//    NOT become a member, so accessing it is correctly TK2339.
// Cross-checked against tsc 6.0.3 (TS2341 / TS2339).

class E {
  private g = 2;                 // inferred type + private modifier → a private member
  constructor(plain: number) {}  // no modifier → NOT a member
}

const e = new E(1);
e.g;     // error[TK2341]: Property 'g' is private
e.plain; // error[TK2339]: Property 'plain' does not exist
