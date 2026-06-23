// M13 — a class with a private/protected member is NOMINAL: a structurally
// identical object (or other class) is NOT assignable to it; only the class's own
// instances are. (The private member must originate from the same declaration.)

class Secret {
  private x: number;
  constructor() {
    this.x = 1;
  }
}

const a: Secret = new Secret(); // ok — same class
const b: Secret = { x: 1 };     // error[TK2322]
// ^ object literal lacks Secret's private member

class Other {
  private x: number;
  constructor() {
    this.x = 1;
  }
}

const c: Secret = new Other(); // error[TK2322]
// ^ Other.x is a different private member than Secret.x
