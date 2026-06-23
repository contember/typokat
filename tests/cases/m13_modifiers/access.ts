// M13 — access modifiers: access control for private / protected members.
// private: only within the declaring class. protected: declaring class + subclasses.

class Account {
  private balance: number;
  protected owner: string;
  constructor(b: number, o: string) {
    this.balance = b;
    this.owner = o;
  }
  check(): number {
    return this.balance; // ok — private accessible within the declaring class
  }
}

const acc = new Account(100, "a");
const b = acc.balance; // error[TK2341]: Property 'balance' is private
const o = acc.owner;   // error[TK2445]: Property 'owner' is protected

class Sub extends Account {
  constructor() {
    super(1, "a");
  }
  readOwner(): string {
    return this.owner; // ok — protected is accessible in a subclass
  }
  readBalance(): number {
    return this.balance; // error[TK2341]: Property 'balance' is private
    // ^ private is NOT accessible even in a subclass
  }
}
